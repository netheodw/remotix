use std::net::IpAddr;

use crate::error::{AppError, AppResult};

pub fn validate_hostname(value: &str) -> AppResult<()> {
    let value = value.trim();
    if value.is_empty() {
        return Err(AppError::Validation("hostname is required".into()));
    }
    if value.parse::<IpAddr>().is_ok() {
        return Ok(());
    }
    if value.len() > 253 {
        return Err(AppError::Validation("hostname is too long".into()));
    }
    let labels: Vec<&str> = value.split('.').collect();
    let valid = labels.iter().all(|label| {
        !label.is_empty()
            && label.len() <= 63
            && !label.starts_with('-')
            && !label.ends_with('-')
            && label.chars().all(|c| c.is_ascii_alphanumeric() || c == '-')
    });
    if !valid {
        return Err(AppError::Validation(
            "hostname must be a valid DNS name or IP address".into(),
        ));
    }
    Ok(())
}

pub fn validate_port(port: u16) -> AppResult<()> {
    if port == 0 {
        return Err(AppError::Validation("port must be between 1 and 65535".into()));
    }
    Ok(())
}

pub fn validate_username(value: &str) -> AppResult<()> {
    if value.trim().is_empty() {
        return Err(AppError::Validation("username is required".into()));
    }
    if value.len() > 256 {
        return Err(AppError::Validation("username is too long".into()));
    }
    Ok(())
}

pub fn validate_display_name(value: &str) -> AppResult<()> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        return Err(AppError::Validation("name is required".into()));
    }
    if trimmed.len() > 200 {
        return Err(AppError::Validation("name is too long".into()));
    }
    Ok(())
}

pub fn validate_bind_address(value: &str) -> AppResult<()> {
    if value.parse::<IpAddr>().is_err() {
        return Err(AppError::Validation(
            "bind address must be a valid IPv4 or IPv6 address".into(),
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_valid_hostnames_and_ips() {
        assert!(validate_hostname("db.internal").is_ok());
        assert!(validate_hostname("192.168.1.10").is_ok());
        assert!(validate_hostname("::1").is_ok());
        assert!(validate_hostname("localhost").is_ok());
    }

    #[test]
    fn rejects_invalid_hostnames() {
        assert!(validate_hostname("").is_err());
        assert!(validate_hostname("-bad.example.com").is_err());
        assert!(validate_hostname("has a space").is_err());
    }

    #[test]
    fn rejects_out_of_range_ports() {
        assert!(validate_port(0).is_err());
        assert!(validate_port(22).is_ok());
        assert!(validate_port(65535).is_ok());
    }
}
