use std::io::{Read, Write};
use std::net::TcpStream;
use std::sync::mpsc;
use std::time::Duration;

const HANDSHAKE_TIMEOUT: Duration = Duration::from_secs(10);

pub enum HandshakeResult {
    Ready { stream: TcpStream, host: String, port: u16 },
    Failed,
}

/// Performs a minimal RFC 1928 SOCKS5 handshake (no authentication,
/// CONNECT command only) on a freshly-accepted connection, then reports
/// the requested destination back to the forward's main loop.
///
/// The success reply is sent optimistically once a well-formed CONNECT
/// request is parsed, before the SSH `direct-tcpip` channel is actually
/// opened (that only happens on the forward's single session-owning
/// thread). If the upstream channel then fails to open, the client simply
/// sees the connection drop rather than a SOCKS-level error reply - an
/// accepted simplification for this MVP, documented in the README.
pub fn negotiate(mut stream: TcpStream, tx: mpsc::Sender<HandshakeResult>) {
    let _ = stream.set_read_timeout(Some(HANDSHAKE_TIMEOUT));
    let _ = stream.set_write_timeout(Some(HANDSHAKE_TIMEOUT));

    match try_negotiate(&mut stream) {
        Some((host, port)) => {
            let _ = tx.send(HandshakeResult::Ready { stream, host, port });
        }
        None => {
            let _ = tx.send(HandshakeResult::Failed);
        }
    }
}

fn try_negotiate(stream: &mut TcpStream) -> Option<(String, u16)> {
    let mut header = [0u8; 2];
    stream.read_exact(&mut header).ok()?;
    if header[0] != 0x05 {
        return None;
    }
    let nmethods = header[1] as usize;
    let mut methods = vec![0u8; nmethods];
    stream.read_exact(&mut methods).ok()?;
    if !methods.contains(&0x00) {
        let _ = stream.write_all(&[0x05, 0xFF]);
        return None;
    }
    stream.write_all(&[0x05, 0x00]).ok()?;

    let mut request = [0u8; 4];
    stream.read_exact(&mut request).ok()?;
    let (version, cmd, atyp) = (request[0], request[1], request[3]);
    if version != 0x05 || cmd != 0x01 {
        let _ = stream.write_all(&[0x05, 0x07, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
        return None;
    }

    let host = match atyp {
        0x01 => {
            let mut addr = [0u8; 4];
            stream.read_exact(&mut addr).ok()?;
            format!("{}.{}.{}.{}", addr[0], addr[1], addr[2], addr[3])
        }
        0x03 => {
            let mut len = [0u8; 1];
            stream.read_exact(&mut len).ok()?;
            let mut name = vec![0u8; len[0] as usize];
            stream.read_exact(&mut name).ok()?;
            String::from_utf8(name).ok()?
        }
        0x04 => {
            let mut addr = [0u8; 16];
            stream.read_exact(&mut addr).ok()?;
            std::net::Ipv6Addr::from(addr).to_string()
        }
        _ => {
            let _ = stream.write_all(&[0x05, 0x08, 0x00, 0x01, 0, 0, 0, 0, 0, 0]);
            return None;
        }
    };

    let mut port_bytes = [0u8; 2];
    stream.read_exact(&mut port_bytes).ok()?;
    let port = u16::from_be_bytes(port_bytes);

    stream.write_all(&[0x05, 0x00, 0x00, 0x01, 0, 0, 0, 0, 0, 0]).ok()?;
    stream.set_read_timeout(None).ok()?;
    stream.set_write_timeout(None).ok()?;
    Some((host, port))
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::net::TcpListener;

    fn connected_pair() -> (TcpStream, TcpStream) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let addr = listener.local_addr().unwrap();
        let client = TcpStream::connect(addr).unwrap();
        let (server, _) = listener.accept().unwrap();
        (client, server)
    }

    #[test]
    fn negotiates_domain_name_connect_request() {
        let (mut client, server) = connected_pair();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || negotiate(server, tx));

        // Greeting: version 5, one method, no-auth.
        client.write_all(&[0x05, 0x01, 0x00]).unwrap();
        let mut greeting_reply = [0u8; 2];
        client.read_exact(&mut greeting_reply).unwrap();
        assert_eq!(greeting_reply, [0x05, 0x00]);

        // CONNECT request to a domain name target.
        let domain = b"example.com";
        let mut request = vec![0x05, 0x01, 0x00, 0x03, domain.len() as u8];
        request.extend_from_slice(domain);
        request.extend_from_slice(&443u16.to_be_bytes());
        client.write_all(&request).unwrap();

        let mut reply = [0u8; 10];
        client.read_exact(&mut reply).unwrap();
        assert_eq!(reply[..2], [0x05, 0x00]);

        handle.join().unwrap();
        match rx.recv().unwrap() {
            HandshakeResult::Ready { host, port, .. } => {
                assert_eq!(host, "example.com");
                assert_eq!(port, 443);
            }
            HandshakeResult::Failed => panic!("expected a successful handshake"),
        }
    }

    #[test]
    fn rejects_non_connect_command() {
        let (mut client, server) = connected_pair();
        let (tx, rx) = mpsc::channel();
        let handle = std::thread::spawn(move || negotiate(server, tx));

        client.write_all(&[0x05, 0x01, 0x00]).unwrap();
        let mut greeting_reply = [0u8; 2];
        client.read_exact(&mut greeting_reply).unwrap();

        // BIND (0x02) instead of CONNECT (0x01) must be rejected.
        client
            .write_all(&[0x05, 0x02, 0x00, 0x01, 127, 0, 0, 1, 0, 80])
            .unwrap();

        handle.join().unwrap();
        assert!(matches!(rx.recv().unwrap(), HandshakeResult::Failed));
    }
}
