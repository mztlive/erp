//! 交接命令共用的幂等键、回放指纹与目标组织检查。
//!
//! 商品、供给等显式交接在不同文件中重复实现同一机械逻辑；本模块只收敛
//! “去空白幂等键、审计指纹前缀匹配、组织链路启用”三段无领域分支的检查。
//! 目标账号资格、审计动作与指纹载荷仍由各交接调用方决定。

use erp_identity::entity::organization::OrgTree;
use erp_identity::repository::OrganizationRepository;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 去空白后的幂等键；空白时拒绝。
///
/// # 参数
/// * `raw` - 请求携带的原始幂等键
///
/// # 返回
/// 返回去空白后的幂等键。
///
/// # 错误
/// 去空白后为空时返回校验错误。
///
/// # 关键业务约束
/// 与既有交接入口保持同一空键文案，不得补默认键。
pub(crate) fn trimmed_idempotency_key(raw: &str) -> Result<String> {
    let key = raw.trim().to_string();
    if key.is_empty() {
        return Err(Error::ValidationError("幂等键不能为空".into()));
    }
    Ok(key)
}

/// 判断审计留言是否属于同一交接命令指纹。
///
/// # 参数
/// * `message` - 已提交审计留言
/// * `expected_fingerprint` - 本次命令指纹
///
/// # 返回
/// 精确一致或以 `{expected};` 开头时为 true。
///
/// # 错误
/// 无。
pub(crate) fn replay_fingerprint_matches(message: Option<&str>, expected_fingerprint: &str) -> bool {
    let Some(message) = message else {
        return false;
    };
    let expected = format!("command_sha256={expected_fingerprint}");
    message == expected || message.starts_with(&format!("{expected};"))
}

/// 同一幂等键用于不同载荷时拒绝。
///
/// # 参数
/// * `message` - 已提交审计留言
/// * `expected_fingerprint` - 本次命令指纹
/// * `conflict` - 异载荷时的冲突文案
///
/// # 返回
/// 指纹一致时成功。
///
/// # 错误
/// 指纹不一致时返回冲突错误。
pub(crate) fn ensure_replay_fingerprint(
    message: Option<&str>,
    expected_fingerprint: &str,
    conflict: &str,
) -> Result<()> {
    if replay_fingerprint_matches(message, expected_fingerprint) {
        return Ok(());
    }
    Err(Error::ConflictError(conflict.to_string()))
}

/// 校验显式目标组织整条路径均启用。
///
/// # 参数
/// * `db` - 目标数据库
/// * `target_org` - 可选目标组织
/// * `executor` - 调用方执行器
///
/// # 返回
/// 未指定组织或路径全部启用时成功。
///
/// # 错误
/// 目标组织停用时拒绝。
///
/// # 关键业务约束
/// 只做启用检查，不提供任何范围授权；需拒绝公司组织的调用方先自检。
pub(crate) async fn ensure_org_enabled(
    db: &Database,
    target_org: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(org) = target_org.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(());
    };
    let state = OrganizationRepository::new(db).state(executor).await?;
    let tree = OrgTree::new(&state.units)?;
    let path = tree.path(org)?;
    if path.iter().any(|node| !node.enabled) {
        return Err(Error::BusinessLogicError("目标业务组织已停用".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn idempotency_key_trims_and_rejects_blank() {
        assert_eq!(trimmed_idempotency_key("  key-1  ").unwrap(), "key-1");
        assert!(trimmed_idempotency_key("").is_err());
        assert!(trimmed_idempotency_key("   ").is_err());
    }

    #[test]
    fn replay_accepts_exact_and_suffixed_fingerprint() {
        assert!(replay_fingerprint_matches(Some("command_sha256=abc"), "abc"));
        assert!(replay_fingerprint_matches(Some("command_sha256=abc;target=user-2"), "abc"));
        assert!(!replay_fingerprint_matches(Some("command_sha256=def"), "abc"));
        assert!(!replay_fingerprint_matches(Some("command_sha256=abc-def"), "abc"));
        assert!(!replay_fingerprint_matches(None, "abc"));
        assert!(!replay_fingerprint_matches(Some(""), "abc"));
    }

    #[test]
    fn replay_conflict_keeps_caller_message() {
        assert!(ensure_replay_fingerprint(Some("command_sha256=abc"), "abc", "冲突").is_ok());
        let error =
            ensure_replay_fingerprint(Some("command_sha256=def"), "abc", "同一幂等键已用于不同的商品交接")
                .unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "同一幂等键已用于不同的商品交接")
        );
    }
}
