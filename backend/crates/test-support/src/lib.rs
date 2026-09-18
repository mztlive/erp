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
pub mod filter;
pub mod indexes;
pub mod jwt;
pub mod seed;

pub use api::TestApi;
pub use db::TestDb;
pub use error::{Error, Result};
pub use filter::matches_filter;
pub use indexes::assert_indexes;
pub use jwt::mint_jwt;
pub use seed::seed_admin_account;
use uuid::Uuid;

/// 测试 MongoDB 连接串的环境变量名。
pub(crate) const TEST_MONGO_URI_ENV: &str = "ERP_TEST_MONGO_URI";

/// 读取测试 MongoDB 连接串。
///
/// 未设置或仅空白时返回 `None`，供存在性判断与连接复用同一口径。
pub(crate) fn test_mongo_uri() -> Option<String> {
    let uri = std::env::var(TEST_MONGO_URI_ENV).ok()?;
    (!uri.trim().is_empty()).then_some(uri)
}

/// 生成随机十六进制短串。
///
/// # 参数
/// * `len` - 截取长度（超过 32 时按 32 处理，保证不越界 panic）
///
/// # 返回值
/// 返回 UUID v4 十六进制形式的前 `len` 个字符。
pub(crate) fn uuid_hex_n(len: usize) -> String {
    let hex = Uuid::new_v4().simple().to_string();
    hex[..len.min(hex.len())].to_string()
}

/// 判断是否具备真实 MongoDB（单节点副本集）环境。
///
/// # 返回值
/// `ERP_TEST_MONGO_URI` 已设置且非空时返回 `true`。
pub fn mongo_env_present() -> bool {
    test_mongo_uri().is_some()
}

/// 需要真实 MongoDB 的集成测试门控宏使用的统一跳过提示前缀。
///
/// 各分支的动态部分（`module_path!()`）仍由宏在调用点展开，避免提示
/// 指向本 crate 而非实际跳过的测试模块。
#[doc(hidden)]
pub const MONGO_SKIP_HINT: &str =
    "SKIP: ERP_TEST_MONGO_URI 未设置（需要 mongo:7 单节点副本集），已跳过 MongoDB 集成测试";

/// 需要真实 MongoDB 的集成测试门控宏。
///
/// `ERP_TEST_MONGO_URI` 缺失或为空时打印跳过原因并从当前测试函数 `return`，
/// 否则求值传入的异步测试体。支持两种用法（均可直接写在 `#[tokio::test]`
/// 异步测试函数里）：
///
/// ```ignore
/// # use test_support::require_mongo;
/// # async fn run(_db: ()) {}
/// # async fn example() {
/// #   let db = ();
/// require_mongo!(async move { run(db).await }.await);
/// require_mongo!(async { run(db).await });
/// # }
/// ```
///
/// 第一种形态由调用方自行 `await`；第二种形态由宏内部 `await`。同步表达式
/// 形态（`require_mongo!(some_sync_fn())`）同样可用。
#[macro_export]
macro_rules! require_mongo {
    (async move $body:block) => {
        $crate::require_mongo!(async { $body })
    };
    (async $body:block) => {
        $crate::require_mongo!((async $body).await)
    };
    ($body:expr) => {{
        if $crate::mongo_env_present() {
            $body
        } else {
            ::std::eprintln!("{}: {}", $crate::MONGO_SKIP_HINT, ::std::module_path!());
            return;
        }
    }};
}
