//! 库存列表范围信封与人员筛选规范化。

use application_core::{EmptyScopeVersion, PageView, QueryIds, ScopeVersion};
use serde::Serialize;

use crate::error::{Error, Result};
use crate::ports::InventoryScopeMeta;

/// 流水列表范围摘要：仓库维与经办人求交。
pub const MOVEMENT_SCOPE_SUMMARY: &str = "库存流水仓库范围与经办人筛选求交";
/// 流水所有人口径：记录人，不是库存所有人。
pub const MOVEMENT_OWNERSHIP_BASIS: &str = "warehouse_and_recorded_by";
/// 调整列表范围摘要：仓库维与经办、申请人、当前审批人求交。
pub const ADJUSTMENT_SCOPE_SUMMARY: &str = "库存调整仓库范围与经办人、申请人、当前审批人筛选求交";
/// 调整所有人口径：经办、快照申请人与当前审批人。
pub const ADJUSTMENT_OWNERSHIP_BASIS: &str = "warehouse_and_prepared_by_applicant_assignee";
/// 余额列表范围摘要：仅仓库维，不按人员授权。
pub const BALANCE_SCOPE_SUMMARY: &str = "库存余额仓库范围，不按人员授权";
/// 余额没有虚构所有人。
pub const BALANCE_OWNERSHIP_BASIS: &str = "warehouse";

/// 库存列表响应：分页字段与范围元信息同一信封。
#[derive(Debug, Clone, Serialize)]
pub struct InventoryListPage<T> {
    #[serde(flatten)]
    pub page: PageView<T>,
    /// 当前授权指纹；跨页必须原样回传。
    pub scope_version: String,
    /// 权限策略版本。
    pub policy_version: u64,
    /// 组织关系版本。
    pub organization_version: u64,
    /// 授权时点。
    pub as_of: String,
    /// 无仓库范围时为 `no_scope`。
    pub empty_reason: Option<&'static str>,
    /// 当前列表范围口径说明。
    pub scope_summary: &'static str,
    /// 人员条件权威来源；余额为仓库维。
    pub ownership_basis: &'static str,
}

impl<T> InventoryListPage<T> {
    /// 用授权元信息包装分页结果。
    /// 返回带范围信封的列表页。`no_scope` 为真时 `empty_reason` 为 `no_scope`，否则为 `None`。
    /// `page` 为已按仓库与人员条件求交的分页；`meta` 为对应资源动作的授权元信息。
    /// `no_scope` 为缺仓库维或空范围；`scope_summary` 为范围摘要。
    /// `ownership_basis` 为所有人口径。
    pub fn from_page(
        page: PageView<T>,
        meta: &InventoryScopeMeta,
        no_scope: bool,
        scope_summary: &'static str,
        ownership_basis: &'static str,
    ) -> Self {
        Self {
            page,
            scope_version: meta.scope_version().to_string(),
            policy_version: meta.policy_version(),
            organization_version: meta.organization_version(),
            as_of: meta.as_of().to_string(),
            empty_reason: no_scope.then_some("no_scope"),
            scope_summary,
            ownership_basis,
        }
    }
}

/// 处理库存经办人筛选时保留缺省与空集合的区别，返回原 ID 顺序，不去重。任何不区分大小写的 `me` 都返回 `ValidationError`。
pub fn normalized_user_ids(ids: Option<&QueryIds>) -> Result<Option<Vec<String>>> {
    let Some(ids) = ids else {
        return Ok(None);
    };
    if ids.as_slice().iter().any(|id| id.eq_ignore_ascii_case("me")) {
        return Err(Error::ValidationError("人员筛选不得使用 me".to_string()));
    }
    Ok(Some(ids.as_slice().to_vec()))
}

/// 处理库存分页请求时将空白 `scope_version` 视为缺省，过长则返回 `ValidationError`，再由分页过程检查指纹是否匹配。
pub fn normalized_scope_version(value: Option<&str>) -> Result<Option<String>> {
    let Some(value) = value.map(str::trim).filter(|value| !value.is_empty()) else {
        return Ok(None);
    };
    if value.len() > 256 {
        return Err(Error::ValidationError("范围版本非法".to_string()));
    }
    Ok(Some(value.to_string()))
}

/// 跨页读取库存列表时比较客户端的 `scope_version` 和当前授权指纹，避免翻页继续使用已变化的范围。
/// 第二页起缺少该值，或任意页提交的值不同于当前指纹时，返回带 `DATA_SCOPE_CHANGED` 的 `ConflictError`。
pub fn ensure_scope_version(page: u64, expected: Option<&str>, current: &str) -> Result<()> {
    let version = ScopeVersion::new(expected, EmptyScopeVersion::Provided);
    version
        .require_for_page(page)
        .and_then(|()| version.verify(current))
        .map_err(|error| Error::ConflictError(format!("DATA_SCOPE_CHANGED：{}", error.detail())))
}

#[cfg(test)]
mod tests {
    use application_core::PageView;

    use super::*;
    use crate::ports::InventoryScopeMeta;

    #[test]
    fn me_is_rejected_and_ids_are_kept() {
        let ids: QueryIds = serde_json::from_str("\"user-2,user-1\"").unwrap();
        assert_eq!(
            normalized_user_ids(Some(&ids)).unwrap().as_deref(),
            Some(["user-1".to_string(), "user-2".to_string()].as_slice())
        );
        let me: QueryIds = serde_json::from_str("\"me\"").unwrap();
        assert!(normalized_user_ids(Some(&me)).unwrap_err().to_string().contains("me"));
        assert!(normalized_user_ids(None).unwrap().is_none());
    }

    #[test]
    fn scope_version_conflict_and_missing_page_two_are_changed() {
        ensure_scope_version(1, None, "v1").unwrap();
        ensure_scope_version(2, Some("v1"), "v1").unwrap();
        let missing = ensure_scope_version(2, None, "v1").unwrap_err().to_string();
        assert!(missing.contains("DATA_SCOPE_CHANGED"));
        let changed = ensure_scope_version(2, Some("old"), "v1").unwrap_err().to_string();
        assert!(changed.contains("DATA_SCOPE_CHANGED"));

        assert!(normalized_scope_version(Some(&"x".repeat(257))).is_err());
        assert_eq!(normalized_scope_version(Some(" v1 ")).unwrap().as_deref(), Some("v1"));
    }

    #[test]
    fn empty_reason_is_no_scope_only_when_warehouse_scope_missing() {
        let meta = InventoryScopeMeta::new("v1", 3, 4, "2026-01-01T00:00:00Z");
        let page = InventoryListPage::<u8>::from_page(
            PageView::default(),
            &meta,
            true,
            BALANCE_SCOPE_SUMMARY,
            BALANCE_OWNERSHIP_BASIS,
        );
        assert_eq!(page.empty_reason, Some("no_scope"));
        assert_eq!(page.ownership_basis, "warehouse");

        let filled = InventoryListPage::<u8>::from_page(
            PageView::default(),
            &meta,
            false,
            MOVEMENT_SCOPE_SUMMARY,
            MOVEMENT_OWNERSHIP_BASIS,
        );
        assert!(filled.empty_reason.is_none());
        assert_eq!(filled.scope_version, "v1");
    }
}
