//! `audit_event`：安全审计和变更留痕（数据模型 §4.5.4 / W19 §5.2）。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::AuditEventId;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 操作者 ID 最大长度。
const ACTOR_ID_MAX_LEN: usize = 128;
/// 操作者名称快照最大长度。
const ACTOR_LABEL_MAX_LEN: usize = 128;
/// 责任角色快照最大长度。
const ACTOR_ROLE_MAX_LEN: usize = 128;
/// 动作代码最大长度。
const ACTION_TYPE_MAX_LEN: usize = 128;
/// 对象类型代码最大长度。
const OBJECT_TYPE_MAX_LEN: usize = 64;
/// 对象 ID 最大长度。
const OBJECT_ID_MAX_LEN: usize = 128;
/// 对象安全标题最大长度。
const OBJECT_LABEL_MAX_LEN: usize = 256;
/// 请求追踪号最大长度。
const TRACE_ID_MAX_LEN: usize = 128;
/// 字段名最大长度。
const FIELD_NAME_MAX_LEN: usize = 128;
/// 变更字段名数量上限。
const MAX_CHANGED_FIELDS: usize = 64;
/// 安全摘要最大长度。
const SAFE_DIGEST_MAX_LEN: usize = 128;
/// 来源 IP 最大长度。
const SOURCE_IP_MAX_LEN: usize = 64;
/// 设备上下文最大长度。
const DEVICE_CONTEXT_MAX_LEN: usize = 256;

/// 审计结果（W19 §5.2：成功、拒绝、失败、结果未知后的最终结论）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditEventResult {
    /// 成功。
    Success,
    /// 拒绝。
    Denied,
    /// 失败。
    Failed,
    /// 结果未知。
    Unknown,
}

impl AuditEventResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "成功",
            Self::Denied => "拒绝",
            Self::Failed => "失败",
            Self::Unknown => "结果未知",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Denied => "DENIED",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// 审计事件创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventData {
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照（使用当时显示名，不用当前角色覆盖）。
    pub actor_label: String,
    /// 动作发生时责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 链路追踪号。
    pub trace_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（只记录字段名和「已变更」，不记录敏感旧值或新值）。
    pub changed_field_names: Vec<String>,
    /// 安全摘要（带密钥摘要或不可逆摘要引用，不能据此离线枚举原值）。
    pub safe_digest: Option<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 设备上下文。
    pub device_context: Option<String>,
}

/// 审计事件实体（数据模型 §4.5.4 / W19 §5.2）。
///
/// 追加式留痕，不可编辑、不可删除；敏感字段只记录「已变更」与摘要，不记录
/// 完整旧值或新值（§4.5.4）。字段与既有 `entities::audit_log` 对齐（domains.md
/// 注：`audit_log → audit_event` 字段对齐），事件追加使用 `BaseModel` 持久化
/// 元数据，`created_at` 即事件发生时间。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AuditEvent {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 操作者 ID。
    pub actor_id: String,
    /// 操作者名称快照。
    pub actor_label: String,
    /// 责任角色快照。
    pub actor_role: String,
    /// 动作代码。
    pub action_type: String,
    /// 业务对象类型代码。
    pub object_type: String,
    /// 业务对象 ID。
    pub object_id: Option<String>,
    /// 业务对象安全标题。
    pub object_label: Option<String>,
    /// 请求追踪号。
    pub request_id: Option<String>,
    /// 链路追踪号。
    pub trace_id: Option<String>,
    /// 最终结果。
    pub result: AuditEventResult,
    /// 变更字段名（去重保序）。
    pub changed_field_names: Vec<String>,
    /// 安全摘要。
    pub safe_digest: Option<String>,
    /// 来源 IP。
    pub source_ip: Option<String>,
    /// 设备上下文。
    pub device_context: Option<String>,
}

impl AuditEvent {
    /// 创建审计事件。
    ///
    /// 完成全部文本字段的校验与规范化（trim、非空、长度上限）；`changed_field_names`
    /// 逐项 trim、去重、保序，数量不超过上限（防审计膨胀）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::AuditEventId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的审计事件。
    ///
    /// # 错误
    /// 当必填字段为空/超长，或变更字段名数量越界时返回错误。
    pub fn new(id: AuditEventId, data: AuditEventData) -> Result<Self> {
        let actor_id = normalize_required_text(
            data.actor_id,
            "操作者ID不能为空",
            ACTOR_ID_MAX_LEN,
            "操作者ID过长",
        )?;
        let actor_label = normalize_required_text(
            data.actor_label,
            "操作者名称不能为空",
            ACTOR_LABEL_MAX_LEN,
            "操作者名称过长",
        )?;
        let actor_role = normalize_required_text(
            data.actor_role,
            "责任角色不能为空",
            ACTOR_ROLE_MAX_LEN,
            "责任角色过长",
        )?;
        let action_type =
            normalize_required_text(data.action_type, "动作不能为空", ACTION_TYPE_MAX_LEN, "动作过长")?;
        let object_type = normalize_required_text(
            data.object_type,
            "对象类型不能为空",
            OBJECT_TYPE_MAX_LEN,
            "对象类型过长",
        )?;
        let object_id = normalize_optional_text(data.object_id, "对象ID", OBJECT_ID_MAX_LEN)?;
        let object_label = normalize_optional_text(data.object_label, "对象标题", OBJECT_LABEL_MAX_LEN)?;
        let request_id = normalize_optional_text(data.request_id, "请求追踪号", TRACE_ID_MAX_LEN)?;
        let trace_id = normalize_optional_text(data.trace_id, "链路追踪号", TRACE_ID_MAX_LEN)?;
        let changed_field_names = normalize_field_names(data.changed_field_names)?;
        let safe_digest = normalize_optional_text(data.safe_digest, "安全摘要", SAFE_DIGEST_MAX_LEN)?;
        let source_ip = normalize_optional_text(data.source_ip, "来源IP", SOURCE_IP_MAX_LEN)?;
        let device_context =
            normalize_optional_text(data.device_context, "设备上下文", DEVICE_CONTEXT_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            actor_id,
            actor_label,
            actor_role,
            action_type,
            object_type,
            object_id,
            object_label,
            request_id,
            trace_id,
            result: data.result,
            changed_field_names,
            safe_digest,
            source_ip,
            device_context,
        })
    }
}

