//! P0-3 测试夹具：`TestDb`、`require_mongo!`、种子与断言辅助。
//!
//! 只作为 dev-dependency 使用；所有需要真实 MongoDB 的测试统一
//! `#[ignore]` + `ERP_TEST_MONGO_URI` 门控（见 conventions 7.2）。
//!
//! 本 crate 禁止依赖 `database` / `services` / `web-api`：P2/P3 消费方以
//! dev-dependency 引入本 crate，若本 crate 反向依赖它们会形成环。因此：
//! - 数据库连接直接使用 `mongodb` crate；
//! - JWT 只复制 `apps/web-api/src/core/auth/jwt.rs` 的 token 结构与
//!   HMAC-SHA256 签名算法（`mint_jwt`），不引用 web-api 代码；
//! - HTTP 测试客户端以 `axum::Router` 为参数（`TestApi::new`），由调用方
//!   提供已经组装好的路由，本 crate 不负责启动服务。

pub mod api;
pub mod db;
pub mod error;
pub mod indexes;
pub mod jwt;
pub mod seed;

pub use api::TestApi;
pub use db::TestDb;
pub use error::{Error, Result};
pub use indexes::assert_indexes;
pub use jwt::mint_jwt;
pub use seed::seed_admin_account;

/// 判断是否具备真实 MongoDB（单节点副本集）环境。
///
/// # 返回值
/// `ERP_TEST_MONGO_URI` 已设置且非空时返回 `true`。
pub fn mongo_env_present() -> bool {
    std::env::var("ERP_TEST_MONGO_URI")
        .map(|uri| !uri.trim().is_empty())
        .unwrap_or(false)
}

/// 需要真实 MongoDB 的集成测试门控宏。
///
/// `ERP_TEST_MONGO_URI` 缺失或为空时打印跳过原因并从当前测试函数 `return`，
/// 否则求值传入的异步测试体。支持两种用法（均可直接写在 `#[tokio::test]`
/// 异步测试函数里）：
///
/// ```ignore
/// require_mongo!(async move { run(db).await }.await);
/// require_mongo!(async { run(db).await });
/// ```
///
/// 第一种形态由调用方自行 `await`；第二种形态由宏内部 `await`。同步表达式
/// 形态（`require_mongo!(some_sync_fn())`）同样可用。
#[macro_export]
macro_rules! require_mongo {
    (async move $body:block) => {
        $crate::require_mongo!(async { $body })
    };
    (async $body:block) => {{
        if $crate::mongo_env_present() {
            (async $body).await
        } else {
            ::std::eprintln!(
                "SKIP: ERP_TEST_MONGO_URI 未设置（需要 mongo:7 单节点副本集），已跳过 MongoDB 集成测试: {}",
                ::std::module_path!()
            );
            return;
        }
    }};
    ($body:expr) => {{
        if $crate::mongo_env_present() {
            $body
        } else {
            ::std::eprintln!(
                "SKIP: ERP_TEST_MONGO_URI 未设置（需要 mongo:7 单节点副本集），已跳过 MongoDB 集成测试: {}",
                ::std::module_path!()
            );
            return;
        }
    }};
}
