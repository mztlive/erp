//! `audit_event`：安全审计和变更留痕（数据模型 §4.5.4 / W19 §5.2）。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::AuditEventId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

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

/// W19 §5.2 安全审计记录的动作结果，区分成功、拒绝、失败和暂无法确认。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AuditEventResult {
    /// 动作已确认成功。
    Success,
    /// 动作因权限或策略被拒绝。
    Denied,
    /// 动作执行失败。
    Failed,
    /// 动作结果暂无法确认，需以查证后的结论为准。
    Unknown,
}

impl AuditEventResult {
    /// 为审计界面提供「成功」「拒绝」「失败」「结果未知」四种中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "成功",
            Self::Denied => "拒绝",
            Self::Failed => "失败",
            Self::Unknown => "结果未知",
        }
    }

    /// 返回与序列化一致的稳定代码：`SUCCESS`、`DENIED`、`FAILED` 或 `UNKNOWN`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "SUCCESS",
            Self::Denied => "DENIED",
            Self::Failed => "FAILED",
            Self::Unknown => "UNKNOWN",
        }
    }
}

/// 追加身份治理审计事件前，由写入方提供的动作结果、快照和请求关联信息。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AuditEventData {
    /// 发起动作的账号或系统身份 ID。
    pub actor_id: String,
    /// 动作发生时的操作者显示名，供历史展示使用。
    pub actor_label: String,
    /// 动作发生时的责任角色标识，保留当时的身份上下文。
    pub actor_role: String,
    /// 用于分类和检索的审计动作代码，如 `user_role.assign`。
    pub action_type: String,
    /// 动作所涉及的对象类型代码，用于按业务类别追溯。
    pub object_type: String,
    /// 已知的目标记录 ID；无具体记录时可缺省。
    pub object_id: Option<String>,
    /// 可安全展示的目标标题快照，须由写入方排除敏感内容。
    pub object_label: Option<String>,
    /// 入口请求的追踪号，用于关联同一请求产生的事件。
    pub request_id: Option<String>,
    /// 跨服务链路的追踪号，用于关联调用链记录。
    pub trace_id: Option<String>,
    /// 写入方在记录时掌握的动作结果。
    pub result: AuditEventResult,
    /// 以字段名标记「已变更」，不携带旧值或新值。
    pub changed_field_names: Vec<String>,
    /// 写入方提供的带密钥摘要或不可逆摘要引用，须防止离线枚举原值。
    pub safe_digest: Option<String>,
    /// 入口已取得的来源 IP 地址文本。
    pub source_ip: Option<String>,
    /// 入口已取得的安全设备上下文，如系统及版本。
    pub device_context: Option<String>,
}

/// 身份治理的追加式审计事件（数据模型 §4.5.4 / W19 §5.2），禁止编辑和删除。
///
/// 留痕字段与 `audit_log` 的对应字段对齐。敏感字段仅保留变更标记和安全摘要，供历史追溯。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AuditEvent {
    /// 扁平存储的持久化元数据；`created_at` 用作事件发生时间。
    #[serde(flatten)]
    pub base: BaseModel,
    /// 发生动作的操作者 ID，供审计按身份筛选。
    pub actor_id: String,
    /// 留痕时保存的操作者显示名，展示时沿用该快照。
    pub actor_label: String,
    /// 留痕时保存的责任角色，用于还原动作发生时的身份上下文。
    pub actor_role: String,
    /// 已记录的审计动作代码，供分类检索和目录映射。
    pub action_type: String,
    /// 审计目标的对象类型，和对象 ID 一起定位业务记录。
    pub object_type: String,
    /// 审计目标的记录 ID，缺省表示未记录具体目标标识。
    pub object_id: Option<String>,
    /// 审计目标在动作发生时的安全展示标题。
    pub object_label: Option<String>,
    /// 已记录的入口请求追踪号，供按请求反查事件。
    pub request_id: Option<String>,
    /// 已记录的链路追踪号，供跨服务追查。
    pub trace_id: Option<String>,
    /// 事件中记录的动作结果，供审计筛选和展示。
    pub result: AuditEventResult,
    /// 规范化后按首次出现顺序保存的唯一字段名，仅表达变更事实。
    pub changed_field_names: Vec<String>,
    /// 用于安全追溯的摘要或摘要引用，完整敏感值不得入库。
    pub safe_digest: Option<String>,
    /// 动作入口记录的来源 IP 文本，缺省表示未记录。
    pub source_ip: Option<String>,
    /// 动作发生时留存的安全设备上下文，供排查使用。
    pub device_context: Option<String>,
}

