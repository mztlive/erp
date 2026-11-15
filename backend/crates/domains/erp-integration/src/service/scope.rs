//! 集成列表范围版本与空集口径。

use application_core::{EmptyScopeVersion, ScopeVersion};
use erp_core::common::time::Instant;
use erp_identity::CompiledAccess;
use erp_permissions::Permission;

use crate::error::{Error, Result};

/// 集成列表公共范围元数据。
pub struct ScopedIntegrationList {
    /// 跨页必须原样回传的范围版本。
    pub scope_version: String,
    /// 授权解析时点。
    pub as_of: String,
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
    ///
    /// # 返回
    /// 返回不含内部授权证明的范围摘要。
    ///
    /// # 错误
    /// 无。
    pub fn from_access(access: &CompiledAccess) -> Self {
        Self {
            scope_version: access.version(),
            as_of: Instant::now().unix_secs().to_string(),
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
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
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
    ScopeVersion::new(expected, EmptyScopeVersion::Missing)
        .verify(actual)
        .map_err(|error| data_scope_changed(error.detail()))
}

/// 列表授权解析结果（两类范围列表共用）。
pub struct ResolvedListScope {
    /// 已解析范围事实。
    /// 仓储读取条件。
    pub read_scope: CompiledAccess,
    /// 展开后的组织 ID。
    pub owner_org_unit_ids: Vec<String>,
    /// 范围元数据。
    pub meta: ScopedIntegrationList,
}

/// 解析列表授权、展开组织筛选并校验范围版本（错误任务与差异列表共用）。
///
/// 按授权解析、组织展开、元数据和版本校验的顺序执行。
///
/// # 参数
/// * `access` - 当前列表访问器
/// * `actor` - 已认证操作人
/// * `permission` - 两类列表入口的内置权限
/// * `org_unit_ids` - 请求组织筛选；空表示不按组织收窄
/// * `include_descendants` - 是否包含有效下级
/// * `scope_version` - 客户端回传的范围版本；第一页可空
/// * `executor` - 调用方执行器
///
/// # 返回
/// 返回已解析事实、仓储条件、展开后的组织 ID 与列表元数据。
///
/// # 错误
/// 授权解析或组织展开失败，或回传版本与当前快照不一致时返回对应错误。
pub async fn resolve_list_scope(
    access: &super::IntegrationAccess,
    actor: &application_core::AuditActor,
    permission: &Permission,
    org_unit_ids: &[String],
    include_descendants: bool,
    scope_version: Option<&str>,
    executor: &mut dyn persistence_core::Executor,
) -> Result<ResolvedListScope> {
    let read_scope = access.resolve(actor, permission, executor).await?;
    let owner_org_unit_ids = expand_org_filter(access, org_unit_ids, include_descendants, executor).await?;
    let meta = ScopedIntegrationList::from_access(&read_scope);
    ensure_scope_version(scope_version, &meta.scope_version)?;
    Ok(ResolvedListScope { read_scope, owner_org_unit_ids, meta })
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
/// 未知组织时拒绝。
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
    use super::*;
    #[test]
    fn later_pages_require_the_same_authorization_version() {
        assert!(ensure_page(2, None).is_err());
        assert!(ensure_page(1, None).is_ok());
        assert!(ensure_scope_version(Some("v2"), "v1").is_err());
        assert!(ensure_scope_version(Some("v1"), "v1").is_ok());
    }
}
