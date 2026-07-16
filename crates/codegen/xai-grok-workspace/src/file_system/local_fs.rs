use std::path::{Path, PathBuf};
use tokio::fs;

use crate::file_system::{AsyncFileSystem, FsError};

pub struct LocalFs {
    root: PathBuf,
}

impl LocalFs {
    pub fn new(root: PathBuf) -> Self {
        Self { root }
    }
}

#[async_trait::async_trait]
impl AsyncFileSystem for LocalFs {
    fn root(&self) -> &Path {
        &self.root
    }

    async fn exists(&self, path: &Path) -> Result<bool, FsError> {
        Ok(fs::try_exists(path).await?)
    }

    async fn read_file(&self, path: &Path) -> Result<Vec<u8>, FsError> {
        Ok(fs::read(path).await?)
    }

    async fn try_read_file(&self, path: &Path) -> Result<Option<Vec<u8>>, FsError> {
        match fs::read(path).await {
            Ok(bytes) => Ok(Some(bytes)),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
            Err(e) => Err(e.into()),
        }
    }

    async fn write_file(&self, path: &Path, data: &[u8]) -> Result<(), FsError> {
        if let Some(dir) = path.parent() {
            fs::create_dir_all(dir).await?;
        }
        fs::write(path, data).await?;
        Ok(())
    }

    async fn delete_file(&self, path: &Path) -> Result<(), FsError> {
        match fs::remove_file(path).await {
            Ok(()) => Ok(()),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(()),
            Err(e) => Err(e.into()),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn delete_file_succeeds_when_target_is_already_missing() {
        let temp = tempfile::tempdir().expect("temp dir");
        let fs_impl = LocalFs::new(temp.path().to_path_buf());
        let file_path = temp.path().join("rewind-race.txt");

        fs::write(&file_path, b"content")
            .await
            .expect("create test file");

        fs_impl
            .delete_file(&file_path)
            .await
            .expect("first delete should succeed");

        // Regression: delete should be idempotent under TOCTOU races.
        fs_impl
            .delete_file(&file_path)
            .await
            .expect("missing file delete should be treated as success");
    }
}
