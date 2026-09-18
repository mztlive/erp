//! 域 D06 `access_control` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；本域无金额字段。
//!
//! 角色/账号/Casbin 能力已有 `services::iam` 承载，本域 DTO 只覆盖增补的
//! `permission` 目录、`data_scope`、`user_role` 绑定记录与 `audit_event` 查询
//! （domains.md：D06 只做 data_scope 增补与 audit_log→audit_event 字段对齐）。

/// 排序方向。
pub use application_core::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
pub use application_core::PageView;
/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use application_core::normalize_sort;

/// 列表时间排序白名单（`created_at`/`updated_at`），三类列表复用同一语义。
pub(crate) const TIMESTAMP_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 由原始分页与排序参数构造归一化分页查询（三类列表 `normalized()` 共用）。
///
/// 排序字段过白名单校验，页码与单页条数取默认值并 clamp 到合法区间。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `page` - 可选页码；缺省为第一页
/// * `page_size` - 可选单页条数；缺省为默认大小
///
/// # 返回
/// 返回归一化后的分页与排序参数。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) fn page_params(
    sort_by: &Option<String>,
    sort_dir: &Option<String>,
    page: Option<u64>,
    page_size: Option<u32>,
) -> crate::error::Result<PageParams> {
    let (sort_by, sort_dir) = normalize_sort(sort_by, sort_dir, TIMESTAMP_SORT_FIELDS)?;
    Ok(PageParams {
        page: application_core::page_or_default(page),
        page_size: application_core::page_size_or_default(page_size),
        sort_by,
        sort_dir,
    })
}

pub mod audit_event;
pub mod data_scope;
pub mod permission;
pub mod user_role;

pub use audit_event::{AuditEventListParams, AuditEventView};
pub use data_scope::{
    CreateDataScopeRequest, DataScopeListMeta, DataScopeListParams, DataScopeListView, DataScopeView,
};
pub use permission::{
    CreatePermissionRequest, PermissionListParams, PermissionView, UpdatePermissionRequest,
};
pub use user_role::{AssignUserRoleRequest, RevokeUserRoleRequest, UserRoleListParams, UserRoleView};
#[cfg(test)]
mod tests {
    use serde_json::json;
    use validator::Validate;

    use super::{
        AssignUserRoleRequest, AuditEventListParams, CreateDataScopeRequest, CreatePermissionRequest,
        DataScopeListMeta, DataScopeListParams, DataScopeListView, PageView, PermissionListParams, SortDir,
        UpdatePermissionRequest, normalize_sort,
    };
    use crate::entity::access_control::{AuditEventResult, DataScopeSubjectType, DataScopeType};

    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("actor_id".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) =
            normalize_sort(&Some(" updated_at ".to_string()), &None, &["created_at", "updated_at"]).unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn permission_list_params_normalize_and_validate() {
        let params = PermissionListParams {
            resource: Some(" sales_order ".to_string()),
            disabled: Some(false),
            system: Some(true),
            page: Some(2),
            page_size: Some(50),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.resource.as_deref(), Some("sales_order"));
        assert_eq!(query.system, Some(true));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);

        let invalid = PermissionListParams { page: Some(0), page_size: Some(u32::MAX), ..Default::default() };
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn tier_a_list_params_default_to_empty() {
        assert!(PermissionListParams::default().resource.is_none());
        assert!(DataScopeListParams::default().subject_id.is_none());
        assert!(AuditEventListParams::default().actor_id.is_none());
    }

    #[test]
    fn tier_c_repository_filters_default_to_first_page_size_20() {
        use crate::repository::{AuditEventFilter, DataScopeFilter, PermissionFilter};
        assert_eq!((PermissionFilter::default().page, PermissionFilter::default().page_size), (1, 20));
        assert_eq!((DataScopeFilter::default().page, DataScopeFilter::default().page_size), (1, 20));
        assert_eq!((AuditEventFilter::default().page, AuditEventFilter::default().page_size), (1, 20));
        assert!(!PermissionFilter::default().sort_ascending);
    }

