# Remotix

Remotix is a local-first, Termius-style SSH client and tunnel manager for Windows and Linux. It is a native desktop
application, not a hosted service: every credential, connection profile, and known-host record lives in a single
encrypted SQLite database on your own machine. There is no account, no cloud sync, no telemetry, and no server
component of any kind - the only network traffic Remotix generates is the direct SSH connection you ask it to make.

## Download

Windows installers are on the **[Releases page](https://github.com/netheodw/remotix/releases/latest)**: download
`Remotix_<version>_x64-setup.exe` and run it. Upgrading keeps your data. The installer is not code-signed, so
SmartScreen may ask you to confirm.

## Privacy and local-only guarantees

- **No cloud account.** The vault is protected by a master password you choose; Remotix never transmits it or any
  derived key anywhere.
- **No sync service, no hosted backend, no REST/GraphQL API, no local HTTP server.** The UI talks to the Rust core
  exclusively through Tauri's in-process IPC (typed commands and streaming channels).
- **No telemetry.** Nothing is collected, batched, or phoned home - not even anonymized usage counts.
- **Secrets are encrypted at rest.** Passwords, private keys, and key passphrases are individually encrypted with
  AES-256-GCM under a key derived from your master password with Argon2id. They are never written to disk, logs, or
  the frontend's own state in plaintext.
- **You own the data file.** Everything lives in one SQLite file under your OS's per-user application-data directory
  (see [Database & vault location](#database-and-vault-location)). Back it up, move it, or delete it like any other
  local file.

## Feature overview (this milestone)

- Encrypted local vault with a master password, Argon2id key derivation, and AES-256-GCM secret storage.
- Nested Terminal Groups and SSH profiles (password, private key, or private key + passphrase auth) with create,
  edit, duplicate, move, search, test-connection, connect, and delete.
- First-use SSH host-key trust with SHA-256 fingerprint display, persistent known-hosts storage, and a hard block
  (never a silent accept) on a changed key.
- Multiple interactive SSH terminal tabs built on `@xterm/xterm`, with resize, reconnect, and per-tab connection
  state.
- Per-profile command history: every command run in a terminal is stored locally against its SSH profile, and Up /
  Down replay it, so the history is still there after a reconnect or an app restart - unlike the remote shell's own
  history, which starts empty on each new session. See [Command history](#command-history).
- Import from a Termius JSON export: host names, addresses, ports, usernames, credentials (passwords or private
  keys), and port forwards, with a preview that flags invalid hosts, hosts that already exist, and listener
  conflicts. The export is read only by the Rust core, so its plaintext credentials never reach the UI.
- Nested Port Forward Groups and forwarding rules, independent of the terminal group tree.
- Re-file SSH profiles and port forwards straight from their lists: tick rows and pick a group from
  **Move to group...**, or drag rows (a single row or the whole selection) onto a group in the tree.
- Local (`-L`) and remote (`-R`) TCP forwarding with start/stop, live status, safe `127.0.0.1` defaults, and an
  explicit warning before binding beyond loopback.
- A standards-compliant local SOCKS5 listener for dynamic (`-D`) forwarding (see
  [Dynamic forwarding limitations](#dynamic-forwarding-limitations) below).
- Bounded exponential-backoff reconnect for forwards, which intentionally does **not** auto-retry after a host-key
  mismatch or an authentication failure.
- An SFTP file browser (the **Files** screen): dual local/remote panes, directory navigation on both sides,
  upload/download through a cancelable transfer queue, remote delete, and remote edit - a file is downloaded to a
  scratch directory, opened in your OS editor, and re-uploaded automatically when you save it.

## Prerequisites

### Both platforms

- [Node.js](https://nodejs.org/) 20+ and npm
- [Rust](https://www.rust-lang.org/tools/install) (stable toolchain)
- The [Tauri prerequisites](https://tauri.app/start/prerequisites/) for your OS

### Windows

- Visual Studio 2022 Build Tools (or Visual Studio 2022) with the **Desktop development with C++** workload - this
  provides the MSVC linker and the CRT/Windows SDK libraries the Rust toolchain needs.
- [Strawberry Perl](https://strawberryperl.com/) - required once, only to compile OpenSSL from source the first time
  you build (`libssh2`/`ssh2` depend on it via the `vendored-openssl` feature). Git for Windows' bundled Perl is not
  sufficient; it is missing several core modules `Configure` needs.
- WebView2 Runtime (preinstalled on current Windows 10/11).

> **If a Windows machine has more than one Visual Studio installation**, `cargo build` can pick the wrong one and
> fail while linking a build script with `LNK1104: cannot open file 'msvcrt.lib'`. `src-tauri/.cargo/config.toml`
> pins the linker and its INCLUDE/LIB paths to work around this on the machine this project was developed on; if you
> hit this error on a different machine, either delete that file (if your machine has one clean VS install) or edit
> the paths in it to match your BuildTools/VS installation, or open a "Developer Command Prompt for VS" and build
> from that shell instead.

### Linux

- Standard build essentials (`build-essential`/`gcc`, `pkg-config`) and the GTK/WebKitGTK development headers Tauri
  needs - see the Tauri prerequisites link above for the exact package list per distribution.
- `perl` and `make` (used the same way as on Windows, to build OpenSSL from source via `vendored-openssl`).

## Development

```bash
npm install
npm run tauri dev
```

This starts the Vite dev server for the React/TypeScript frontend and launches the Tauri window with the Rust core
attached, with hot reload on the frontend side.

## Tests

Frontend:

```bash
npx tsc --noEmit       # type-check
npx vitest run         # unit tests (see src/**/*.test.ts, where present)
```

Rust core:

```bash
cd src-tauri
cargo test
```

Rust tests cover: vault setup/unlock/lock and the rate limiter, encryption round-trips and nonce uniqueness,
migrations, group-tree CRUD and cycle prevention, SSH profile CRUD and secret handling, port-forward validation
(duplicate local listeners, missing target fields, unknown profile references), known-host first-trust/changed-key
behavior, the forward reconnect backoff/stop logic, the `AppError` JSON contract the frontend relies on, and the
SOCKS5 handshake parser.

## Packaging

```bash
npm run tauri build
```

Produces, per the Tauri bundler configuration for the host platform: an MSI/NSIS installer on Windows, and an
AppImage/`.deb`/`.rpm` on Linux (whichever the build environment supports).

### Publishing a release

Releases are built locally and published from this machine. Bump the version in `package.json`,
`src-tauri/tauri.conf.json` and `src-tauri/Cargo.toml`, commit, then from PowerShell:

```powershell
.\scripts\release.ps1                      # build, push, tag v<version>, create the GitHub release
.\scripts\release.ps1 -SkipBuild -Notes "What changed"
```

It needs the GitHub CLI once: `winget install GitHub.cli`, then `gh auth login`.

## Architecture overview

```
src/                        React + TypeScript UI
  api/                       typed IPC wrappers + TS types mirroring the Rust domain types
  components/                shared UI (dialogs, group tree, xterm wrapper, status badges)
    files/                   SFTP panes, transfer queue panel, path/sort helpers
  screens/                   one file per major screen (vault setup/unlock, terminals, files, forwards, active, settings)
  state/                     React context providers (vault status, cached lists, forward status stream, terminal tabs, files session)

src-tauri/src/
  commands/                  thin #[tauri::command] functions - validate input shape, delegate, map errors
  domain/                    plain data types shared by repositories and commands
  repositories/              all SQL, one file per entity (groups, profiles, forwards, known hosts, secrets)
  vault/                     Argon2id + AES-256-GCM crypto and the lock/unlock state machine
  ssh/                       ssh2 session/handshake, host-key verification, interactive PTY terminal,
                              local/remote/dynamic forwarding, the SOCKS5 parser, and the session/forward registry
  sftp/                      SFTP browsing session, transfer worker (separate connection), remote-edit scratch files
  importers/                 Termius JSON export parsing (read only on the Rust side, so plaintext never reaches the UI)
  db/                        SQLite pool + migrations
  error.rs                   the single AppError type returned across IPC (never carries secrets)
  state.rs                   the app-wide Tauri state (db pool, vault manager, ssh manager)
```

Rust ↔ frontend communication is exclusively Tauri's typed `#[tauri::command]` functions, `tauri::ipc::Channel`
for streaming (terminal output, test-connection stages, SFTP transfer progress), and app-wide Tauri events for
forward status - never a REST call, WebSocket, or localhost server.

## Database and vault location

The SQLite database (`remotix.db`) lives in the OS-standard per-user application-data directory for the app
identifier `com.remotix.app`, e.g.:

- **Windows:** `%APPDATA%\com.remotix.app\remotix.db`
- **Linux:** `~/.local/share/com.remotix.app/remotix.db`

The exact path for your install is also shown on the Settings screen. It is created on first launch, alongside the
same directory used for window state.

### Backup and restore

Settings -> **Backup** -> **Export backup...** writes one `.remotix` file that is encrypted as a whole with the
master password (profiles, credentials, sudo passwords, forwards, notes, snippets, history). To restore, or to move
to another computer, use **Import backup...** in Settings, or **Restore from backup** on the first screen of a new
install, and enter the master password the vault had when the backup was made. Importing over existing data keeps a
copy of it as `remotix-before-import-<time>.db` in the data folder. Windows Hello has to be turned on again after a
restore.

By hand: close Remotix (tray -> Quit), copy `remotix.db` (and `remotix.db-wal` if present) elsewhere, and put them
back the same way to restore.

## Security model and limitations

- The vault key is derived with Argon2id from your master password and a random salt stored in `vault_metadata`; the
  password and derived key are **never** persisted.
- Every secret (`secrets` table row) is encrypted independently with AES-256-GCM, a fresh random nonce per
  encryption, and additional authenticated data binding the ciphertext to that record's id and the encryption format
  version - so ciphertext from one record can never be replayed onto another.
- Host keys are verified on every connection. First use requires an explicit Trust decision (fingerprint shown); a
  key that has changed since is never silently replaced - reconnecting is blocked until the user explicitly chooses
  "Replace trusted key."
- Local and dynamic forwards default to binding `127.0.0.1`; binding a non-loopback address requires an explicit
  acknowledgement in the editor because it exposes the forward to the local network.
- SSH profile fields are never interpolated into a shell command - all SSH operations go through the `ssh2` library's
  typed API directly.
- A wrong master password is rate-limited locally with a short, increasing delay and never corrupts or deletes the
  vault.
- There is intentionally no password-reset mechanism. Losing the master password means losing access to every stored
  secret; there is no recovery path by design.
- An OS keyring integration was **not** implemented in this milestone (it remains an optional future enhancement, and
  would only ever be a convenience layered on top of - never a replacement for - master-password vault encryption).
- **Windows crypto backend:** `libssh2-sys` defaults to Windows' native crypto API (WinCNG) on Windows unless the
  `openssl-on-win32` feature is explicitly enabled - WinCNG's libssh2 backend does not implement curve25519, ECDSA, or
  Ed25519, which almost every current OpenSSH server now requires (they were reachable only via classic
  Diffie-Hellman groups otherwise, and modern OpenSSH deployments increasingly omit those entirely). This project
  enables `openssl-on-win32` in `src-tauri/Cargo.toml` so the vendored OpenSSL backend - with full modern algorithm
  support - is used on both platforms. Verified by connecting to a live OpenSSH 10.x server.

### Dynamic forwarding limitations

The SOCKS5 listener implements the RFC 1928 handshake (no-auth method, `CONNECT` command, IPv4/domain-name/IPv6
address types) and opens a real `direct-tcpip` SSH channel per accepted connection - it does not proxy through any
third-party or cloud resolver. One simplification versus a fully spec-perfect proxy: the SOCKS success reply is sent
back to the client as soon as a well-formed `CONNECT` request is parsed, before the SSH `direct-tcpip` channel is
confirmed open (channel setup happens on the forward's single session-owning thread, immediately afterward). If the
upstream channel then fails to open, the client sees the connection drop rather than a SOCKS-level error code. For
this desktop client's use case (a personal SOCKS proxy through a trusted SSH host) this is a reasonable trade-off; it
is documented here rather than left silent.

## Command history

Each terminal tab loads the command history stored for its SSH profile and navigates it with **Up** and **Down**,
exactly like a shell would: Up walks back through earlier commands, Down walks forward, and stepping back past the
newest entry restores whatever was half-typed on the line. The entry replaces the input line through the remote
shell's own line editor (Ctrl+E, Ctrl+U, then the text), so nothing is ever submitted on the user's behalf.

The history is per SSH profile and kept in the local SQLite database (`command_history`), so it survives reconnects
and restarts, and is shared by every tab open on that profile. The newest 2000 commands per profile are kept; a
repeat of the previous command is not stored twice. Deleting a profile deletes its history with it.

How a command is captured: the prompt is read off the screen on the first keystroke of each line, and at Enter the
whole logical line (following soft wraps) is read back and the prompt removed. Reading the rendered line rather than
replaying keystrokes means tab completion, Ctrl+R and mid-line edits are all captured correctly. If the shell redraws
a different prompt than the one captured, the line is skipped rather than guessed at.

What it deliberately leaves alone:

- **Full-screen programs.** On the alternate screen buffer (vim, less, top, htop, nano) Up/Down pass straight
  through and nothing is recorded.
- **Commands starting with a space or tab** are never recorded, matching the `HISTCONTROL=ignorespace` convention.
- **The remote shell's history file.** Remotix never reads or writes `~/.bash_history` and friends. With an empty
  Remotix history the arrows fall through to the shell, so its own history still works.

**Clear history** in the terminal tab strip wipes the stored history for the active tab's profile and empties it in
every open pane. Up/Down can be turned off entirely in **Settings -> Terminal**, which leaves the arrows to the
remote shell.

Note that commands are stored in the clear, like hostnames and usernames - the vault's encryption covers credentials
only. The alternate screen is the only signal used to stand down; application cursor-key mode is deliberately not
used as a second one, because zsh's line editor turns it on at every prompt. That means a REPL that runs on the
normal screen (`python`, `mysql`) will see Remotix's history on Up rather than its own - the leading-space rule, the
Clear history button, and the settings toggle are the ways around that.

## Support matrix and deferred features

**Implemented and tested this milestone:** encrypted vault lifecycle; nested terminal groups and SSH profiles
(password / key / key+passphrase); host-key trust-on-first-use and changed-key blocking; interactive PTY terminals in
tabs with per-profile command history; nested port-forward groups; local and remote TCP forwarding with reconnect/backoff; dynamic SOCKS5 forwarding;
the SFTP file browser with transfer queue and remote edit; Termius JSON import; local settings; Windows and Linux
packaging via the Tauri bundler.

**Deferred out of scope, per the product brief:** mobile clients, cloud sync, accounts/login, team or shared vaults,
collaboration features, a browser-based admin console, serial connections, credential sharing, plugins, and AI
features.

**Known gaps to be aware of:**

- Split terminal panes are not implemented; each terminal is a full-width tab (a robust tabbed workspace was
  prioritized over panes for this milestone, as the spec allows deferring panes explicitly).
- An OS-keyring-backed convenience unlock was not built (see above).
- SOCKS5's success-reply-before-channel-confirmation simplification, described above.
- Playwright end-to-end coverage was not added in this pass; Vitest and Rust `cargo test` are the automated coverage
  for this milestone, alongside the manual verification guide below.

## Manual verification guide

With a real SSH server available (a spare Linux VM, container, or cloud instance is enough):

1. Launch the app, create a master password, and confirm the empty dashboard appears.
2. Create a Terminal Group, then an SSH profile inside it pointing at your test server (password or key auth).
3. Use **Test Connection** - confirm the timeline shows Connecting → Host-key verification → Authenticating, then
   Success. On first run you should see the Trust/Reject dialog with a SHA-256 fingerprint; trusting it should let
   the same test succeed again without re-prompting.
4. Click **Connect** and confirm an interactive shell opens in a new tab; run a few commands, resize the window, and
   confirm the terminal reflows.
5. Run a handful of distinct commands, then press **Up** repeatedly and confirm each one comes back newest-first,
   **Down** walks forward again, and a half-typed line is restored once you step past the newest entry. Open `vim`
   or `top` and confirm the arrows behave normally there. Run a command prefixed with a space and confirm Up never
   offers it. Close the tab, reconnect to the same profile, and confirm the history is still there; then use
   **Clear history** in the tab strip and confirm Up falls through to the remote shell again.
6. Disconnect (close the remote shell or the tab's Disconnect action) and confirm the tab shows a clear disconnected
   state with a working Reconnect.
7. Under Port Forwards, create a **Local** forward (e.g. `127.0.0.1:15432 → <a reachable host>:<port>` through the
   same profile), Start it, and confirm a client (e.g. `psql`, `curl`, or `nc 127.0.0.1 15432`) can reach the target
   through the tunnel; Stop it and confirm the port is released.
8. Repeat for a **Remote** forward and, if desired, a **Dynamic** forward (point a SOCKS5-aware client at
   `127.0.0.1:<port>` and confirm it can reach the network through the SSH host).
9. Open the **Files** screen, connect to your test server, browse both panes, and upload/download a file - confirm
   the transfer appears in the queue with progress, and that Cancel stops an in-flight transfer. Use **Edit** on a
   small remote text file, confirm it opens in your OS editor, save it, and confirm Remotix re-uploads it.
10. Lock the vault while a terminal/forward is active and confirm both are torn down and the unlock screen returns;
    unlock again and confirm profiles are intact but nothing auto-reconnects.
