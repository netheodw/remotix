# AGENTS.md

This file provides guidance to AI coding agents (OpenCode, Claude Code, Cursor, etc.) when working with code in this
repository.

Remotix is a local-first SSH client, SFTP browser, and tunnel manager built with Tauri 2: a React 19 + TypeScript
UI (`src/`) and a Rust core (`src-tauri/`). There is no server, HTTP API, or sync. The UI talks to Rust only
through Tauri IPC. All data lives in one SQLite file, `remotix.db`, in the app-data dir for `com.remotix.app`.

`README.md` is user-facing and mostly current; treat it as the product overview, and this file as the working
instructions for changing code.

## Commands

```bash
npm install
npm run tauri dev          # Vite on :1420 + Tauri window with the Rust core
npm run tauri build        # installers (MSI/NSIS on Windows, AppImage/deb/rpm on Linux)
npx tsc --noEmit           # frontend type-check (no ESLint/Prettier is configured)
npm test                   # vitest run, jsdom env, globals enabled
npx vitest run src/components/terminalHistory.test.ts   # one test file
npx vitest run -t "drops blank lines"                   # tests matching a name
```

Rust, run from `src-tauri/`:

```bash
cargo test                          # unit tests + integration tests in tests/
cargo test vault::                  # tests whose path matches a filter
cargo test --test vault_security    # one integration test binary
cargo check
```

### Windows build gotchas

- Run `cargo` and `npm run tauri` from **PowerShell**, not Git Bash. `ssh2` uses `vendored-openssl`, which
  builds OpenSSL with `perl Configure`. In Git Bash, `perl` resolves to MSYS perl, and the build fails. It needs
  Strawberry Perl.
- Run `cargo` from inside `src-tauri/`. Using `--manifest-path` from the repo root has invalidated build
  fingerprints, which forces a multi-minute OpenSSL rebuild and can cause `LNK1104: msvcrt.lib`.
- `src-tauri/.cargo/config.toml` pins the MSVC linker and the INCLUDE/LIB paths to this machine's VS 2022
  BuildTools install, because more than one VS install is present.
- Walls of `LNK4099: PDB 'ossl_static.pdb' was not found` warnings are normal noise.
- Vite uses `strictPort` 1420. A killed `tauri dev` can leave an orphaned `node` process holding that port. Find
  it with `Get-NetTCPConnection -LocalPort 1420`.

## Architecture

### IPC contract (read multiple files together)

A command exists in four places, and all four must match:

1. `#[tauri::command]` in `src-tauri/src/commands/<area>.rs`
2. Registration in `generate_handler![...]` in `src-tauri/src/lib.rs`
3. A typed wrapper in `src/api/commands.ts`
4. TS mirrors of the Rust domain types in `src/api/types.ts`

Casing rules:

- Top-level `invoke` args are camelCase in JS (for example `profileId`). Tauri maps them to the snake_case Rust
  parameters.
- Struct payloads use their serde field names. Most are snake_case (`parent_id`, `terminal_group_id`).
  `EnqueueInput` in `commands/sftp.rs` is the exception, with `rename_all = "camelCase"`.

Command pattern: an async command clones `state.db` / `state.vault` and does its work inside
`tokio::task::spawn_blocking`, because rusqlite and ssh2 both block. It maps the `JoinError` to
`AppError::Internal`. Commands stay thin; SQL lives in `repositories/`, and SSH work lives in `ssh/` and `sftp/`.

Streaming:

- `tauri::ipc::Channel` carries terminal output (base64 in `TerminalEvent::Output`), test-connection stages, and
  SFTP `TransferEvent`s. The frontend creates the `Channel` in `api/commands.ts`.
- Forward status goes through the app-wide `forward-status` event (`app.emit` in `commands/forwards.rs`), and
  `state/ForwardStatusContext.tsx` listens for it.

### Errors

`AppError` (`src-tauri/src/error.rs`) is the only error type that crosses IPC. It serializes as
`{ kind: "<snake_case>", message?: ... }`. Unit tests in `error.rs` lock this JSON shape, and
`src/api/errors.ts` depends on it:

- `errorMessage()` turns an error into user-facing text.
- `parseHostKeyError()` handles `host_key_untrusted` and `host_key_mismatch`. Their structured payload feeds
  `HostKeyDialog`. After the user decides, the frontend calls `known_hosts_trust` and retries.

Error variants must never carry secrets. Lower-level errors are redacted in the `From` impls, and the full
detail goes only to `tracing`.

### Vault and secrets

- `vault/manager.rs` holds the lock/unlock state machine and the rate limiter. `vault/crypto.rs` holds Argon2id
  and AES-256-GCM. Each record's AAD binds its id and a format version.
- Secrets are stored in the `secrets` table through `SecretRepository`, and decrypted only inside Rust:
  `ProfileRepository::load_auth_material` decrypts them just before an SSH connect.
