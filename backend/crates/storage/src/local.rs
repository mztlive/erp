use std::{
    ffi::OsString,
    path::{Component, Path, PathBuf},
};
use tokio::{fs, io::AsyncWriteExt};
use uuid::Uuid;

use crate::{Error, Result};

/// 本地文件存储实现
pub struct LocalStorage {
    /// 基础存储路径
    base_path: PathBuf,
}

impl LocalStorage {
    /// 创建新的本地存储实例
    ///
    /// # 参数
    ///
    /// * `base_path` - 基础存储路径
    ///
    /// # 错误
    ///
    /// 如果基础路径不存在或无法创建，将返回错误
    pub async fn new<P: AsRef<Path>>(base_path: P) -> Result<Self> {
        let base_path = base_path.as_ref().to_path_buf();

        // 确保基础路径存在
        if !base_path.exists() {
            fs::create_dir_all(&base_path).await?;
        }

        Ok(Self { base_path })
    }

    /// 保存文件
    ///
    /// # 参数
    ///
    /// * `path` - 相对存储路径
    /// * `content` - 文件内容
    ///
    /// # 错误
    ///
    /// 如果临时文件无法写入、同步或原子发布，将返回错误；发布失败时不会暴露半文件。
    pub async fn save<P: AsRef<Path>>(&self, path: P, content: &[u8]) -> Result<()> {
        let full_path = self.full_path(path)?;

        if let Some(parent) = full_path.parent() {
            fs::create_dir_all(parent).await?;
        }

        let mut temporary_file = PendingFile::new(temporary_path(&full_path)?);
        write_and_publish(temporary_file.path(), &full_path, content).await?;
        temporary_file.mark_published();

        Ok(())
    }

    /// 读取文件
    ///
    /// # 参数
    ///
    /// * `path` - 相对存储路径
    ///
    /// # 返回
    ///
    /// 返回文件内容的字节数组
    ///
    /// # 错误
    ///
    /// 如果文件不存在或无法读取，将返回错误
    pub async fn read<P: AsRef<Path>>(&self, path: P) -> Result<Vec<u8>> {
        let full_path = self.full_path(path)?;

        if !full_path.exists() {
            return Err(Error::NotFound);
        }

        Ok(fs::read(&full_path).await?)
    }

    /// 删除文件
    ///
    /// # 参数
    ///
    /// * `path` - 相对存储路径
    ///
    /// # 错误
    ///
    /// 如果文件不存在或无法删除，将返回错误
    pub async fn delete<P: AsRef<Path>>(&self, path: P) -> Result<()> {
        let full_path = self.full_path(path)?;

        if !full_path.exists() {
            return Err(Error::NotFound);
        }

        fs::remove_file(&full_path).await?;
        Ok(())
    }

    /// 检查文件是否存在
    ///
    /// # 参数
    ///
    /// * `path` - 相对存储路径
    pub async fn exists<P: AsRef<Path>>(&self, path: P) -> bool {
        match self.full_path(path) {
            Ok(full_path) => full_path.exists(),
            Err(_) => false,
        }
    }

    /// 获取完整文件路径
    ///
    /// # 参数
    ///
    /// * `path` - 相对存储路径
    ///
    /// # 错误
    ///
    /// 如果路径无效，将返回错误
    fn full_path<P: AsRef<Path>>(&self, path: P) -> Result<PathBuf> {
        let path = path.as_ref();

        if path.as_os_str().is_empty() || path.is_absolute() {
            return Err(Error::PathError("存储路径必须是非空相对路径".to_string()));
        }

        let mut has_normal_component = false;
        for component in path.components() {
            match component {
                Component::Normal(_) => has_normal_component = true,
                Component::CurDir => {}
                Component::ParentDir | Component::RootDir | Component::Prefix(_) => {
                    return Err(Error::PathError("存储路径不能越过基础目录".to_string()));
                }
            }
        }

        if !has_normal_component {
            return Err(Error::PathError("存储路径不能为空".to_string()));
        }

        Ok(self.base_path.join(path))
    }
}

/// 在失败或异步任务取消时尽力删除尚未发布的临时文件。
struct PendingFile {
    path: PathBuf,
    published: bool,
}

impl PendingFile {
    /// 创建待发布文件清理守卫。
    fn new(path: PathBuf) -> Self {
        Self {
            path,
            published: false,
        }
    }

    /// 返回临时文件路径。
    fn path(&self) -> &Path {
        &self.path
    }

    /// 标记临时文件已经通过 rename 发布，禁止 Drop 清理最终文件。
    fn mark_published(&mut self) {
        self.published = true;
    }
}

impl Drop for PendingFile {
    fn drop(&mut self) {
        if !self.published {
            let _cleanup_result = std::fs::remove_file(&self.path);
        }
    }
}

