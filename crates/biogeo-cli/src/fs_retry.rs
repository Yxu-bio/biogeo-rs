use std::fs;
use std::io;
use std::path::Path;
use std::thread;
use std::time::Duration;

const WINDOWS_RENAME_RETRY_DELAYS_MS: &[u64] = &[5, 10, 20, 40, 80, 160];

pub fn rename(from: &Path, to: &Path) -> io::Result<()> {
    for delay_ms in WINDOWS_RENAME_RETRY_DELAYS_MS {
        match fs::rename(from, to) {
            Ok(()) => return Ok(()),
            Err(error) if is_transient_windows_rename_error(&error) => {
                thread::sleep(Duration::from_millis(*delay_ms));
            }
            Err(error) => return Err(error),
        }
    }
    fs::rename(from, to)
}

fn is_transient_windows_rename_error(error: &io::Error) -> bool {
    cfg!(windows)
        && matches!(
            error.kind(),
            io::ErrorKind::PermissionDenied | io::ErrorKind::WouldBlock
        )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ordinary_rename_moves_the_file() {
        let root = std::env::temp_dir().join(format!(
            "biogeo-fs-retry-{}-{}",
            std::process::id(),
            crate::analysis_result::stable_fingerprint(b"ordinary-rename")
        ));
        let source = root.with_extension("source");
        let target = root.with_extension("target");
        let _ = fs::remove_file(&source);
        let _ = fs::remove_file(&target);
        fs::write(&source, b"payload").unwrap();

        rename(&source, &target).unwrap();

        assert!(!source.exists());
        assert_eq!(fs::read(&target).unwrap(), b"payload");
        fs::remove_file(target).unwrap();
    }
}
