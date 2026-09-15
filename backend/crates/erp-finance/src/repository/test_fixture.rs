//! 本地环境门控夹具，保留既有 MongoDB 内联测试且不依赖旧业务 crate。

/// 按随机库名连接并创建、`Drop` 时清理的本地测试库夹具。
///
/// 不依赖 `test-support`，避免领域 crate 对旧夹具 crate 形成 dev 回边。
pub(crate) struct TestDb {
    client: mongodb::Client,
    db: mongodb::Database,
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
    pub(crate) async fn new(prefix: &str) -> mongodb::error::Result<Self> {
        let uri = std::env::var("ERP_TEST_MONGO_URI").unwrap_or_default();
        let client = mongodb::Client::with_uri_str(&uri).await?;
        let sanitized: String = prefix
            .chars()
            .filter(|ch| ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_'))
            .take(32)
            .collect();
        let prefix = if sanitized.is_empty() { "test".to_string() } else { sanitized };
        let name = format!("{prefix}_{}", mongodb::bson::oid::ObjectId::new().to_hex());
        let db = client.database(&name);
        db.create_collection("_fixture").await?;
        Ok(Self { client, db, name })
    }

    /// 返回创建测试数据库的客户端。
    pub(crate) fn client(&self) -> &mongodb::Client {
        &self.client
    }

    /// 返回数据库实例引用。
    pub(crate) fn db(&self) -> &mongodb::Database {
        &self.db
    }
}

impl Drop for TestDb {
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

/// `ERP_TEST_MONGO_URI` 已设置且非空时返回 `true`。
pub(crate) fn mongo_env_present() -> bool {
    std::env::var("ERP_TEST_MONGO_URI").map(|uri| !uri.trim().is_empty()).unwrap_or(false)
}

/// 需要真实 MongoDB 的集成测试门控宏。
///
/// `ERP_TEST_MONGO_URI` 缺失或为空时打印跳过原因并从当前测试函数 `return`。
macro_rules! require_mongo {
    (async $body:block) => {{
        if $crate::repository::test_fixture::mongo_env_present() {
            (async $body).await
        } else {
            ::std::eprintln!(
                "SKIP: ERP_TEST_MONGO_URI 未设置（需要 mongo:7 单节点副本集），已跳过 MongoDB 集成测试: {}",
                ::std::module_path!()
            );
            return;
        }
    }};
}

pub(crate) use require_mongo;
