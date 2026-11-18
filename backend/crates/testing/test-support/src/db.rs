//! `TestDb`：按随机库名连接并创建、`Drop` 时清理的测试数据库夹具。

use mongodb::{Client, Database};

use crate::{Error, Result, TEST_MONGO_URI_ENV, test_mongo_uri, uuid_hex_n};

/// 数据库名最大字符数（MongoDB 上限 64 字节；字符集已过滤为 ASCII，
/// 此处按字符计数，预留后缀空间）。
const MAX_DB_NAME_LEN: usize = 32;

/// 随机十六进制串截取长度：数据库名后缀。
const UUID_DB_SUFFIX_LEN: usize = 8;

/// 每个测试数据库内创建的标记集合，保证数据库真实存在、`drop` 可生效。
const FIXTURE_COLLECTION: &str = "_fixture";

/// 按随机库名连接并创建的独立测试数据库。
///
/// 每次 `TestDb::new(prefix)` 都会基于前缀生成随机数据库名并创建数据库，
/// 互不共享；`Drop` 时异步清理该数据库（尽力而为：在独立线程中执行，
/// 避免在异步运行时内 `block_on` 恐慌）。测试结束即随 `TestDb` 析构清理，
/// 禁止跨测试共享固定库名（conventions 7.2）。
pub struct TestDb {
    db: Database,
}

impl TestDb {
    /// 为需要真实 MongoDB 的测试创建独立数据库。把调用方前缀清理、截断并加随机后缀，再创建标记集合，使库名互不冲突且可清理。
    ///
    /// `ERP_TEST_MONGO_URI` 缺失或为空时返回 `Error::EnvMissing`；连接或创建集合失败时返回 MongoDB 错误。
    pub async fn new(prefix: &str) -> Result<Self> {
        let uri = test_mongo_uri().ok_or(Error::EnvMissing(TEST_MONGO_URI_ENV))?;
        let client = Client::with_uri_str(&uri).await?;
        let db = client.database(&random_db_name(prefix));
        db.create_collection(FIXTURE_COLLECTION).await?;
        Ok(Self { db })
    }

    /// 借用此测试独占的数据库句柄，供布置数据、调用仓储或核对写入结果。
    pub fn db(&self) -> &Database {
        &self.db
    }

    /// 借用创建此测试数据库的 MongoDB 客户端，供测试用同一客户端建立会话。
    pub fn client(&self) -> &Client {
        self.db.client()
    }

    /// 返回此测试实际分配的随机数据库名，供集合命名空间相关的核对或诊断使用。
    pub fn name(&self) -> &str {
        self.db.name()
    }

    /// 在用例结束时等待测试数据库删除完成，并把 MongoDB 删除错误交给调用方。需要确认清理结果时显式调用此方法；析构只提供兜底。
    pub async fn cleanup(&self) -> Result<()> {
        self.db.drop().await?;
        Ok(())
    }
}

impl Drop for TestDb {
    // 测试夹具离开作用域时，启动独立线程和运行时尝试删除测试库，避免在当前异步运行时内阻塞。用例需要确认完成或接收失败时，应显式调用 `TestDb::cleanup`。
    fn drop(&mut self) {
        let db = self.db.clone();
        std::thread::spawn(move || {
            let runtime = tokio::runtime::Builder::new_current_thread().enable_all().build();
            let Ok(runtime) = runtime else { return };
            let _ = runtime.block_on(async move { db.drop().await });
        });
    }
}

// 过滤并截断测试库名前缀，加随机后缀以隔离并发用例；过滤后没有可用字符时使用 `test`。
fn random_db_name(prefix: &str) -> String {
    let sanitized: String = prefix
        .chars()
        .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
        .take(MAX_DB_NAME_LEN)
        .collect();
    let prefix = if sanitized.is_empty() { "test".to_string() } else { sanitized };
    format!("{prefix}_{}", uuid_hex_n(UUID_DB_SUFFIX_LEN))
}

#[cfg(test)]
mod tests {
    use super::random_db_name;

    #[test]
    fn random_db_name_should_sanitize_and_truncate_prefix() {
        let name = random_db_name("p0/测试 test\n");
        assert!(name.starts_with("p0test_"));
        assert!(name.chars().all(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '_' | '-')));
        assert_eq!(name.len(), 6 + 1 + 8);
    }

    #[test]
    fn random_db_name_should_fallback_for_empty_prefix() {
        let name = random_db_name("///");
        assert!(name.starts_with("test_"));
    }
}
