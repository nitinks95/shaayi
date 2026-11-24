// First-launch acceptance marker. The marker is an empty-ish file in the app config directory.
use std::fs;
use std::path::{Path, PathBuf};

const MARKER: &str = "terms-accepted";

fn marker_path(dir: &Path) -> PathBuf {
    dir.join(MARKER)
}

pub fn accept_in(dir: &Path) -> Result<(), String> {
    fs::create_dir_all(dir).map_err(|e| e.to_string())?;
    fs::write(marker_path(dir), b"accepted\n").map_err(|e| e.to_string())
}

pub fn has_accepted_in(dir: &Path) -> bool {
    marker_path(dir).is_file()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn unique_dir(name: &str) -> PathBuf {
        let nanos = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        std::env::temp_dir().join(format!("shaayi-first-launch-{name}-{}-{nanos}", std::process::id()))
    }

    #[test]
    fn not_accepted_before_accept() {
        let dir = unique_dir("fresh");
        assert!(!has_accepted_in(&dir));
    }

    #[test]
    fn accept_persists_marker_file() {
        let dir = unique_dir("persist");
        accept_in(&dir).unwrap();
        assert!(has_accepted_in(&dir));
        assert!(dir.join("terms-accepted").is_file());
        accept_in(&dir).unwrap(); // idempotent
        assert!(has_accepted_in(&dir));
        let _ = fs::remove_dir_all(&dir);
    }
}
