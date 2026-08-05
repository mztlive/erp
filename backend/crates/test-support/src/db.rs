//! `TestDb`：按随机库名连接并创建、`Drop` 时清理的测试数据库夹具。

use mongodb::{Client, Database};
use uuid::Uuid;

use crate::{Error, Result};

/// 数据库名最大字节数（MongoDB 上限 64 字节，预留后缀空间）。
const MAX_DB_NAME_LEN: usize = 32;

/// 每个测试数据库内创建的标记集合，保证数据库真实存在、`drop` 可生效。
const FIXTURE_COLLECTION: &str = "_fixture";

/// 按随机库名连接并创建的独立测试数据库。
///
/// 每次 `TestDb::new(prefix)` 都会基于前缀生成随机数据库名并创建数据库，
/// 互不共享；`Drop` 时异步清理该数据库（尽力而为：在独立线程中执行，
/// 避免在异步运行时内 `block_on` 恐慌）。测试结束即随 `TestDb` 析构清理，
/// 禁止跨测试共享固定库名（conventions 7.2）。
pub struct TestDb {
    client: Client,
    db: Database,
    name: String,
}

impl TestDb {
    /// 创建独立测试数据库。
    ///
    /// # 参数
    /// * `prefix` - 数据库名前缀，仅保留字母数字与 `-`/`_`，超长截断
    ///
    /// # 返回值
    /// 返回连接并创建完成（含标记集合）的测试数据库夹具。
    ///
    /// # 错误
    /// `ERP_TEST_MONGO_URI` 未设置或 MongoDB 连接/建库失败时返回错误。
    pub async fn new(prefix: &str) -> Result<Self> {
        let uri = mongo_uri()?;
        let client = Client::with_uri_str(&uri).await?;
        let name = random_db_name(prefix);
        let db = client.database(&name);
        db.create_collection(FIXTURE_COLLECTION).await?;
        Ok(Self { client, db, name })
    }

    /// 返回数据库实例引用。
    ///
    /// # 返回值
    /// 返回当前测试数据库的引用。
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// 返回底层 MongoDB 客户端引用。
    ///
    /// # 返回值
    /// 返回创建当前测试数据库所用的客户端引用。
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// 返回随机数据库名。
    ///
    /// # 返回值
    /// 返回当前测试数据库的名称。
    pub fn name(&self) -> &str {
        &self.name
    }
}

impl Drop for TestDb {
    /// 在独立线程中异步 drop 测试数据库。
    ///
    /// 异步测试的 `Drop` 发生在运行时内部，直接 `block_on` 会恐慌；这里
    /// 用独立当前线程运行时完成清理，避免影响测试进程。
    fn drop(&mut self) {
        let client = self.client.clone();
        let name = self.name.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
            let Ok(runtime) = runtime else { return };
            let _ = runtime.block_on(async move { client.database(&name).drop().await });
        });
    }
}

/// 读取 `ERP_TEST_MONGO_URI` 环境变量。
///
/// # 返回值
/// 返回连接串。
///
/// # 错误
/// 环境变量未设置或为空时返回 `Error::EnvMissing`。
fn mongo_uri() -> Result<String> {
    let uri = std::env::var("ERP_TEST_MONGO_URI").unwrap_or_default();
    if uri.trim().is_empty() {
        return Err(Error::EnvMissing("ERP_TEST_MONGO_URI"));
    }
    Ok(uri)
}

/// 生成 `前缀_uuid短前缀` 形式的合法数据库名。
///
/// # 参数
/// * `prefix` - 调用方指定的前缀
///
/// # 返回值
/// 返回去除非法字符并截断后的随机数据库名。
fn random_db_name(prefix: &str) -> String {
    let sanitized: String = prefix
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .take(MAX_DB_NAME_LEN)
        .collect();
    let prefix = if sanitized.is_empty() {
        "test".to_string()
    } else {
        sanitized
    };
    format!("{prefix}_{}", &Uuid::new_v4().simple().to_string()[..8])
}

/// 生成随机十六进制短串。
///
/// # 返回值
/// 返回 UUID v4 的前 12 位十六进制字符。
pub(crate) fn uuid_hex() -> String {
    Uuid::new_v4().simple().to_string()
}

#[cfg(test)]
mod tests {
    use super::random_db_name;

    #[test]
    fn random_db_name_should_sanitize_and_truncate_prefix() {
        let name = random_db_name("p0/测试 test\n");
        assert!(name.starts_with("p0test_"));
        assert!(name
            .chars()
            .all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')));
        assert_eq!(name.len(), 6 + 1 + 8);
    }

    #[test]
    fn random_db_name_should_fallback_for_empty_prefix() {
        let name = random_db_name("///");
        assert!(name.starts_with("test_"));
    }
}
