//! 集成列表范围版本与空集口径。

use application_core::{EmptyScopeVersion, ScopeVersion};
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
    /// 由已解析范围构造列表元数据。返回不含内部授权证明的范围摘要。
    ///
    /// `access` 为当前动作已解析事实。
    pub fn from_access(access: &CompiledAccess) -> Self {
        Self {
            scope_version: access.version(),
            as_of: access.as_of().unix_secs().to_string(),
            scope_summary: "集成当前处理人所属内部组织范围",
            ownership_basis: "current_handler",
        }
    }
}

/// 构造可被 HTTP 边界识别的范围变化冲突。返回带 `DATA_SCOPE_CHANGED` 前缀的冲突错误。
pub fn data_scope_changed(detail: &str) -> Error {
    Error::ConflictError(format!("DATA_SCOPE_CHANGED：{detail}"))
}

/// 后续页必须携带当前范围版本，禁止拼接不同授权快照。
///
/// 第一页或版本非空时成功。第二页及之后缺少版本时返回 `DATA_SCOPE_CHANGED`。
///
/// `version` 为客户端回传的范围版本。
pub fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    ScopeVersion::new(version, EmptyScopeVersion::Missing)
        .require_for_page(page)
        .map_err(|error| data_scope_changed(error.detail()))
}

/// 客户端回传的范围版本必须与当前快照一致。
///
/// 未携带或完全一致时成功。版本不一致时返回 `DATA_SCOPE_CHANGED`。
///
/// - `expected`：后续页携带的范围版本；第一页可为空。
/// - `actual`：本次查询快照的范围版本。
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
/// 返回已解析事实、仓储条件、展开后的组织 ID 与列表元数据。授权解析或组织展开失败，或回传版本与当前快照不一致时返回对应错误。
///
/// - `access`：当前列表访问器。
/// - `org_unit_ids`：请求组织筛选；空表示不按组织收窄。
/// - `scope_version`：客户端回传的范围版本；第一页可空。
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
    let owner_org_unit_ids = if org_unit_ids.is_empty() {
        Vec::new()
    } else {
        access.expand_org_units(org_unit_ids, include_descendants, executor).await?.into_iter().collect()
    };

    let meta = ScopedIntegrationList::from_access(&read_scope);
    ensure_scope_version(scope_version, &meta.scope_version)?;
    Ok(ResolvedListScope { read_scope, owner_org_unit_ids, meta })
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
