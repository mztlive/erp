//! 最小账号/角色/权限种子，供 P3 HTTP 测试鉴权使用。
//!
//! 集合名与 `database/src/indexes/access_control.rs` 保持一致：
//! `accounts` / `roles` / `casbin_rules`。Casbin 规则文档形态与
//! `database/src/casbin_adapter.rs` 中的 `CasbinRule` 完全一致
//! （`_id` 为 `sec\u{1f}ptype\u{1f}values 拼接` 的字符串）。

use entities::{Role, RoleData};
use mongodb::bson::{doc, Document};
use mongodb::Database;

use crate::{db::uuid_hex, Result};

/// 账号集合名。
const ACCOUNTS: &str = "accounts";
/// 角色集合名。
const ROLES: &str = "roles";
/// Casbin 规则集合名。
const CASBIN_RULES: &str = "casbin_rules";

/// 种子账号的持久化版本（`BaseModel::new` 固定为 1，与 `mint_jwt` 对应）。
pub(crate) const ACCOUNT_VERSION: u64 = 1;

/// Casbin 角色键前缀（与 services/iam/rbac.rs `ROLE_PREFIX` 一致）。
const ROLE_PREFIX: &str = "role:";
/// 后台管理员主体前缀（与 services/iam/rbac.rs `subject()` 一致）。
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
    let now = chrono::Utc::now().timestamp();
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
/// 写入一条 `p` 权限规则 + 一条 `g` 角色绑定规则；不写入
/// `casbin_policy_state` 版本文档，首次加载的 Enforcer 快照即包含这些规则。
async fn insert_role_and_policies(db: &Database, account_id: &str) -> Result<()> {
    let role_id = format!("p0-test-{}", &uuid_hex()[..8]);
    let role = Role::new(
        role_id.clone(),
        RoleData {
            name: "P0 测试管理员".to_string(),
            description: None,
            system: false,
        },
    )?;
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
    let values = values
        .iter()
        .map(|value| value.to_string())
        .collect::<Vec<String>>();
    let id = format!("{sec}\u{1f}{ptype}\u{1f}{}", values.join("\u{1f}"));
    doc! { "_id": id, "sec": sec, "ptype": ptype, "values": values }
}

/// 生成随机账号 ID。
///
/// # 返回值
/// 返回 `acc-<12 位十六进制>` 形式的账号 ID。
fn new_account_id() -> String {
    format!("acc-{}", &uuid_hex()[..12])
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
