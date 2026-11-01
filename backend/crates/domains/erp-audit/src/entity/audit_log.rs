use std::num::NonZeroU32;

use application_core::AuditActor;
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{AccountKind, Result};
use serde::{Deserialize, Serialize};

use super::business_event::BusinessAuditEvent;
use crate::error::{Error as AuditError, Result as AuditResult};

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
const MESSAGE_MAX_LEN: usize = 8192;

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
    /// 类型化业务事件；历史日志缺失时保持缺失。
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub structured_event: Option<BusinessAuditEvent>,
}

impl AuditLog {
    /// 保存同一命令内已预检的写入序号，不改变事件身份和发生时间。
    ///
    /// # 参数
    /// * `sequence` - 从 1 开始的事件序号。
    ///
    /// # 返回
    /// 返回已写入序号的审计日志，不改变 `base` 与 `message`。
    ///
    /// # 错误
    /// 序号为零或缺少结构化事件时返回 `Error::ValidationError`。
    pub fn with_event_sequence(mut self, sequence: u32) -> AuditResult<Self> {
        let sequence = NonZeroU32::new(sequence)
            .ok_or_else(|| AuditError::ValidationError("审计事件序号必须为正整数".into()))?;
        self.structured_event
            .as_mut()
            .ok_or_else(|| AuditError::ValidationError("审计缺少结构化事件".into()))?
            .event_sequence = sequence;
        Ok(self)
    }

