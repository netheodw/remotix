//! Windows Hello through `KeyCredentialManager`.
//!
//! Hello keys are RSA-2048 signing keys held by the TPM (or the Hello
//! software container). RSA PKCS#1 v1.5 signatures are deterministic, so
//! signing the same stored challenge always yields the same bytes; the
//! vault hashes that signature into the key that wraps the vault key. The
//! signature can only be produced after the user passes the Hello prompt
//! (PIN, fingerprint or face), and never leaves this process.

use crate::error::AppResult;

/// Name of the Hello key container. One per Windows user, shared by dev and
/// release builds just like the app-data directory is.
#[cfg(windows)]
const CREDENTIAL_NAME: &str = "com.remotix.app.vault";

#[cfg(windows)]
mod imp {
    use std::sync::atomic::{AtomicBool, Ordering};
    use std::sync::Arc;
    use std::time::{Duration, Instant};

    use windows::core::{HSTRING, PCWSTR};
    use windows::Security::Credentials::{
        KeyCredential, KeyCredentialCreationOption, KeyCredentialManager, KeyCredentialStatus,
    };
    use windows::Storage::Streams::{DataReader, DataWriter, IBuffer};
    use windows::Win32::UI::WindowsAndMessaging::{FindWindowW, SetForegroundWindow};

    use super::CREDENTIAL_NAME;
    use crate::error::{AppError, AppResult};

    fn win_err(context: &'static str) -> impl Fn(windows::core::Error) -> AppError {
        move |e| {
            tracing::error!(error = %e, context, "windows hello call failed");
            AppError::BiometricUnavailable("Windows Hello is not available right now.".into())
        }
    }

    fn status_err(status: KeyCredentialStatus) -> AppError {
        match status {
            KeyCredentialStatus::UserCanceled | KeyCredentialStatus::UserPrefersPassword => {
                AppError::BiometricCancelled
            }
            KeyCredentialStatus::NotFound => AppError::BiometricUnavailable(
                "The Windows Hello key for Remotix was removed. Unlock with the master password and turn Windows Hello on again in Settings.".into(),
            ),
            KeyCredentialStatus::SecurityDeviceLocked => {
                AppError::BiometricUnavailable("The security device is locked. Try again later.".into())
            }
            other => {
                tracing::warn!(status = other.0, "windows hello returned an error status");
                AppError::BiometricUnavailable("Windows Hello could not verify you.".into())
            }
        }
    }

    /// The Hello prompt is owned by a separate system process and, for a
    /// desktop (non-UWP) app, often opens behind our window. Bring it to the
    /// front while the operation is pending - the same workaround KeePassXC
    /// and Bitwarden use, since there is no HWND-parented variant of
    /// `KeyCredentialManager`.
    fn raise_prompt_while(done: Arc<AtomicBool>) {
        std::thread::spawn(move || {
            let class = HSTRING::from("Credential Dialog Xaml Host");
            let deadline = Instant::now() + Duration::from_secs(10);
            while !done.load(Ordering::Relaxed) && Instant::now() < deadline {
                if let Ok(hwnd) = unsafe { FindWindowW(PCWSTR(class.as_ptr()), PCWSTR::null()) } {
                    if !hwnd.is_invalid() {
                        let _ = unsafe { SetForegroundWindow(hwnd) };
                        return;
                    }
                }
                std::thread::sleep(Duration::from_millis(100));
            }
        });
    }

    fn with_prompt<T>(f: impl FnOnce() -> AppResult<T>) -> AppResult<T> {
        let done = Arc::new(AtomicBool::new(false));
        raise_prompt_while(done.clone());
        let result = f();
        done.store(true, Ordering::Relaxed);
        result
    }

    fn to_buffer(bytes: &[u8]) -> AppResult<IBuffer> {
        let writer = DataWriter::new().map_err(win_err("DataWriter::new"))?;
        writer.WriteBytes(bytes).map_err(win_err("WriteBytes"))?;
        writer.DetachBuffer().map_err(win_err("DetachBuffer"))
    }

