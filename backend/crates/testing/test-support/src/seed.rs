//! 最小账号/角色/权限种子，供 P3 HTTP 测试鉴权使用。
//!
//! 行为契约（集合名与规则文档形态）：写入 `accounts` / `roles` /
//! `casbin_rules` 三个集合，与身份领域的访问控制索引及 Casbin 适配器保持
//! 一致（规则文档 `_id` 为 `sec\u{1f}ptype\u{1f}values 拼接` 的字符串）。
//! 角色键前缀与主体前缀的取值同步身份领域 RBAC 实现（`role:` /
//! `user:admin:`），改动任一侧须同步另一侧。

use chrono::Utc;
use erp_identity::{Role, RoleData, RoleId};
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

/// 写入后台测试账号、角色及 Casbin 授权规则，让请求可通过认证和指定列表权限的检查。返回账号 ID，供 `mint_jwt` 按相同身份规则签发测试令牌。
///
/// 写入集合失败时返回 MongoDB 错误；构造角色失败时返回实体校验错误。
pub async fn seed_admin_account(db: &Database) -> Result<String> {
    let account_id = format!("acc-{}", uuid_hex_n(UUID_ACCOUNT_SUFFIX_LEN));
    let login = seed_login(&account_id);
    insert_account(db, &account_id, &login).await?;
    insert_role_and_policies(db, &account_id).await?;
    Ok(account_id)
}

// 写入启用且未删除的管理员测试账号，为会话校验提供固定版本和可识别的登录身份。
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

// 为测试账号创建角色，并写入 `SEED_PERMISSIONS` 对应的权限规则和账号绑定，使权限加载器首次读取时可直接解析。无需单独种入策略版本文档。
async fn insert_role_and_policies(db: &Database, account_id: &str) -> Result<()> {
    let role_id = format!("p0-test-{}", uuid_hex_n(UUID_ROLE_SUFFIX_LEN));
    let role_key = format!("{ROLE_PREFIX}{role_id}");
    let role = Role::new(RoleId::parse(role_id)?, RoleData::new("P0 测试管理员").with_system(false))?;
    db.collection::<Role>(ROLES).insert_one(role).await?;

    let mut rules: Vec<Document> = SEED_PERMISSIONS
        .iter()
        .map(|(resource, action)| casbin_rule("p", &[&role_key, resource, action]))
        .collect();
    let subject = format!("{SUBJECT_PREFIX}{account_id}");
    rules.push(casbin_rule("g", &[&subject, &role_key]));
    db.collection::<Document>(CASBIN_RULES).insert_many(rules).await?;
    Ok(())
}

// 按 Casbin 的入库结构生成种子规则，`p` 和 `g` 分别用同一个值填充区段和类型，规则 ID 由类型及字段组合确定。
fn casbin_rule(kind: &str, values: &[&str]) -> Document {
    let values = values.iter().map(|value| value.to_string()).collect::<Vec<String>>();
    let id = format!("{kind}\u{1f}{kind}\u{1f}{}", values.join("\u{1f}"));
    doc! { "_id": id, "sec": kind, "ptype": kind, "values": values }
}

// 用账号 ID 派生稳定的测试登录名；账号种入和 JWT 签发共用该规则，确保会话校验看到同一个身份。
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
