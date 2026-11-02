//! Desktop shortcuts (`.lnk`) that start Remotix straight into one SSH
//! profile's terminal.

use std::path::Path;

use crate::error::AppResult;

/// Command-line flag the shortcut passes; parsed by [`profile_from_args`].
pub const OPEN_PROFILE_FLAG: &str = "--open-profile";

/// Returns the profile id from `--open-profile <id>` in `args`, if present
/// and shaped like a UUID. Anything else is ignored rather than trusted:
/// the value only ever selects an existing profile in the local DB.
pub fn profile_from_args<I, S>(args: I) -> Option<String>
where
    I: IntoIterator<Item = S>,
    S: AsRef<str>,
{
    let mut iter = args.into_iter();
    while let Some(arg) = iter.next() {
        let arg = arg.as_ref();
        let value = if arg == OPEN_PROFILE_FLAG {
            iter.next().map(|v| v.as_ref().to_string())
        } else {
            arg.strip_prefix("--open-profile=").map(str::to_string)
        };
        if let Some(value) = value {
            return uuid::Uuid::parse_str(&value).ok().map(|u| u.to_string());
        }
    }
    None
}

/// Strips characters Windows does not allow in file names.
pub fn safe_file_stem(name: &str) -> String {
    let cleaned: String = name
        .chars()
        .map(|c| if r#"<>:"/\|?*"#.contains(c) || c.is_control() { '_' } else { c })
        .collect();
    let trimmed = cleaned.trim().trim_end_matches('.').trim();
    if trimmed.is_empty() {
        "Terminal".to_string()
    } else {
        trimmed.chars().take(80).collect()
    }
}

#[cfg(windows)]
pub fn create(link_path: &Path, target: &Path, arguments: &str, description: &str) -> AppResult<()> {
    use windows::core::{Interface, HSTRING, PCWSTR};
    use windows::Win32::System::Com::{
        CoCreateInstance, CoInitializeEx, CoUninitialize, IPersistFile, CLSCTX_INPROC_SERVER,
        COINIT_APARTMENTTHREADED,
    };
    use windows::Win32::UI::Shell::{IShellLinkW, ShellLink};

    use crate::error::AppError;

    let err = |e: windows::core::Error| {
        tracing::error!(error = %e, "creating desktop shortcut failed");
        AppError::Io("could not create the desktop shortcut".into())
    };
    let wide = |s: &HSTRING| PCWSTR(s.as_ptr());

    let target_s = HSTRING::from(target.as_os_str());
    let workdir_s = HSTRING::from(target.parent().unwrap_or(target).as_os_str());
    let args_s = HSTRING::from(arguments);
    let desc_s = HSTRING::from(description);
    let link_s = HSTRING::from(link_path.as_os_str());

    unsafe {
        // S_FALSE (already initialized on this thread) is fine; a different
        // apartment mode is not, but blocking-pool threads never set one.
        let init = CoInitializeEx(None, COINIT_APARTMENTTHREADED);
        let result = (|| {
            let link: IShellLinkW = CoCreateInstance(&ShellLink, None, CLSCTX_INPROC_SERVER).map_err(err)?;
            link.SetPath(wide(&target_s)).map_err(err)?;
            link.SetArguments(wide(&args_s)).map_err(err)?;
            link.SetWorkingDirectory(wide(&workdir_s)).map_err(err)?;
            link.SetDescription(wide(&desc_s)).map_err(err)?;
            link.SetIconLocation(wide(&target_s), 0).map_err(err)?;
            let file: IPersistFile = link.cast().map_err(err)?;
            file.Save(wide(&link_s), true).map_err(err)
        })();
        if init.is_ok() {
            CoUninitialize();
        }
        result
    }
}

#[cfg(not(windows))]
pub fn create(_link_path: &Path, _target: &Path, _arguments: &str, _description: &str) -> AppResult<()> {
    Err(crate::error::AppError::Validation(
        "desktop shortcuts are only supported on Windows".into(),
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const ID: &str = "0b6f8f2e-3c1a-4f7e-9a51-2d3c4b5a6978";

    #[test]
    fn reads_profile_id_from_separate_or_inline_flag() {
        assert_eq!(profile_from_args(["remotix.exe", "--open-profile", ID]).as_deref(), Some(ID));
        assert_eq!(
            profile_from_args(["remotix.exe", &format!("--open-profile={ID}")]).as_deref(),
            Some(ID)
        );
    }

    #[test]
    fn ignores_missing_or_malformed_profile_id() {
        assert_eq!(profile_from_args(["remotix.exe"]), None);
        assert_eq!(profile_from_args(["remotix.exe", "--open-profile"]), None);
        assert_eq!(profile_from_args(["remotix.exe", "--open-profile", "'; DROP TABLE"]), None);
    }

    #[test]
    fn file_stem_drops_reserved_characters() {
        assert_eq!(safe_file_stem("prod: web/01?"), "prod_ web_01_");
        assert_eq!(safe_file_stem("  ...  "), "Terminal");
    }
}