    fn from_buffer(buffer: &IBuffer) -> AppResult<Vec<u8>> {
        let len = buffer.Length().map_err(win_err("IBuffer::Length"))? as usize;
        let reader = DataReader::FromBuffer(buffer).map_err(win_err("DataReader::FromBuffer"))?;
        let mut out = vec![0u8; len];
        reader.ReadBytes(&mut out).map_err(win_err("ReadBytes"))?;
        Ok(out)
    }

    fn sign_with(credential: &KeyCredential, challenge: &[u8]) -> AppResult<Vec<u8>> {
        let result = credential
            .RequestSignAsync(&to_buffer(challenge)?)
            .and_then(|op| op.get())
            .map_err(win_err("RequestSignAsync"))?;
        let status = result.Status().map_err(win_err("sign status"))?;
        if status != KeyCredentialStatus::Success {
            return Err(status_err(status));
        }
        from_buffer(&result.Result().map_err(win_err("sign result"))?)
    }

    pub fn is_supported() -> bool {
        KeyCredentialManager::IsSupportedAsync()
            .and_then(|op| op.get())
            .unwrap_or(false)
    }

    pub fn create_and_sign(challenge: &[u8]) -> AppResult<Vec<u8>> {
        with_prompt(|| {
            let result = KeyCredentialManager::RequestCreateAsync(
                &HSTRING::from(CREDENTIAL_NAME),
                KeyCredentialCreationOption::ReplaceExisting,
            )
            .and_then(|op| op.get())
            .map_err(win_err("RequestCreateAsync"))?;
            let status = result.Status().map_err(win_err("create status"))?;
            if status != KeyCredentialStatus::Success {
                return Err(status_err(status));
            }
            let credential = result.Credential().map_err(win_err("create credential"))?;
            sign_with(&credential, challenge)
        })
    }

    pub fn sign(challenge: &[u8]) -> AppResult<Vec<u8>> {
        with_prompt(|| {
            let result = KeyCredentialManager::OpenAsync(&HSTRING::from(CREDENTIAL_NAME))
                .and_then(|op| op.get())
                .map_err(win_err("OpenAsync"))?;
            let status = result.Status().map_err(win_err("open status"))?;
            if status != KeyCredentialStatus::Success {
                return Err(status_err(status));
            }
            let credential = result.Credential().map_err(win_err("open credential"))?;
            sign_with(&credential, challenge)
        })
    }

    pub fn delete() {
        let outcome = KeyCredentialManager::DeleteAsync(&HSTRING::from(CREDENTIAL_NAME)).and_then(|op| op.get());
        if let Err(e) = outcome {
            // Already gone is the common case; nothing to undo either way.
            tracing::debug!(error = %e, "windows hello key delete");
        }
    }
}

#[cfg(not(windows))]
mod imp {
    use crate::error::{AppError, AppResult};

    fn unavailable() -> AppError {
        AppError::BiometricUnavailable("Windows Hello is only available on Windows.".into())
    }

    pub fn is_supported() -> bool {
        false
    }

    pub fn create_and_sign(_challenge: &[u8]) -> AppResult<Vec<u8>> {
        Err(unavailable())
    }

    pub fn sign(_challenge: &[u8]) -> AppResult<Vec<u8>> {
        Err(unavailable())
    }

    pub fn delete() {}
}

/// Whether this Windows user has Hello set up (a PIN at minimum).
/// Blocking - call from `spawn_blocking`.
pub fn is_supported() -> bool {
    imp::is_supported()
}

/// Creates (or replaces) the Remotix Hello key and signs `challenge` with
/// it. Shows the Hello prompt. Blocking.
pub fn create_and_sign(challenge: &[u8]) -> AppResult<Vec<u8>> {
    imp::create_and_sign(challenge)
}

/// Signs `challenge` with the existing Remotix Hello key. Shows the Hello
/// prompt. Blocking.
pub fn sign(challenge: &[u8]) -> AppResult<Vec<u8>> {
    imp::sign(challenge)
}

/// Removes the Remotix Hello key, if any. Blocking.
pub fn delete() {
    imp::delete()
}
