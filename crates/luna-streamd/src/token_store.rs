use std::fs::{self, OpenOptions};
use std::io::{self, Write};
use std::path::PathBuf;

#[cfg(unix)]
use std::os::unix::fs::{OpenOptionsExt, PermissionsExt};

pub struct TokenStore {
    dir: PathBuf,
}

impl TokenStore {
    pub fn new(dir: PathBuf) -> Self {
        Self { dir }
    }

    pub fn default_dir() -> PathBuf {
        if let Some(state) = std::env::var_os("XDG_STATE_HOME") {
            PathBuf::from(state).join("luna-stream")
        } else if let Some(home) = std::env::var_os("HOME") {
            PathBuf::from(home)
                .join(".local")
                .join("state")
                .join("luna-stream")
        } else {
            PathBuf::from("/tmp/luna-stream")
        }
    }

    fn file_path(&self) -> PathBuf {
        self.dir.join("screencast.token")
    }

    pub fn load(&self) -> Option<String> {
        let path = self.file_path();
        if path.exists() {
            let s = fs::read_to_string(path).ok()?;
            let trimmed = s.trim();
            if trimmed.is_empty() {
                None
            } else {
                Some(trimmed.to_string())
            }
        } else {
            None
        }
    }

    pub fn save(&self, token: &str) -> io::Result<()> {
        fs::create_dir_all(&self.dir)?;
        #[cfg(unix)]
        {
            let mut perms = fs::metadata(&self.dir)?.permissions();
            perms.set_mode(0o700);
            fs::set_permissions(&self.dir, perms)?;
        }

        let target = self.file_path();
        let temp_path = self
            .dir
            .join(format!(".screencast.token.tmp.{}", std::process::id()));

        let mut file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .mode(0o600)
            .open(&temp_path)?;

        file.write_all(token.as_bytes())?;
        file.sync_all()?;
        drop(file);

        fs::rename(temp_path, target)?;
        Ok(())
    }

    pub fn clear(&self) -> io::Result<()> {
        let path = self.file_path();
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
