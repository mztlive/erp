//! 集成列表范围版本与空集口径。

use crate::error::{Error, Result};
use crate::ports::IntegrationResolvedScope;
use crate::repository::IntegrationReadScope;

/// 集成列表公共范围元数据。
pub struct ScopedIntegrationList {
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// RBAC 策略版本。
    pub policy_version: u64,
    /// 组织配置版本。
    pub organization_version: u64,
    /// 授权解析时点。
    pub as_of: String,
    /// 角色无有效范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前范围口径摘要。
    pub scope_summary: &'static str,
    /// 当前处理人权威来源。
    pub ownership_basis: &'static str,
}

impl ScopedIntegrationList {
    /// 由已解析范围构造列表元数据。
    ///
    /// # 参数
    /// * `access` - 当前动作已解析事实
    /// * `read_scope` - 已映射仓储条件
    ///
    /// # 返回
    /// 返回不含内部授权证明的范围摘要。
    ///
    /// # 错误
    /// 无。
    pub fn from_access(access: &IntegrationResolvedScope, read_scope: &IntegrationReadScope) -> Self {
        Self {
            scope_version: access.scope_version.clone(),
            policy_version: access.policy_version,
            organization_version: access.organization_version,
            as_of: access.as_of.unix_secs().to_string(),
            empty_reason: (!access.has_scope_rules() || read_scope.is_empty()).then_some("no_scope"),
            scope_summary: "集成当前处理人所属内部组织范围",
            ownership_basis: "current_handler",
        }
    }
}

/// 构造可被 HTTP 边界识别的范围变化冲突。
///
/// # 参数
/// * `detail` - 面向用户的中文恢复说明
///
/// # 返回
/// 返回带 `DATA_SCOPE_CHANGED` 前缀的冲突错误。
///
/// # 错误
/// 无。
pub fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

/// 后续页必须携带当前范围版本，禁止拼接不同授权快照。
///
/// # 参数
/// * `page` - 请求页码
/// * `version` - 客户端回传的范围版本
///
/// # 返回
/// 第一页或版本非空时成功。
///
/// # 错误
/// 第二页及之后缺少版本时返回 `DATA_SCOPE_CHANGED`。
pub fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    if page > 1 && version.is_none_or(str::is_empty) {
        return Err(data_scope_changed("请从第一页刷新后继续查询"));
    }
    Ok(())
}

/// 客户端回传的范围版本必须与当前快照一致。
///
/// # 参数
/// * `expected` - 后续页携带的范围版本；第一页可为空
/// * `actual` - 本次查询快照的范围版本
///
/// # 返回
/// 未携带或完全一致时成功。
///
/// # 错误
/// 版本不一致时返回 `DATA_SCOPE_CHANGED`。
pub fn ensure_scope_version(expected: Option<&str>, actual: &str) -> Result<()> {
    match expected {
        None | Some("") => Ok(()),
        Some(version) if version == actual => Ok(()),
        Some(_) => Err(data_scope_changed("数据范围已变化，请从第一页刷新")),
    }
}

/// 展开请求组织筛选；空列表表示不按组织收窄。
///
/// # 参数
/// * `access` - 当前动作访问器
/// * `org_unit_ids` - 请求组织 ID
/// * `include_descendants` - 是否包含有效下级
/// * `executor` - 与授权相同的执行器
///
/// # 返回
/// 返回展开后的组织 ID；请求为空时返回空列表。
///
/// # 错误
/// 未知组织或未装配时拒绝。
pub async fn expand_org_filter(
    access: &super::IntegrationAccess,
    org_unit_ids: &[String],
    include_descendants: bool,
    executor: &mut dyn persistence_core::Executor,
) -> Result<Vec<String>> {
    if org_unit_ids.is_empty() {
        return Ok(Vec::new());
    }
    Ok(access.expand_org_units(org_unit_ids, include_descendants, executor).await?.into_iter().collect())
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;

    use super::*;
    use crate::ports::IntegrationResolvedClause;
    use crate::repository::IntegrationReadScope;

    fn access(has_rule: bool) -> IntegrationResolvedScope {
        IntegrationResolvedScope {
            user_id: "actor".into(),
            resource: "integration_error_task".into(),
            action: "list".into(),
            role_clauses: if has_rule {
                vec![IntegrationResolvedClause { self_owned: true, ..Default::default() }]
            } else {
                vec![]
            },
            user_limit: None,
            policy_version: 1,
            organization_version: 2,
            scope_version: "v1".into(),
            as_of: Instant::from_unix_secs(10),
        }
    }

    #[test]
    fn empty_scope_is_labeled_no_scope() {
        let read = IntegrationReadScope::default();
        let meta = ScopedIntegrationList::from_access(&access(false), &read);
        assert_eq!(meta.empty_reason, Some("no_scope"));
        assert_eq!(ensure_page(2, None).unwrap_err().to_string().contains("DATA_SCOPE_CHANGED"), true);
        assert!(ensure_scope_version(Some("v2"), "v1").is_err());
    }
}
