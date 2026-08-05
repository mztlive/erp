use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::Result;
use crate::validation::{normalize_optional_text, normalize_required_text};
use crate::AccountKind;

/// 操作人ID最大长度。
const ACTOR_ID_MAX_LEN: usize = 128;
/// 操作人账号最大长度。
const ACTOR_ACCOUNT_MAX_LEN: usize = 64;
/// 审计动作最大长度。
const ACTION_MAX_LEN: usize = 128;
/// 资源类型最大长度。
const RESOURCE_TYPE_MAX_LEN: usize = 64;
/// 资源ID最大长度。
const RESOURCE_ID_MAX_LEN: usize = 64;
/// 消息最大长度。
const MESSAGE_MAX_LEN: usize = 256;

/// 审计日志创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditLogData {
    pub actor_id: String,
    pub actor_account: String,
    pub actor_type: AccountKind,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub success: bool,
    pub message: Option<String>,
}

/// 审计日志实体。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AuditLog {
    #[serde(flatten)]
    pub base: BaseModel,
    pub actor_id: String,
    pub actor_account: String,
    pub actor_type: AccountKind,
    pub action: String,
    pub resource_type: String,
    pub resource_id: Option<String>,
    pub success: bool,
    pub message: Option<String>,
}

impl AuditLog {
    /// 创建新的审计日志。
    ///
    /// # 参数
    /// * `id` - 审计日志ID
    /// * `data` - 审计日志创建数据
    ///
    /// # 返回值
    /// 返回新的审计日志实体
    pub fn new(id: String, data: AuditLogData) -> Result<Self> {
        let actor_id = normalize_required_text(
            data.actor_id,
            "操作人ID不能为空",
            ACTOR_ID_MAX_LEN,
            "操作人ID长度不符合要求",
        )?;
        let actor_account = normalize_required_text(
            data.actor_account,
            "操作人账号不能为空",
            ACTOR_ACCOUNT_MAX_LEN,
            "操作人账号长度不符合要求",
        )?;
        let action =
            normalize_required_text(data.action, "动作不能为空", ACTION_MAX_LEN, "动作长度不符合要求")?;
        let resource_type = normalize_required_text(
            data.resource_type,
            "资源类型不能为空",
            RESOURCE_TYPE_MAX_LEN,
            "资源类型长度不符合要求",
        )?;
        let resource_id = normalize_optional_text(data.resource_id, "资源ID", RESOURCE_ID_MAX_LEN)?;
        let message = normalize_optional_text(data.message, "消息", MESSAGE_MAX_LEN)?;

        Ok(Self {
            base: BaseModel::new(id),
            actor_id,
            actor_account,
            actor_type: data.actor_type,
            action,
            resource_type,
            resource_id,
            success: data.success,
            message,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{AuditLog, AuditLogData};
    use crate::AccountKind;

    fn audit_data() -> AuditLogData {
        AuditLogData {
            actor_id: "actor-1".to_string(),
            actor_account: "admin01".to_string(),
            actor_type: AccountKind::Admin,
            action: "auth.login".to_string(),
            resource_type: "auth".to_string(),
            resource_id: None,
            success: true,
            message: None,
        }
    }

    #[test]
    fn new_should_keep_audit_fields() {
        let log = AuditLog::new("audit-1".to_string(), audit_data()).unwrap();
        assert_eq!(log.actor_id, "actor-1");
        assert_eq!(log.action, "auth.login");
    }

    #[test]
    fn new_should_reject_unbounded_actor_identity() {
        let mut data = audit_data();
        data.actor_account = "x".repeat(65);

        assert!(AuditLog::new("audit-1".to_string(), data).is_err());
    }
}