/// 为最终文件创建同目录、不可预测的临时路径。
fn temporary_path(final_path: &Path) -> Result<PathBuf> {
    let filename = final_path
        .file_name()
        .ok_or_else(|| Error::PathError("存储路径缺少文件名".to_string()))?;
    let mut temporary_name = OsString::from(".");
    temporary_name.push(filename);
    temporary_name.push(format!(".{}.uploading", Uuid::new_v4()));
    Ok(final_path.with_file_name(temporary_name))
}

/// 将内容完整同步到临时文件后，通过同目录 rename 原子发布。
async fn write_and_publish(temporary_path: &Path, final_path: &Path, content: &[u8]) -> std::io::Result<()> {
    let mut file = fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(temporary_path)
        .await?;
    file.write_all(content).await?;
    file.sync_all().await?;
    drop(file);
    fs::rename(temporary_path, final_path).await
}

#[cfg(test)]
mod tests {
    use super::*;
    use tempfile::tempdir;

    /// 测试 `test_save_and_read` 行为。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    #[tokio::test]
    async fn test_save_and_read() -> Result<()> {
        let temp_dir = tempdir()?;
        let storage = LocalStorage::new(temp_dir.path()).await?;

        let content = b"Hello, World!";
        storage.save("test.txt", content).await?;

        let read_content = storage.read("test.txt").await?;
        assert_eq!(read_content, content);

        Ok(())
    }

    /// 测试 `test_delete` 行为。
    ///
    /// # 返回
    /// 返回执行结果，`Ok` 表示成功，`Err` 表示失败。
    ///
    /// # 错误
    /// 当验证失败或底层操作失败时返回错误。
    #[tokio::test]
    async fn test_delete() -> Result<()> {
        let temp_dir = tempdir()?;
        let storage = LocalStorage::new(temp_dir.path()).await?;

        storage.save("test.txt", b"test").await?;
        assert!(storage.exists("test.txt").await);

        storage.delete("test.txt").await?;
        assert!(!storage.exists("test.txt").await);

        Ok(())
    }

    /// 测试 `test_not_found` 行为。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[tokio::test]
    async fn test_not_found() {
        let temp_dir = tempdir().unwrap();
        let storage = LocalStorage::new(temp_dir.path()).await.unwrap();

        assert!(matches!(
            storage.read("nonexistent.txt").await,
            Err(Error::NotFound)
        ));
    }

    /// 测试 `test_invalid_path` 行为。
    ///
    /// # 返回
    /// 不返回数据，仅表示执行结果。
    #[tokio::test]
    async fn test_invalid_path() {
        let temp_dir = tempdir().unwrap();
        let storage = LocalStorage::new(temp_dir.path()).await.unwrap();

        assert!(matches!(
            storage.save("../test.txt", b"test").await,
            Err(Error::PathError(_))
        ));
    }

    /// 绝对文件路径不得绕过存储基础目录。
    #[tokio::test]
    async fn rejects_absolute_path_traversal() -> Result<()> {
        let base_dir = tempdir()?;
        let outside_dir = tempdir()?;
        let outside_file = outside_dir.path().join("escaped.txt");
        let storage = LocalStorage::new(base_dir.path()).await?;

        let result = storage.save(&outside_file, b"escaped").await;

        assert!(matches!(result, Err(Error::PathError(_))));
        assert!(!outside_file.exists());
        Ok(())
    }

    /// 空路径与仅含当前目录的路径不得指向存储根目录本身。
    #[tokio::test]
    async fn rejects_empty_storage_path() -> Result<()> {
        let temp_dir = tempdir()?;
        let storage = LocalStorage::new(temp_dir.path()).await?;

        assert!(matches!(
            storage.save("", b"test").await,
            Err(Error::PathError(_))
        ));
        assert!(matches!(
            storage.save(".", b"test").await,
            Err(Error::PathError(_))
        ));
        Ok(())
    }

    /// 原子发布失败后不得遗留可占用磁盘的临时文件。
    #[tokio::test]
    async fn failed_publish_removes_temporary_file() -> Result<()> {
        let temp_dir = tempdir()?;
        let storage = LocalStorage::new(temp_dir.path()).await?;
        fs::create_dir(temp_dir.path().join("occupied")).await?;

        let result = storage.save("occupied", b"content").await;

        assert!(matches!(result, Err(Error::IoError(_))));
        let mut entries = fs::read_dir(temp_dir.path()).await?;
        let only_entry = entries.next_entry().await?.unwrap();
        assert_eq!(only_entry.file_name(), "occupied");
        assert!(entries.next_entry().await?.is_none());
        Ok(())
    }
}