/// 规范化变更字段名列表：trim、去重、保序、数量与长度上限。
///
/// # 参数
/// * `field_names` - 原始字段名列表
///
/// # 返回
/// 返回去重保序后的字段名列表。
///
/// # 错误
/// 当数量超过上限或任一字段名为空/超长时返回错误。
fn normalize_field_names(field_names: Vec<String>) -> Result<Vec<String>> {
    if field_names.len() > MAX_CHANGED_FIELDS {
        return Err(Error::from(format!(
            "变更字段名数量不能超过 {MAX_CHANGED_FIELDS}"
        )));
    }
    let mut seen = HashSet::new();
    let mut normalized = Vec::with_capacity(field_names.len());
    for name in field_names {
        let name = normalize_required_text(name, "变更字段名不能为空", FIELD_NAME_MAX_LEN, "变更字段名过长")?;
        if seen.insert(name.clone()) {
            normalized.push(name);
        }
    }
    Ok(normalized)
}

#[cfg(test)]
mod tests {
    use super::{AuditEvent, AuditEventData, AuditEventResult};
    use crate::ids::AuditEventId;

    fn data() -> AuditEventData {
        AuditEventData {
            actor_id: " user-1 ".to_string(),
            actor_label: " 张三 ".to_string(),
            actor_role: "sales".to_string(),
            action_type: "sales_order.approve".to_string(),
            object_type: "sales_order".to_string(),
            object_id: Some(" SO-1 ".to_string()),
            object_label: Some(" 销售单 SO-1 ".to_string()),
            request_id: Some("req-1".to_string()),
            trace_id: Some("trace-1".to_string()),
            result: AuditEventResult::Success,
            changed_field_names: vec![
                " status ".to_string(),
                "status".to_string(),
                "gross_amount".to_string(),
            ],
            safe_digest: Some("digest-1".to_string()),
            source_ip: Some(" 10.0.0.1 ".to_string()),
            device_context: Some("mac/13".to_string()),
        }
    }

    /// happy path：字段 trim、字段名去重保序。
    #[test]
    fn new_trims_fields_and_deduplicates_field_names() {
        let event = AuditEvent::new(AuditEventId::new("ae-1"), data()).unwrap();
        assert_eq!(event.actor_id, "user-1");
        assert_eq!(event.actor_label, "张三");
        assert_eq!(event.object_id.as_deref(), Some("SO-1"));
        assert_eq!(event.source_ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(
            event.changed_field_names,
            vec!["status", "gross_amount"],
            "重复字段名去重且保序"
        );
        assert_eq!(event.result, AuditEventResult::Success);
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_action_type() {
        let payload = AuditEventData {
            action_type: "  ".to_string(),
            ..data()
        };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    /// 失败路径：字段名数量越界被拒。
    #[test]
    fn new_rejects_too_many_changed_fields() {
        let payload = AuditEventData {
            changed_field_names: (0..super::MAX_CHANGED_FIELDS + 1)
                .map(|i| format!("field_{i}"))
                .collect(),
            ..data()
        };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    /// 失败路径：超长对象标题被拒。
    #[test]
    fn new_rejects_overlong_object_label() {
        let payload = AuditEventData {
            object_label: Some("x".repeat(257)),
            ..data()
        };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn result_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&AuditEventResult::Denied).unwrap(),
            "\"DENIED\""
        );
        assert_eq!(AuditEventResult::Unknown.as_str(), "UNKNOWN");
        assert_eq!(AuditEventResult::Failed.label(), "失败");
        assert_eq!(AuditEventResult::Success.label(), "成功");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let event = AuditEvent::new(AuditEventId::new("ae-1"), data()).unwrap();
        let roundtrip: AuditEvent = bson::from_document(bson::to_document(&event).unwrap()).unwrap();
        assert_eq!(roundtrip, event);
    }
}
