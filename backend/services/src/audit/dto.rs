use entities::{AccountKind, AuditLog};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 审计日志列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct AuditLogListParams {
    pub actor_account: Option<String>,
    pub action: Option<String>,
    pub resource_type: Option<String>,
    pub success: Option<bool>,
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
}

impl AuditLogListParams {
    /// 归一化审计日志列表查询参数。
    ///
    /// # 返回值
    /// 返回不依赖仓储类型的规范化查询参数。
    pub(crate) fn normalized(&self) -> NormalizedAuditLogListParams {
        NormalizedAuditLogListParams {
            actor_account: normalized_text(self.actor_account.as_deref()),
            action: normalized_text(self.action.as_deref()),
            resource_type: normalized_text(self.resource_type.as_deref()),
            success: self.success,
            page: page_or_default(self.page),
            page_size: page_size_or_default(self.page_size),
        }
    }
}

/// Service 内部使用的规范化审计日志列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct NormalizedAuditLogListParams {
    pub(crate) actor_account: Option<String>,
    pub(crate) action: Option<String>,
    pub(crate) resource_type: Option<String>,
    pub(crate) success: Option<bool>,
    pub(crate) page: u64,
    pub(crate) page_size: u32,
}

#[derive(Debug, Serialize)]
pub struct AuditLogItem {
    pub id: String,
    pub actor_id: String,
    pub actor_account: String,
    pub actor_type: AccountKind,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub success: bool,
    pub message: Option<String>,
    pub created_at: u64,
}

impl From<AuditLog> for AuditLogItem {
    /// 将审计日志实体转换为响应结构。
    ///
    /// # 参数
    /// * `log` - 审计日志实体
    ///
    /// # 返回值
    /// 返回审计日志响应结构
    fn from(log: AuditLog) -> Self {
        Self {
            id: log.base.id,
            actor_id: log.actor_id,
            actor_account: log.actor_account,
            actor_type: log.actor_type,
            action: log.action,
            resource_type: log.resource_type,
            resource_id: log.resource_id,
            success: log.success,
            message: log.message,
            created_at: log.base.created_at,
        }
    }
}

#[cfg(test)]
mod tests {
    use entities::AccountKind;
    use serde_json::json;
    use validator::Validate;

    use super::{AuditLogItem, AuditLogListParams};

    #[test]
    fn list_params_normalize_text_and_pagination_defaults() {
        let params = AuditLogListParams {
            actor_account: Some("  admin01 ".into()),
            action: Some("   ".into()),
            resource_type: Some(" consumer ".into()),
            success: Some(true),
            page: None,
            page_size: None,
        };

        let normalized = params.normalized();

        assert_eq!(normalized.actor_account.as_deref(), Some("admin01"));
        assert_eq!(normalized.action, None);
        assert_eq!(normalized.resource_type.as_deref(), Some("consumer"));
        assert_eq!(normalized.success, Some(true));
        assert_eq!(normalized.page, 1);
        assert_eq!(normalized.page_size, 20);
    }

    #[test]
    fn audit_log_item_keeps_snake_case_json_contract() {
        let item = AuditLogItem {
            id: "audit-1".to_string(),
            actor_id: "admin-1".to_string(),
            actor_account: "root".to_string(),
            actor_type: AccountKind::Admin,
            action: "consumer.update".to_string(),
            resource_type: "consumer".to_string(),
            resource_id: Some("consumer-1".to_string()),
            success: true,
            message: None,
            created_at: 42,
        };

        assert_eq!(
            serde_json::to_value(item).unwrap(),
            json!({
                "id": "audit-1",
                "actor_id": "admin-1",
                "actor_account": "root",
                "actor_type": "admin",
                "action": "consumer.update",
                "resource_type": "consumer",
                "resource_id": "consumer-1",
                "success": true,
                "message": null,
                "created_at": 42,
            })
        );
    }

    #[test]
    fn list_params_reject_unbounded_page_size_and_clamp_internal_normalization() {
        let params = AuditLogListParams {
            actor_account: None,
            action: None,
            resource_type: None,
            success: None,
            page: Some(1),
            page_size: Some(u32::MAX),
        };

        assert!(params.validate().is_err());
        assert_eq!(params.normalized().page_size, 100);
    }
}
