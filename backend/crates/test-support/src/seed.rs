//! 最小账号/角色/权限种子，供 P3 HTTP 测试鉴权使用。
//!
//! 行为契约（集合名与规则文档形态）：写入 `accounts` / `roles` /
//! `casbin_rules` 三个集合，与身份领域的访问控制索引及 Casbin 适配器保持
//! 一致（规则文档 `_id` 为 `sec\u{1f}ptype\u{1f}values 拼接` 的字符串）。
//! 角色键前缀与主体前缀的取值同步身份领域 RBAC 实现（`role:` /
//! `user:admin:`），改动任一侧须同步另一侧。

use chrono::Utc;
use erp_identity::{Role, RoleData};
use mongodb::Database;
use mongodb::bson::{Document, doc};

use crate::{Result, uuid_hex_n};

/// 账号集合名。
const ACCOUNTS: &str = "accounts";
/// 角色集合名。
const ROLES: &str = "roles";
/// Casbin 规则集合名。
const CASBIN_RULES: &str = "casbin_rules";

/// 随机十六进制串截取长度：种子角色 ID 后缀。
const UUID_ROLE_SUFFIX_LEN: usize = 8;
/// 随机十六进制串截取长度：种子账号 ID 后缀。
const UUID_ACCOUNT_SUFFIX_LEN: usize = 12;

/// 种子账号的持久化版本（`BaseModel::new` 固定为 1，与 `mint_jwt` 对应）。
pub(crate) const ACCOUNT_VERSION: u64 = 1;

/// Casbin 角色键前缀（与身份领域 RBAC 的角色键规则一致）。
const ROLE_PREFIX: &str = "role:";
/// 后台管理员主体前缀（与身份领域 RBAC 的主体规则一致）。
const SUBJECT_PREFIX: &str = "user:admin:";

/// 种子管理员角色拥有的 `list` 类权限键 `(resource, action)`。
const SEED_PERMISSIONS: &[(&str, &str)] = &[("role", "list"), ("admin", "list"), ("audit_log", "list")];

/// 种子一个后台管理员账号及其角色与 Casbin 权限策略。
///
/// 在 `accounts` / `roles` / `casbin_rules` 三个集合分别插入最小记录，
/// 使该账号对 `SEED_PERMISSIONS` 中的 `list` 类权限可被
/// `authenticate` + `with_permission` 中间件放行。账号登录名、类型与版本
/// 由 `seed_login` 派生规则固定，`mint_jwt` 按同一规则签发可用 JWT。
///
/// # 参数
/// * `db` - 目标数据库
///
/// # 返回值
/// 返回新生成的账号 ID（用于 `mint_jwt` 签发凭据）。
///
/// # 错误
/// 当任一集合写入失败时返回错误。
pub async fn seed_admin_account(db: &Database) -> Result<String> {
    let account_id = new_account_id();
    let login = seed_login(&account_id);
    insert_account(db, &account_id, &login).await?;
    insert_role_and_policies(db, &account_id).await?;
    Ok(account_id)
}

/// 构造并插入种子账号文档。
///
/// 账号 `version` 固定为 `ACCOUNT_VERSION`、`deleted_at` 为 0，满足
/// `BackofficeAuthService::validate_session` 的校验条件。
async fn insert_account(db: &Database, account_id: &str, login: &str) -> Result<()> {
    let now = Utc::now().timestamp();
    let account = doc! {
        "id": account_id,
        "version": ACCOUNT_VERSION as i64,
        "created_at": now,
        "updated_at": now,
        "deleted_at": 0_i64,
        "account": login,
        "password": "p0-fixture-not-a-real-hash",
        "name": "P0 测试管理员",
        "kind": "admin",
        "status": "active",
    };
    db.collection::<Document>(ACCOUNTS).insert_one(account).await?;
    Ok(())
}

/// 构造并插入种子角色与 Casbin 权限/绑定规则。
///
/// 按 `SEED_PERMISSIONS` 写入 N 条 `p` 权限规则 + 1 条 `g` 角色绑定规则
///（当前共 4 条，见 smoke 测试断言）；不写入 `casbin_policy_state`
/// 版本文档，首次加载的 Enforcer 快照即包含这些规则。
async fn insert_role_and_policies(db: &Database, account_id: &str) -> Result<()> {
    let role_id = format!("p0-test-{}", uuid_hex_n(UUID_ROLE_SUFFIX_LEN));
    let role = Role::new(role_id.clone(), RoleData::new("P0 测试管理员").with_system(false))?;
    db.collection::<Role>(ROLES).insert_one(role).await?;

    let role_key = format!("{ROLE_PREFIX}{role_id}");
    let mut rules: Vec<Document> = SEED_PERMISSIONS
        .iter()
        .map(|(resource, action)| casbin_rule("p", "p", &[&role_key, resource, action]))
        .collect();
    let subject = format!("{SUBJECT_PREFIX}{account_id}");
    rules.push(casbin_rule("g", "g", &[&subject, &role_key]));
    db.collection::<Document>(CASBIN_RULES).insert_many(rules).await?;
    Ok(())
}

/// 构造与 Casbin Adapter 完全同构的规则文档。
///
/// # 参数
/// * `sec` - 规则区段（`p` 或 `g`）
/// * `ptype` - 规则类型
/// * `values` - 规则值（权限规则为 `[角色键, 资源, 动作]`，绑定规则为
///   `[主体, 角色键]`）
///
/// # 返回值
/// 返回可插入 `casbin_rules` 集合的文档。
fn casbin_rule(sec: &str, ptype: &str, values: &[&str]) -> Document {
    let values = values.iter().map(|value| value.to_string()).collect::<Vec<String>>();
    let id = format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"));
    doc! { "_id": id, "sec": sec, "ptype": ptype, "values": values }
}

/// 生成随机账号 ID。
///
/// # 返回值
/// 返回 `acc-<12 位十六进制>` 形式的账号 ID。
fn new_account_id() -> String {
    format!("acc-{}", uuid_hex_n(UUID_ACCOUNT_SUFFIX_LEN))
}

/// 按种子派生规则生成登录账号。
///
/// 账号长度落在 `LoginAccount` 的 3..=64 字符区间；`mint_jwt` 必须复用
/// 同一规则，否则 `validate_session` 会拒绝签发身份。
///
/// # 参数
/// * `account_id` - 种子账号 ID
///
/// # 返回值
/// 返回与账号 ID 一一对应的登录账号。
pub(crate) fn seed_login(account_id: &str) -> String {
    format!("test_admin_{account_id}")
}

#[cfg(test)]
mod tests {
    use super::seed_login;

    #[test]
    fn seed_login_should_be_reproducible_and_within_length_limits() {
        let login = seed_login("acc-1234567890ab");
        assert_eq!(login, "test_admin_acc-1234567890ab");
        assert!((3..=64).contains(&login.chars().count()));
    }
}