    #[test]
    fn data_scope_list_params_require_subject_type_for_subject_query() {
        let params = DataScopeListParams {
            subject_type: Some(DataScopeSubjectType::Role),
            subject_id: Some(" role-sales ".to_string()),
            resource: Some(" sales_order ".to_string()),
            action: Some(" list ".to_string()),
            scope_version: Some(" scope-v ".to_string()),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.subject_id.as_deref(), Some("role-sales"));
        assert_eq!(query.resource.as_deref(), Some("sales_order"));
        assert_eq!(query.action.as_deref(), Some("list"));
        assert_eq!(query.scope_version.as_deref(), Some("scope-v"));

        let missing =
            DataScopeListParams { subject_id: Some("role-sales".to_string()), ..Default::default() };
        assert!(missing.normalized().is_err());

        let wildcard = DataScopeListParams { resource: Some("*".to_string()), ..Default::default() };
        assert!(wildcard.normalized().is_err());
    }

    #[test]
    fn data_scope_list_envelope_keeps_versions_off_items() {
        let view = DataScopeListView::compose(
            PageView { items: Vec::new(), total: 0, page: 1, page_size: 20 },
            DataScopeListMeta::new("scope-v", "2026-09-15T00:00:00Z")
                .with_policy_version(3)
                .with_organization_version(4)
                .with_no_scope(true),
        );
        let json = serde_json::to_value(&view).unwrap();
        assert_eq!(json["empty_reason"], "no_scope");
        assert_eq!(json["scope_version"], "scope-v");
        assert_eq!(json["policy_version"], 3);
        assert_eq!(json["organization_version"], 4);
        assert_eq!(json["ownership_basis"], "data_scope_configuration");
        assert_eq!(json["items"].as_array().unwrap().len(), 0);
        assert!(json.get("role_clauses").is_none());
    }

    #[test]
    fn permission_create_request_keeps_fields() {
        let request: CreatePermissionRequest = serde_json::from_value(json!({
            "resource": "sales_order",
            "action": "approve",
            "name": "销售单审批",
        }))
        .unwrap();
        assert!(!request.system);
        let data = request.into_data();
        assert_eq!(data.resource, "sales_order");
    }

    #[test]
    fn permission_update_reports_changed_fields_in_contract_order() {
        let empty = UpdatePermissionRequest { version: 1, name: None, description: None, disabled: None };
        assert!(empty.changed_field_names().is_empty());

        let complete = UpdatePermissionRequest {
            version: 1,
            name: Some("销售单审批".to_string()),
            description: Some(String::new()),
            disabled: Some(false),
        };
        assert_eq!(complete.changed_field_names(), vec!["name", "description", "disabled"]);
    }

    #[test]
    fn data_scope_create_request_converts() {
        let request: CreateDataScopeRequest = serde_json::from_value(json!({
            "subject_type": "role",
            "subject_id": "role-sales",
            "scope_type": "team",
            "scope_targets": ["team-1", "team-2"],
            "schema_version": 2, "resource": "sales_order", "actions": ["list"],
            "target_dimension": "internal_org", "target_mode": "explicit", "include_descendants": false, "enabled": true,
        }))
        .unwrap();
        let data = request.into_data();
        assert_eq!(data.subject_type, DataScopeSubjectType::Role);
        assert_eq!(data.scope_type, DataScopeType::Team);
        assert_eq!(data.scope_targets, vec!["team-1", "team-2"]);
    }

    #[test]
    fn user_role_assign_request_defaults_effective_from_now() {
        let request: AssignUserRoleRequest = serde_json::from_value(json!({
            "user_id": "user-1",
            "role_id": "role-sales",
            "effective_to": 1700604800,
        }))
        .unwrap();
        let data = request.into_data("admin-1");
        assert_eq!(data.user_id, "user-1");
        assert_eq!(data.assigned_by, "admin-1");
        assert_eq!(data.effective_from.unix_secs(), erp_core::common::time::Instant::now().unix_secs());
        assert!(data.effective_to.is_some());
    }

    #[test]
    fn audit_event_list_params_normalize() {
        let params = AuditEventListParams {
            actor_id: Some(" user-1 ".to_string()),
            action_type: Some("permission.create".to_string()),
            result: Some(AuditEventResult::Denied),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.actor_id.as_deref(), Some("user-1"));
        assert_eq!(query.result, Some(AuditEventResult::Denied));
        assert_eq!(query.paging.page_size, 20);
    }
}