impl AuditEvent {
    /// 用调用方分配的 `id` 构造待追加事件，裁剪文本首尾空白并按字段限制字符数。
    /// 必填文本为空、任一文本超长、变更字段名为空或原始字段名列表超过 64 项时返回错误；
    /// 可选空白文本视为缺省，变更字段名去重并保留首次出现的顺序。
    pub fn new(id: AuditEventId, data: AuditEventData) -> Result<Self> {
        let actor_id =
            normalize_required_text(data.actor_id, "操作者ID不能为空", ACTOR_ID_MAX_LEN, "操作者ID过长")?;
        let actor_label = normalize_required_text(
            data.actor_label,
            "操作者名称不能为空",
            ACTOR_LABEL_MAX_LEN,
            "操作者名称过长",
        )?;
        let actor_role =
            normalize_required_text(data.actor_role, "责任角色不能为空", ACTOR_ROLE_MAX_LEN, "责任角色过长")?;

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

// 去重前限制原始列表最多 64 项；逐项裁剪空白，拒绝空名和超过 128 字符的名称，
// 再按首次出现的顺序保留不同字段，控制审计体积。
fn normalize_field_names(field_names: Vec<String>) -> Result<Vec<String>> {
    if field_names.len() > MAX_CHANGED_FIELDS {
        return Err(Error::from(format!("变更字段名数量不能超过 {MAX_CHANGED_FIELDS}")));
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
    use erp_core::ids::AuditEventId;

    use super::{AuditEvent, AuditEventData, AuditEventResult};

    // 构造带空白和重复变更字段名的销售单审批样本，供实体校验和往返测试复用。
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

    // 确认构造时裁剪操作者和目标等文本，保序去重变更字段名并保留动作结果。
    #[test]
    fn new_trims_fields_and_deduplicates_field_names() {
        let event = AuditEvent::new(AuditEventId::new("ae-1"), data()).unwrap();
        assert_eq!(event.actor_id, "user-1");
        assert_eq!(event.actor_label, "张三");
        assert_eq!(event.object_id.as_deref(), Some("SO-1"));
        assert_eq!(event.source_ip.as_deref(), Some("10.0.0.1"));
        assert_eq!(event.changed_field_names, vec!["status", "gross_amount"], "重复字段名去重且保序");
        assert_eq!(event.result, AuditEventResult::Success);
    }

    // 确认动作代码只有空白时无法构造事件。
    #[test]
    fn new_rejects_empty_action_type() {
        let payload = AuditEventData { action_type: "  ".to_string(), ..data() };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    // 确认变更字段名列表超过 64 项时拒绝写入输入。
    #[test]
    fn new_rejects_too_many_changed_fields() {
        let payload = AuditEventData {
            changed_field_names: (0..super::MAX_CHANGED_FIELDS + 1).map(|i| format!("field_{i}")).collect(),
            ..data()
        };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    // 确认可选的安全标题有值时仍受 256 字符上限约束。
    #[test]
    fn new_rejects_overlong_object_label() {
        let payload = AuditEventData { object_label: Some("x".repeat(257)), ..data() };
        assert!(AuditEvent::new(AuditEventId::new("ae-1"), payload).is_err());
    }

    // 固定拒绝的序列化值、未知的稳定代码及成功和失败的中文标签。
    #[test]
    fn result_codes_and_labels_are_stable() {
        assert_eq!(serde_json::to_string(&AuditEventResult::Denied).unwrap(), "\"DENIED\"");
        assert_eq!(AuditEventResult::Unknown.as_str(), "UNKNOWN");
        assert_eq!(AuditEventResult::Failed.label(), "失败");
        assert_eq!(AuditEventResult::Success.label(), "成功");
    }

    // 确认 JSON 序列化与反序列化保留完整审计事件。
    #[test]
    fn entity_roundtrips_through_bson() {
        let event = AuditEvent::new(AuditEventId::new("ae-1"), data()).unwrap();
        let roundtrip: AuditEvent = serde_json::from_str(&serde_json::to_string(&event).unwrap()).unwrap();
        assert_eq!(roundtrip, event);
    }
}
