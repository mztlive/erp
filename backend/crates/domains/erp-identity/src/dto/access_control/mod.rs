//! 管理端的权限目录、人员范围、权限检查、角色绑定留痕和审计列表数据。
//!
//! HTTP 处理函数须直接复用此处的请求和响应类型。`page`、`page_size`、`sort_by`、`sort_dir`
//! 与筛选字段同层传递。绑定和审计时间用 Unix 秒；权限检查的 `checked_at` 用 RFC 3339 文本。
//! 账号、角色与实际 Casbin 绑定由 `service::iam` 管理。

/// 管理端共用分页响应：`items`、`total`、`page` 和 `page_size`。
pub use application_core::PageView;
/// 列表使用的升序或降序。
pub use application_core::SortDir;
/// 供测试复用的排序校验。忽略两端空白，缺省或全空白时使用 `("created_at", Desc)` 的对应默认项。
/// 显式字段须在 `allowed_fields` 中，方向须为 `asc` 或 `desc`，否则返回 `ValidationError`。
#[cfg(test)]
pub(crate) use application_core::normalize_sort;

/// 权限目录和审计列表共用的已校验分页参数。
pub use crate::entity::access_control::paging::PageParams;

pub mod audit_event;
pub mod permission;
pub mod user_role;

pub use audit_event::{AuditEventListParams, AuditEventView};
pub use permission::{
    CreatePermissionRequest, PermissionListParams, PermissionView, UpdatePermissionRequest,
};
pub use user_role::{AssignUserRoleRequest, RevokeUserRoleRequest, UserRoleListParams, UserRoleView};
#[cfg(test)]
mod tests {
    use serde_json::json;
    use validator::Validate;

    use super::{
        AssignUserRoleRequest, AuditEventListParams, CreatePermissionRequest, PermissionListParams, SortDir,
        UpdatePermissionRequest, normalize_sort,
    };
    use crate::entity::access_control::AuditEventResult;

    // 验证排序字段白名单，已知字段可带首尾空白，未指定方向时按降序。
    #[test]
    fn sort_whitelist_rejects_unknown_fields() {
        assert!(normalize_sort(&Some("actor_id".to_string()), &None, &["created_at"]).is_err());
        let (field, direction) =
            normalize_sort(&Some(" updated_at ".to_string()), &None, &["created_at", "updated_at"]).unwrap();
        assert_eq!(field, "updated_at");
        assert_eq!(direction, SortDir::Desc);
    }

    // 验证资源筛选裁剪空白、保留显式分页，并拒绝越界分页。
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

    // 验证未提供筛选条件时，权限资源和审计操作者字段默认为空。
    #[test]
    fn tier_a_list_params_default_to_empty() {
        assert!(PermissionListParams::default().resource.is_none());
        assert!(AuditEventListParams::default().actor_id.is_none());
    }

    // 验证两种仓储筛选器默认第 1 页、每页 20 条，权限目录默认降序。
    #[test]
    fn tier_c_repository_filters_default_to_first_page_size_20() {
        use crate::repository::{AuditEventFilter, PermissionFilter};
        assert_eq!(
            (PermissionFilter::default().paging.page, PermissionFilter::default().paging.page_size),
            (1, 20)
        );
        assert_eq!(
            (AuditEventFilter::default().paging.page, AuditEventFilter::default().paging.page_size),
            (1, 20)
        );
        assert_eq!(PermissionFilter::default().paging.sort_dir, SortDir::Desc);
    }

    // 验证创建请求默认标为自定义权限，并将资源代码传给实体。
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

    // 验证空补丁返回空字段列表，空描述与 `false` 仍按固定顺序列入。
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

    // 验证省略生效时间时使用当前秒，并保留用户、服务端分配人和到期时间。
    #[test]
    fn user_role_assign_request_defaults_effective_from_now() {
        let request: AssignUserRoleRequest = serde_json::from_value(json!({
            "user_id": "user-1",
            "role_id": "role-sales",
            "effective_to": 1700604800,
        }))
        .unwrap();
        let data = request.into_data("admin-1").unwrap();
        assert_eq!(data.user_id, "user-1");
        assert_eq!(data.assigned_by, "admin-1");
        assert_eq!(data.effective_from.unix_secs(), erp_core::common::time::Instant::now().unix_secs());
        assert!(data.effective_to.is_some());
    }

    // 验证审计查询裁剪操作者 ID、保留结果筛选并补齐默认分页大小。
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

pub mod inspection;

pub mod person_scope;
