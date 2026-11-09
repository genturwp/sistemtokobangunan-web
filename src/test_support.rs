//! Helpers shared by unit tests.

use std::path::PathBuf;

/// A database file in the temp directory, removed (with its WAL files) before use and on drop,
/// so a failing test leaves nothing behind. `name` must be unique among the tests.
pub(crate) struct TempDbFile(pub(crate) PathBuf);

impl TempDbFile {
    pub(crate) fn new(name: &str) -> TempDbFile {
        let file = TempDbFile(
            std::env::temp_dir().join(format!("stb-web-test-{name}-{}.db", std::process::id())),
        );
        file.remove();
        file
    }

    fn remove(&self) {
        for suffix in ["", "-wal", "-shm", "-journal"] {
            let mut file = self.0.as_os_str().to_owned();
            file.push(suffix);
            let _ = std::fs::remove_file(file);
        }
    }
}

impl Drop for TempDbFile {
    fn drop(&mut self) {
        self.remove();
    }
}
