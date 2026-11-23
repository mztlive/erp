//! 管理端的人员范围、权限检查和审计列表数据。
//!
//! HTTP 处理函数须直接复用此处的请求和响应类型。`page`、`page_size`、`sort_by`、`sort_dir`
//! 与筛选字段同层传递。审计时间用 Unix 秒；权限检查的 `checked_at` 用 RFC 3339 文本。
//! 账号、角色与实际权限绑定由 `service::iam` 管理。

/// 管理端共用分页响应：`items`、`total`、`page` 和 `page_size`。
pub use application_core::PageView;
/// 列表使用的升序或降序。
pub use application_core::SortDir;
/// 供测试复用的排序校验。忽略两端空白，缺省或全空白时使用 `("created_at", Desc)` 的对应默认项。
/// 显式字段须在 `allowed_fields` 中，方向须为 `asc` 或 `desc`，否则返回 `ValidationError`。
#[cfg(test)]
pub(crate) use application_core::normalize_sort;

/// 审计列表使用的已校验分页参数。
pub use crate::entity::access_control::paging::PageParams;

pub mod audit_event;
pub mod inspection;
pub mod person_scope;

pub use audit_event::{AuditEventListParams, AuditEventView};

#[cfg(test)]
mod tests {
    use super::{AuditEventListParams, SortDir, normalize_sort};
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

    // 验证审计查询默认参数与仓储筛选器默认分页一致。
    #[test]
    fn audit_event_list_params_normalize() {
        use crate::repository::AuditEventFilter;

        assert!(AuditEventListParams::default().actor_id.is_none());
        assert_eq!(
            (AuditEventFilter::default().paging.page, AuditEventFilter::default().paging.page_size),
            (1, 20)
        );

        let params = AuditEventListParams {
            actor_id: Some(" user-1 ".to_string()),
            action_type: Some("person_data_scope.replace".to_string()),
            result: Some(AuditEventResult::Denied),
            ..Default::default()
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.actor_id.as_deref(), Some("user-1"));
        assert_eq!(query.result, Some(AuditEventResult::Denied));
        assert_eq!(query.paging.page_size, 20);
    }
}