- Plaintext never goes back to the UI. `SshProfile` exposes only `has_secret` / `has_passphrase`, and updates
  send `AuthSecretInput { kind: "unchanged" }` to keep the stored secret. The one deliberate exception is
  `profiles_reveal_password` (the eye icon in the profile editor): it returns a stored *password* only, never a
  private key or passphrase, and needs an unlocked vault. Don't widen it to other secret kinds.
- Termius import files are read only on the Rust side (`importers/`), for the same reason.
- `tests/vault_security.rs` scans the raw DB and WAL bytes to confirm no plaintext appears.
- Windows Hello unlock (`platform/hello.rs`, table `vault_hello`): the vault key is AES-GCM wrapped under
  SHA-256 of a Hello (KeyCredentialManager) signature over a stored challenge. Enabling needs the vault unlocked
  *and* the master password. `vault_metadata.password_verified_at` records the last typed password; Hello unlock
  is refused with `master_password_required` once that is 6 months old, before any Hello prompt.
- Desktop shortcuts (`platform/shortcut.rs`, `commands/launch.rs`) start the exe with `--open-profile <uuid>`.
  The id waits in `AppState.pending_launch_profile` until unlock; `tauri-plugin-single-instance` forwards a
  second launch's args and emits `launch-open-profile`.
- `vault_lock` calls `SshManager::stop_all()` before locking. Unlocking auto-starts forwards flagged
  `start_on_launch`. Closing the window also calls `stop_all()`.

### Long-lived SSH work: one OS thread per session

ssh2 sessions are blocking and not `Sync`. So each terminal, each port forward, and each SFTP session runs on its
own dedicated `std::thread`, which owns its `Session`. Callers control it through a `std::sync::mpsc` command
channel. `ssh::SshManager` (in `AppState`) is the registry of live handles, keyed by id. Every open, close, start,
stop, and lock goes through it, so the UI's idea of what is live matches the backend.

- `ssh/session.rs`: `connect_verified` runs the TCP handshake, then checks the host key against
  `KnownHostRepository`, then authenticates. It never interpolates profile fields into a shell command.
- `ssh/forward_runtime.rs`: runs local (-L), remote (-R), and dynamic (-D, SOCKS5 via `ssh/socks5.rs`) forwards,
  with bounded backoff. It intentionally does not retry after a host-key mismatch or an auth failure.
- `sftp/session.rs` and `sftp/transfer.rs`: SFTP uses **two** connections. Browsing runs on the control session.
  Transfers run on a separate, lazily opened connection so a listing never waits behind a transfer (libssh2 holds
  one mutex per session).
- SFTP worker operations are wrapped in `sftp::guard` (`catch_unwind`), because ssh2 panics on non-UTF-8
  filenames on Windows. This is why `[profile.release]` deliberately does **not** set `panic = "abort"`. Keep it
  that way.

### Database

`db/mod.rs` sets up an r2d2 pool with `foreign_keys=ON` and WAL. To add a migration, create
`db/migrations/NNNN_name.sql` and append it to the `MIGRATIONS` array. Each migration runs once, in a
transaction, and is recorded in `schema_migrations`. App settings are plain key/value rows in `app_settings`.

### Frontend

- `App.tsx` gates on vault status through `VaultProvider`, then shows setup, unlock, or the app. The data
  providers (`DataContext`, `ForwardStatusContext`, `TerminalTabsContext`, `FilesContext`) mount only after
  unlock. Navigation is a `section` state in `AppShell`; there is no router.
- Logic that can be tested is kept in plain `.ts` modules next to the components, and most Vitest tests target
  these: `terminalHistory.ts`, `terminalBuffer.ts`, `state/transferQueue.ts`, `files/posixPath.ts` / `winPath.ts`
  / `sort.ts`, and `screens/termiusImport.ts`.
- Command history: `terminalBuffer.ts` captures the command by reading the rendered xterm line (not by
  replaying keystrokes). `terminalHistory.ts` drives Up/Down. `normalizeCommand()` there must stay in sync with
  `normalize` in `src-tauri/src/repositories/command_history_repo.rs`.
- Toolbox: `ToolboxPanel.tsx` shares the terminal side panel with Files (one or the other). Snippets and
  categories live in `snippets` / `snippet_categories` (migration `0004_snippets.sql` seeds the default Linux
  commands). `{{name}}` marks a placeholder, parsed in `components/toolbox.ts`. Commands reach the tab through
  the `registerSender` callback on `TerminalPane`, as a paste; Run also sends Enter.
- Remote edit: `sftp_edit_open` downloads the file to `%TEMP%/remotix-edit/<id>/` and opens it with the OS
  editor through the opener plugin, called from Rust. The frontend polls `local_file_mtime` and re-uploads
  through the transfer queue.

### Test fixtures

`src-tauri/src/testing.rs` is a public, non-`cfg(test)` module, so the integration tests in `tests/` can use it.
For the same reason, `tempfile` is a normal dependency and not a dev-dependency.