    /// 关联入口已经捕获的安全请求编号，内部命令不生成请求号。
    ///
    /// # 参数
    /// * `request_id` - 当前请求的追踪编号；无请求或空白时保持缺失。
    ///
    /// # 返回
    /// 返回带请求关联的审计日志，不改变 `base` 与 `message`。
    ///
    /// # 错误
    /// 编号超长时返回 `Error::Logic`。缺少结构化事件或编号含控制字符时返回 `Error::ValidationError`。
    pub fn with_request_id(mut self, request_id: Option<String>) -> AuditResult<Self> {
        let value = normalize_optional_text(request_id, "请求编号", 128)?;
        if value.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(AuditError::ValidationError("请求编号包含非法字符".into()));
        }
        self.structured_event
            .as_mut()
            .ok_or_else(|| AuditError::ValidationError("审计缺少结构化事件".into()))?
            .request_id = value;
        Ok(self)
    }

    /// 保存认证时已取得的安全操作人名称，不读取当前账户补齐快照。
    ///
    /// # 参数
    /// * `name` - 发生时的操作人名称；未知或空白保持缺失。
    ///
    /// # 返回
    /// 返回保留名称快照、并按该快照重写 `message` 的审计日志。
    ///
    /// # 错误
    /// 名称超长时返回 `Error::Logic`。缺少结构化事件或名称含控制字符时返回 `Error::ValidationError`。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> AuditResult<Self> {
        let value = normalize_optional_text(name, "操作人名称", 128)?;
        if value.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(AuditError::ValidationError("操作人名称包含非法字符".into()));
        }
        let event = self
            .structured_event
            .as_mut()
            .ok_or_else(|| AuditError::ValidationError("审计缺少结构化事件".into()))?;
        event.actor_name_snapshot = value;
        self.message = Some(event.message());
        Ok(self)
    }

    /// 关联独立命令身份，不将幂等键或指纹写入展示事件。
    ///
    /// # 参数
    /// * `command_id` - 服务端稳定命令编号；空白保持缺失。
    ///
    /// # 返回
    /// 返回带命令关联的审计日志，不改变 `base` 与 `message`。
    ///
    /// # 错误
    /// 编号超长时返回 `Error::Logic`。缺少结构化事件或编号含控制字符时返回 `Error::ValidationError`。
    pub fn with_command_id(mut self, command_id: Option<String>) -> AuditResult<Self> {
        let value = normalize_optional_text(command_id, "命令编号", 128)?;
        if value.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(AuditError::ValidationError("命令编号包含非法字符".into()));
        }
        self.structured_event
            .as_mut()
            .ok_or_else(|| AuditError::ValidationError("审计缺少结构化事件".into()))?
            .command_id = value;
        Ok(self)
    }

    /// 保存发生时已明确的安全业务编号，不查询当前对象补齐快照。
    ///
    /// # 参数
    /// * `number` - 当时的业务编号；未知或空白保持缺失。
    ///
    /// # 返回
    /// 返回带编号快照、并按该编号重写 `message` 的审计日志。
    ///
    /// # 错误
    /// 编号超长时返回 `Error::Logic`。缺少结构化事件或编号含控制字符时返回 `Error::ValidationError`。
    pub fn with_resource_number(mut self, number: Option<String>) -> AuditResult<Self> {
        let value = normalize_optional_text(number, "业务编号", 128)?;
        if value.as_ref().is_some_and(|value| value.chars().any(char::is_control)) {
            return Err(AuditError::ValidationError("业务编号包含非法字符".into()));
        }
        let event = self
            .structured_event
            .as_mut()
            .ok_or_else(|| AuditError::ValidationError("审计缺少结构化事件".into()))?;
        event.resource_number_snapshot = value;
        self.message = Some(event.message());
        Ok(self)
    }

    /// 由已鉴权操作人构造成功资源审计的数据。
    ///
    /// `resource_id` 去掉首尾空白后若为空则拒绝，否则按原文放入 `Some`，不会把空白写成缺失目标。不校验动作是否已登记。
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人。只取身份三元组，不保留名称快照和请求号。
    /// * `action` - 审计动作代码。
    /// * `resource_type` - 资源类型稳定代码。
    /// * `resource_id` - 资源业务 ID。
    /// * `message` - 可选业务说明，原样放入创建数据。
    ///
    /// # 返回
    /// 返回 `success` 为真的 `AuditLogData`。
    ///
    /// # 错误
    /// `resource_id` 去空白后为空时返回 `Error::ValidationError`。
    pub fn success_resource_data(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> AuditResult<AuditLogData> {
        if resource_id.trim().is_empty() {
            return Err(AuditError::ValidationError("资源ID不能为空".to_string()));
        }
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        Ok(AuditLogData {
            actor_id,
            actor_account,
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message,
        })
    }

    /// 创建尚未附带结构化事件的审计日志。
    ///
    /// 必填文本会去掉首尾空白。可选文本空白视为缺失。
    ///
    /// # 参数
    /// * `id` - 审计日志 ID，原样写入 `BaseModel`。
    /// * `data` - 审计日志创建数据。
    ///
    /// # 返回
    /// 返回 `structured_event` 为 `None` 的审计日志。
    ///
    /// # 错误
    /// 操作人 ID、操作人账号、动作或资源类型去空白后为空或超长，或资源 ID、消息超长时返回 `erp_core::Error::LogicError`。
    /// 字符数上限分别为操作人 ID 128、账号 64、动作 128、资源类型 64、资源 ID 64、消息 8192。
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
            structured_event: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::{AuditLog, AuditLogData};
    use crate::AuditActorLogs;

    fn audit_actor() -> AuditActor {
        AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
    }

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

    #[test]
    fn success_resource_data_rejects_blank_resource_id() {
        let result = AuditLog::success_resource_data(
            audit_actor(),
            "customer.create",
            "customer",
            "  ".to_string(),
            None,
        );

        assert!(result.is_err());
    }

    #[test]
    fn actor_name_snapshot_keeps_event_identity_and_safe_chinese_message() {
        let log = audit_actor()
            .resource_log_with_message(
                "customer.create",
                "customer",
                "customer-1".to_string(),
                Some("password=private-body".to_string()),
            )
            .unwrap();
        let base = log.base.clone();
        let restored = log.with_actor_name_snapshot(Some("  周晓彤  ".to_string())).unwrap();

        assert_eq!(restored.base, base);
        assert_eq!(
            restored.structured_event.as_ref().unwrap().actor_name_snapshot.as_deref(),
            Some("周晓彤")
        );
        assert!(restored.message.as_ref().unwrap().contains("周晓彤"));
        assert!(restored.message.as_ref().unwrap().contains("创建客户"));
        assert!(!serde_json::to_string(&restored).unwrap().contains("private-body"));
    }

    #[test]
    fn request_and_sequence_setters_preserve_identity_and_safe_projection() {
        let log = audit_actor()
            .resource_log("customer.create", "customer", "customer-1".into())
            .unwrap()
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_command_id(Some("command-1".into()))
            .unwrap();
        let base = log.base.clone();
        let message = log.message.clone();
        let log = log.with_request_id(Some("  trace-1  ".into())).unwrap().with_event_sequence(7).unwrap();
        let event = log.structured_event.as_ref().unwrap();
        assert_eq!(log.base, base);
        assert_eq!(log.message, message);
        assert_eq!(event.event_sequence.get(), 7);
        assert_eq!(event.request_id.as_deref(), Some("trace-1"));
        assert_eq!(event.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(event.command_id.as_deref(), Some("command-1"));
        let internal = log.with_request_id(None).unwrap();
        assert!(internal.structured_event.unwrap().request_id.is_none());
    }

    #[test]
    fn request_and_sequence_setters_reject_unsafe_or_unstructured_values() {
        let log = audit_actor().resource_log("customer.create", "customer", "customer-1".into()).unwrap();
        assert!(log.clone().with_event_sequence(0).is_err());
        assert!(log.clone().with_request_id(Some("x".repeat(129))).is_err());
        assert!(log.with_request_id(Some("trace\nsecret".into())).is_err());
        let raw = AuditLog::new("audit-1".into(), audit_data()).unwrap();
        assert!(raw.clone().with_event_sequence(1).is_err());
        assert!(raw.with_request_id(Some("trace-1".into())).is_err());
    }

    #[test]
    fn actor_name_snapshot_keeps_missing_name_and_rejects_unsafe_input() {
        let log =
            audit_actor().resource_log("customer.create", "customer", "customer-1".to_string()).unwrap();
        let unknown = log.clone().with_actor_name_snapshot(None).unwrap();

        assert_eq!(unknown.structured_event.as_ref().unwrap().actor_name_snapshot, None);
        assert!(log.clone().with_actor_name_snapshot(Some("x".repeat(129))).is_err());
        assert!(log.with_actor_name_snapshot(Some("name\nsecret".to_string())).is_err());
        assert!(
            AuditLog::new("audit-1".to_string(), audit_data())
                .unwrap()
                .with_actor_name_snapshot(Some("周晓彤".to_string()))
                .is_err()
        );
    }
}
