//! 独立业务审计事件：动作和安全字段必须由业务边界显式登记。

use std::collections::HashSet;
use std::num::NonZeroU32;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::money::{Amount, Quantity};
use erp_core::validation::normalize_required_text;
use id_generator::next_id;
use serde::{Deserialize, Deserializer, Serialize};

use super::audit_log::{
    RESOURCE_ID_MAX_LEN, RESOURCE_TYPE_MAX_LEN, SNAPSHOT_MAX_LEN, normalized_actor_identity,
    normalized_snapshot_with_message,
};
use super::{AuditAttempt, AuditAttemptKind, AuditAttemptResult, AuditLog, AuditLogData};
use crate::error::{Error, Result};

/// 业务事件持久化 schema 版本。
const EVENT_SCHEMA_VERSION: u16 = 1;
/// 动作、字段及枚举代码上限。
const CODE_MAX_LEN: usize = SNAPSHOT_MAX_LEN;
/// 名称、中文标签及业务编号上限。
const DISPLAY_MAX_LEN: usize = SNAPSHOT_MAX_LEN;
/// 一次业务事件允许记录的投影字段数。
const PROJECTION_MAX_LEN: usize = 64;

/// 由领域登记的稳定动作元数据，与身份域审计动作目录分开维护。
#[derive(Debug, Clone, Copy)]
pub struct AuditAction {
    pub code: &'static str,
    pub resource_type: &'static str,
    pub label: &'static str,
    pub version: u16,
    pub allowed_fields: &'static [AuditField],
}

/// 明确允许记录的业务字段与中文名称。
#[derive(Debug, Clone, Copy)]
pub struct AuditField {
    pub code: &'static str,
    pub label: &'static str,
    pub kind: AuditFieldKind,
}

/// 安全字段类型；不存在任意文本、请求体或实体序列化入口。
#[derive(Debug, Clone, Copy)]
pub enum AuditFieldKind {
    Code(&'static [AuditCode]),
    Quantity,
    Amount,
    Changed,
}

/// 领域明确登记的业务状态代码及其中文名称。
#[derive(Debug, Clone, Copy)]
pub struct AuditCode {
    pub code: &'static str,
    pub label: &'static str,
}

/// 允许保存的最小业务值；敏感变化只能表达为 `Changed`。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
pub enum AuditValue {
    Code { code: String, label: String },
    Quantity { value: Quantity },
    Amount { value: Amount },
    Changed,
}

/// 反序列化使用空结构体标记，拒绝向敏感变化标记附加原值。
#[derive(Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
enum AuditValueInput {
    Code { code: String, label: String },
    Quantity { value: Quantity },
    Amount { value: Amount },
    Changed {},
}

impl<'de> Deserialize<'de> for AuditValue {
    // 通过封闭的安全值结构读取审计投影，防止旧记录或外来输入带入不受控数据。未知 `kind`、未知字段、类型不符或给 `Changed` 附带值都会导致反序列化失败。
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> std::result::Result<Self, D::Error> {
        Ok(match AuditValueInput::deserialize(deserializer)? {
            AuditValueInput::Code { code, label } => Self::Code { code, label },
            AuditValueInput::Quantity { value } => Self::Quantity { value },
            AuditValueInput::Amount { value } => Self::Amount { value },
            AuditValueInput::Changed {} => Self::Changed,
        })
    }
}

/// 命令执行结果，独立于服务结果等业务事实。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum BusinessEventResult {
    Succeeded,
    Rejected,
    Unknown,
}

/// 执行后明确提供的安全业务字段变化。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFieldChange {
    pub field: String,
    pub before: AuditValue,
    pub after: AuditValue,
}

/// 执行后明确提供的安全业务事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditFact {
    pub field: String,
    pub value: AuditValue,
}

/// 业务执行后提供的目标和允许记录的投影。
#[derive(Debug, Clone)]
pub struct BusinessEventContent {
    pub target_id: String,
    pub target_number: Option<String>,
    pub result: BusinessEventResult,
    pub field_changes: Vec<AuditFieldChange>,
    pub facts: Vec<AuditFact>,
}

/// 固定中文字段名后的持久化字段变化。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessAuditFieldChange {
    pub field: String,
    pub field_label: String,
    pub before: AuditValue,
    pub after: AuditValue,
}

/// 固定中文字段名后的持久化业务事实。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessAuditFact {
    pub field: String,
    pub field_label: String,
    pub value: AuditValue,
}

/// 保存于审计日志中的最小结构化业务事件，名称只取发生时的快照。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessAuditEvent {
    pub schema_version: u16,
    /// 同一命令内的事件写入序号，单事件为1，禁止零值。
    pub event_sequence: NonZeroU32,
    pub action_code: String,
    pub action_version: u16,
    pub action_label: String,
    pub actor_id: String,
    pub actor_account: String,
    pub actor_type: AccountKind,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub actor_name_snapshot: Option<String>,
    pub resource_type: String,
    pub resource_id: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub resource_number_snapshot: Option<String>,
    pub result: BusinessEventResult,
    pub field_changes: Vec<BusinessAuditFieldChange>,
    pub facts: Vec<BusinessAuditFact>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command_id: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub request_id: Option<String>,
    pub occurred_at: u64,
}

/// 写入前已验证的动作、操作人和关联上下文。
#[derive(Debug, Clone)]
pub struct BusinessEventContext {
    event_id: String,
    event_sequence: NonZeroU32,
    actor: AuditActor,
    action: AuditAction,
    actor_name_snapshot: Option<String>,
    command_id: Option<String>,
    request_id: Option<String>,
    target_id: Option<String>,
    target_number: Option<String>,
}

impl AuditAction {
    /// 在业务写入前验证审计动作目录，保证展示只使用合法的稳定代码、中文标签和白名单。动作版本为零、字段数量超限、字段或枚举代码重复、代码或标签格式不合规等均返回校验错误。
    pub fn validate(&self) -> Result<()> {
        validate_code(self.code)?;
        validate_code(self.resource_type)?;
        if self.resource_type.len() > RESOURCE_TYPE_MAX_LEN {
            return Err(validation_error("审计资源类型长度不符合要求"));
        }
        validate_label(self.label)?;
        if self.version == 0 || self.allowed_fields.len() > PROJECTION_MAX_LEN {
            return Err(validation_error("动作版本或审计字段数量不符合要求"));
        }

        let mut fields = HashSet::new();
        for field in self.allowed_fields {
            validate_code(field.code)?;
            validate_label(field.label)?;
            if !fields.insert(field.code) {
                return Err(validation_error("审计字段代码重复"));
            }
            validate_field_kind(field.kind)?;
        }
        Ok(())
    }

    // 按代码找出动作允许记录的字段；白名单未登记的字段直接拒绝，不能进入事件。
    fn field(&self, code: &str) -> Result<&AuditField> {
        self.allowed_fields
            .iter()
            .find(|field| field.code == code)
            .ok_or_else(|| validation_error("业务字段未登记在审计白名单中"))
    }
}

impl BusinessEventContext {
    /// 在正式写入前校验操作人 ID、账号及审计动作，并预生成可与回执关联的事件 ID；名称和请求号沿用已鉴权身份中的预校验快照。ID 或账号为空或超长返回 `Error::Logic`；身份含控制字符或动作配置不合法返回 `Error::ValidationError`。
    pub fn new(actor: AuditActor, action: AuditAction) -> Result<Self> {
        action.validate()?;

        // AuditActor 的私有快照字段已在 setter 中规范化并校验。
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (id, account, kind) = actor.into_parts();
        let (id, account) = normalized_actor_identity(id, account)?;
        validate_plain_text(&id)?;
        validate_plain_text(&account)?;

        Ok(Self {
            event_id: next_id(),
            event_sequence: NonZeroU32::MIN,
            actor: AuditActor::new(id, account, kind),
            action,
            actor_name_snapshot,
            command_id: None,
            request_id,
            target_id: None,
            target_number: None,
        })
    }

    /// 登记执行前已经知道的资源 ID 和业务编号，供事务外尝试追溯。空白表示缺失；超长返回 `Error::Logic`，含控制字符返回 `Error::ValidationError`。
    pub fn with_target(mut self, target_id: Option<String>, target_number: Option<String>) -> Result<Self> {
        self.target_id = normalized_snapshot(target_id, "业务对象编号")?;
        self.target_number = normalized_snapshot(target_number, "业务编号")?;
        Ok(self)
    }

    /// 根据失败、拒绝或结果未知的分类生成独立尝试记录，供事务结束后留痕。尝试使用新 ID，未知目标不补值。
    pub fn attempt(&self, result: AuditAttemptResult) -> AuditAttempt {
        AuditAttempt {
            base: BaseModel::new(next_id()),
            schema_version: 1,
            event_kind: AuditAttemptKind::CommandAttempt,
            action_code: self.action.code.into(),
            action_version: self.action.version,
            action_label: self.action.label.into(),
            actor_id: self.actor.id().into(),
            actor_account: self.actor.account().into(),
            actor_type: self.actor.kind(),
            actor_name_snapshot: self.actor_name_snapshot.clone(),
            resource_type: self.action.resource_type.into(),
            resource_id: self.target_id.clone(),
            resource_number_snapshot: self.target_number.clone(),
            command_id: self.command_id.clone(),
            request_id: self.request_id.clone(),
            result,
        }
    }

    /// 提供预分配的事件 ID，供拥有领域在同一事务中写进独立命令回执。
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// 设置同一命令内按业务写入顺序分配的序号，保留原事件 ID。序号从 `1` 开始，传零会被拒绝；调用方必须在首次业务写入前检查批次是否溢出。
    pub fn with_event_sequence(mut self, sequence: u32) -> Result<Self> {
        self.event_sequence =
            NonZeroU32::new(sequence).ok_or_else(|| validation_error("审计事件序号必须为正整数"))?;
        Ok(self)
    }

    /// 冻结本次操作的显示名，供历史事件和失败尝试展示。未知或空白保持缺失，超长返回 `Error::Logic`，含控制字符返回 `Error::ValidationError`。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Result<Self> {
        self.actor_name_snapshot = normalized_snapshot(name, "操作人名称")?;
        Ok(self)
    }

    /// 记录独立业务命令的稳定编号，供事件与领域回执关联；无命令合同或传入空白时保持缺失。超长返回 `Error::Logic`，含控制字符返回 `Error::ValidationError`。
    pub fn with_command_id(mut self, command_id: Option<String>) -> Result<Self> {
        self.command_id = normalized_snapshot(command_id, "命令编号")?;
        Ok(self)
    }

    /// 沿用入口提供的请求追踪号关联同一次调用，非请求入口或传入空白时保持缺失。超长返回 `Error::Logic`，含控制字符返回 `Error::ValidationError`。
    pub fn with_request_id(mut self, request_id: Option<String>) -> Result<Self> {
        self.request_id = normalized_snapshot(request_id, "请求编号")?;
        Ok(self)
    }

    /// 在业务完成后、事务提交前将明确的目标、结果和白名单投影转成结构化中文审计，同时保留兼容旧日志的字段。
    /// 目标为空、目标或编号含控制字符、投影超限、字段重复或未登记、字段类型不符时返回 `Error::ValidationError`；目标、编号或日志文本超长返回 `Error::Logic`，实体构造仍须通过 `AuditLog::new` 校验。
    pub fn log(&self, content: BusinessEventContent) -> Result<AuditLog> {
        let event = self.event(content)?;
        let mut log = AuditLog::new(
            self.event_id.clone(),
            AuditLogData {
                actor_id: self.actor.id().to_string(),
                actor_account: self.actor.account().to_string(),
                actor_type: self.actor.kind(),
                action: self.action.code.to_string(),
                resource_type: self.action.resource_type.to_string(),
                resource_id: Some(event.resource_id.clone()),
                success: event.result == BusinessEventResult::Succeeded,
                message: Some(event.message()),
            },
        )?;

        let mut event = event;
        event.occurred_at = log.base.created_at;
        log.structured_event = Some(event);
        Ok(log)
    }

    // 校验并归一化资源身份、业务编号与投影，再组装结构化事件，避免任意文本混入目标快照。
    fn event(&self, content: BusinessEventContent) -> Result<BusinessAuditEvent> {
        let resource_id = normalize_required_text(
            content.target_id,
            "资源ID不能为空",
            RESOURCE_ID_MAX_LEN,
            "资源ID长度不符合要求",
        )?;
        validate_plain_text(&resource_id)?;
        let resource_number_snapshot = normalized_snapshot(content.target_number, "业务编号")?;

        let (field_changes, facts) = self.projection(content.field_changes, content.facts)?;
        Ok(BusinessAuditEvent {
            schema_version: EVENT_SCHEMA_VERSION,
            event_sequence: self.event_sequence,
            action_code: self.action.code.to_string(),
            action_version: self.action.version,
            action_label: self.action.label.to_string(),
            actor_id: self.actor.id().to_string(),
            actor_account: self.actor.account().to_string(),
            actor_type: self.actor.kind(),
            actor_name_snapshot: self.actor_name_snapshot.clone(),
            resource_type: self.action.resource_type.to_string(),
            resource_id,
            resource_number_snapshot,
            result: content.result,
            field_changes,
            facts,
            command_id: self.command_id.clone(),
            request_id: self.request_id.clone(),
            occurred_at: 0,
        })
    }

    // 按动作白名单校验事实和前后值，再补上登记的展示标签；两类投影共享数量上限和字段去重，防止同一字段被重复记录。
    fn projection(
        &self,
        changes: Vec<AuditFieldChange>,
        facts: Vec<AuditFact>,
    ) -> Result<(Vec<BusinessAuditFieldChange>, Vec<BusinessAuditFact>)> {
        if changes.len() + facts.len() > PROJECTION_MAX_LEN {
            return Err(validation_error("审计投影字段数量超过上限"));
        }

        let mut used_fields = HashSet::new();
        let mut projected_changes = Vec::with_capacity(changes.len());
        for change in changes {
            let field = self.checked_field(&change.field, &mut used_fields)?;
            validate_value(field.kind, &change.before)?;
            validate_value(field.kind, &change.after)?;
            projected_changes.push(BusinessAuditFieldChange {
                field: change.field,
                field_label: field.label.to_string(),
                before: change.before,
                after: change.after,
            });
        }

        let mut projected_facts = Vec::with_capacity(facts.len());
        for fact in facts {
            let field = self.checked_field(&fact.field, &mut used_fields)?;
            validate_value(field.kind, &fact.value)?;
            projected_facts.push(BusinessAuditFact {
                field: fact.field,
                field_label: field.label.to_string(),
                value: fact.value,
            });
        }
        Ok((projected_changes, projected_facts))
    }

    // 取出白名单字段并登记本次使用；已经作为事实或变更出现的代码不能再次进入同一事件。
    fn checked_field<'a>(&'a self, code: &str, used: &mut HashSet<&'static str>) -> Result<&'a AuditField> {
        let field = self.action.field(code)?;
        if !used.insert(field.code) {
            return Err(validation_error("审计投影字段重复"));
        }
        Ok(field)
    }
}

impl BusinessAuditEvent {
    /// 用事件发生时的安全快照生成中文摘要，供列表及时间线展示；没有名称或业务编号时分别回退为账号和资源 ID，只拼接登记过的事实与变化。
    pub fn message(&self) -> String {
        let actor = self.actor_name_snapshot.as_deref().unwrap_or(&self.actor_account);
        let target = self.resource_number_snapshot.as_deref().unwrap_or(&self.resource_id);
        let result = match self.result {
            BusinessEventResult::Succeeded => "成功",
            BusinessEventResult::Rejected => "已拒绝",
            BusinessEventResult::Unknown => "结果待确认",
        };
        let mut message = format!("{actor}执行{}，业务对象：{target}，执行结果：{result}", self.action_label);

        for change in &self.field_changes {
            message.push_str(&format!(
                "；{}：{} → {}",
                change.field_label,
                change.before.display(),
                change.after.display()
            ));
        }

        for fact in &self.facts {
            message.push_str(&format!("；{}：{}", fact.field_label, fact.value.display()));
        }
        message
    }
}

impl AuditValue {
    // 将安全值转成摘要用文本：枚举用登记中文、数量去尾零、金额固定两位小数，敏感值只显示「已变更」。
    fn display(&self) -> String {
        match self {
            Self::Code { label, .. } => label.clone(),
            Self::Quantity { value } => value.to_decimal().normalize().to_string(),
            Self::Amount { value } => format!("{:.2}元", value.to_decimal()),
            Self::Changed => "已变更".to_string(),
        }
    }
}

// 拒绝空的、超限的或含重复代码的枚举白名单，确保审计状态均有有效且唯一的代码与中文标签。
fn validate_field_kind(kind: AuditFieldKind) -> Result<()> {
    let AuditFieldKind::Code(values) = kind else {
        return Ok(());
    };
    if values.is_empty() || values.len() > PROJECTION_MAX_LEN {
        return Err(validation_error("审计状态枚举数量不符合要求"));
    }
    let mut codes = HashSet::new();
    for value in values {
        validate_code(value.code)?;
        validate_label(value.label)?;
        if !codes.insert(value.code) {
            return Err(validation_error("审计状态代码重复"));
        }
    }
    Ok(())
}

// 按登记类型校验每个投影值；枚举必须同时匹配已登记的代码和标签，防止自由文本或错型值落库。
fn validate_value(kind: AuditFieldKind, value: &AuditValue) -> Result<()> {
    let allowed = match (kind, value) {
        (AuditFieldKind::Code(values), AuditValue::Code { code, label }) => {
            values.iter().any(|value| value.code == code && value.label == label)
        },
        (AuditFieldKind::Quantity, AuditValue::Quantity { .. })
        | (AuditFieldKind::Amount, AuditValue::Amount { .. })
        | (AuditFieldKind::Changed, AuditValue::Changed) => true,
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(validation_error("审计字段值不符合登记的安全类型或状态白名单"))
    }
}

// 将稳定动作、资源和字段代码限制在定长以内的 ASCII 字母、数字及 `._:-`，防止自由文本被当成身份。
fn validate_code(code: &str) -> Result<()> {
    if code.is_empty()
        || code.len() > CODE_MAX_LEN
        || !code.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
    {
        return Err(validation_error("审计稳定代码格式不符合要求"));
    }
    Ok(())
}

// 校验展示标签至少含一个中文字符、不超长、没有首尾空白或控制字符，保证事件摘要使用可读且安全的目录文案。
fn validate_label(label: &str) -> Result<()> {
    if label.trim() != label
        || label.chars().count() > DISPLAY_MAX_LEN
        || !label.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        return Err(validation_error("审计中文标签不符合要求"));
    }
    validate_plain_text(label)
}

// 清理名称与关联号等事件快照，只保留限长且无控制字符的文本；空白表示事件发生时未知。
fn normalized_snapshot(value: Option<String>, field: &str) -> Result<Option<String>> {
    normalized_snapshot_with_message(value, field, "审计字段不能包含控制字符")
}

fn validate_plain_text(value: &str) -> Result<()> {
    if value.chars().any(char::is_control) {
        return Err(validation_error("审计字段不能包含控制字符"));
    }
    Ok(())
}

fn validation_error(message: &str) -> Error {
    Error::ValidationError(message.to_string())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::Error as CoreError;
    use serde_json::json;

    use super::*;
    use crate::AuditLogItem;

    const STATUS: &[AuditCode] =
        &[AuditCode { code: "pending", label: "待确认" }, AuditCode { code: "confirmed", label: "已确认" }];
    const RESULTS: &[AuditCode] =
        &[AuditCode { code: "success", label: "服务成功" }, AuditCode { code: "failure", label: "服务失败" }];
    const FIELDS: &[AuditField] = &[
        AuditField { code: "confirmation_status", label: "确认状态", kind: AuditFieldKind::Code(STATUS) },
        AuditField { code: "service_result", label: "服务结果", kind: AuditFieldKind::Code(RESULTS) },
        AuditField { code: "quantity", label: "服务数量", kind: AuditFieldKind::Quantity },
        AuditField { code: "amount", label: "确认金额", kind: AuditFieldKind::Amount },
        AuditField { code: "bank_details", label: "银行资料", kind: AuditFieldKind::Changed },
    ];
    const ACTION: AuditAction = AuditAction {
        code: "service_fulfillment.confirm",
        resource_type: "service_fulfillment",
        label: "确认服务履约",
        version: 1,
        allowed_fields: FIELDS,
    };
    const DUPLICATED_FIELDS: &[AuditField] = &[FIELDS[0], FIELDS[0]];

    fn actor() -> AuditActor {
        AuditActor::new("actor-1".to_string(), "sales".to_string(), AccountKind::Admin)
    }

    fn code(code: &str, label: &str) -> AuditValue {
        AuditValue::Code { code: code.to_string(), label: label.to_string() }
    }

    fn content() -> BusinessEventContent {
        BusinessEventContent {
            target_id: "service-1".to_string(),
            target_number: Some("FW202610040001".to_string()),
            result: BusinessEventResult::Succeeded,
            field_changes: vec![AuditFieldChange {
                field: "confirmation_status".to_string(),
                before: code("pending", "待确认"),
                after: code("confirmed", "已确认"),
            }],
            facts: vec![
                AuditFact { field: "service_result".to_string(), value: code("failure", "服务失败") },
                AuditFact {
                    field: "quantity".to_string(),
                    value: AuditValue::Quantity { value: Quantity::from_str("1.234567").unwrap() },
                },
            ],
        }
    }

    // 验证构建上下文时冻结当次鉴权取得的操作人名称，成功事件和独立尝试均使用该快照。
    #[test]
    fn context_captures_current_actor_name_for_events_and_attempts() {
        let actor = actor().with_actor_name_snapshot(Some("  林晓燕  ".into())).unwrap();
        let context = BusinessEventContext::new(actor, ACTION).unwrap();
        let log = context.log(content()).unwrap();
        assert_eq!(log.structured_event.unwrap().actor_name_snapshot.as_deref(), Some("林晓燕"));
        assert_eq!(
            context.attempt(AuditAttemptResult::Unknown).actor_name_snapshot.as_deref(),
            Some("林晓燕")
        );
    }

    #[test]
    fn context_preserves_validated_actor_snapshot_boundaries() {
        let limit = "名".repeat(128);
        for (input, expected) in [
            (None, None),
            (Some(" \t\n ".into()), None),
            (Some(format!("  {limit}  ")), Some(limit.as_str())),
        ] {
            let actor =
                actor().with_actor_name_snapshot(input.clone()).unwrap().with_request_id(input).unwrap();
            let context = BusinessEventContext::new(actor, ACTION).unwrap();
            let event = context.log(content()).unwrap().structured_event.unwrap();
            let attempt = context.attempt(AuditAttemptResult::Failed);
            assert_eq!(event.actor_name_snapshot.as_deref(), expected);
            assert_eq!(event.request_id.as_deref(), expected);
            assert_eq!(attempt.actor_name_snapshot.as_deref(), expected);
            assert_eq!(attempt.request_id.as_deref(), expected);
        }

        let too_long = "名".repeat(129);
        assert!(matches!(
            actor().with_actor_name_snapshot(Some(too_long.clone())),
            Err(CoreError::LogicError(message)) if message == "操作人名称长度不符合要求"
        ));
        assert!(matches!(
            actor().with_request_id(Some(too_long)),
            Err(CoreError::LogicError(message)) if message == "请求编号长度不符合要求"
        ));

        for invalid in ["名\n称", "名\0称"] {
            assert!(matches!(
                actor().with_actor_name_snapshot(Some(invalid.into())),
                Err(CoreError::LogicError(message)) if message == "操作人名称包含控制字符"
            ));
            assert!(matches!(
                actor().with_request_id(Some(invalid.into())),
                Err(CoreError::LogicError(message)) if message == "请求编号包含控制字符"
            ));
        }
    }

    #[test]
    fn context_keeps_action_then_actor_validation_before_event_creation() {
        let actor = AuditActor::new(" ".into(), "x".repeat(65), AccountKind::Admin)
            .with_actor_name_snapshot(Some("名".repeat(128)))
            .unwrap()
            .with_request_id(Some("r".repeat(128)))
            .unwrap();
        assert!(matches!(
            BusinessEventContext::new(actor.clone(), AuditAction { version: 0, ..ACTION }),
            Err(Error::ValidationError(message)) if message == "动作版本或审计字段数量不符合要求"
        ));
        assert!(matches!(
            BusinessEventContext::new(actor, ACTION),
            Err(Error::Logic(CoreError::LogicError(message))) if message == "操作人ID不能为空"
        ));
    }

    #[test]
    fn context_captures_request_and_assigns_positive_event_sequence() {
        let actor = actor().with_request_id(Some("  trace-1  ".into())).unwrap();
        let context = BusinessEventContext::new(actor, ACTION).unwrap();
        let single = context.log(content()).unwrap();
        let event = single.structured_event.as_ref().unwrap();
        assert_eq!(event.event_sequence.get(), 1);
        assert_eq!(event.request_id.as_deref(), Some("trace-1"));
        assert_eq!(context.attempt(AuditAttemptResult::Failed).request_id.as_deref(), Some("trace-1"));

        let ordered = context.clone().with_event_sequence(7).unwrap().log(content()).unwrap();
        assert_eq!(ordered.base.id, single.base.id);
        assert_eq!(ordered.structured_event.as_ref().unwrap().event_sequence.get(), 7);

        assert!(context.clone().with_event_sequence(0).is_err());
        assert_eq!(
            context
                .with_event_sequence(u32::MAX)
                .unwrap()
                .log(content())
                .unwrap()
                .structured_event
                .unwrap()
                .event_sequence
                .get(),
            u32::MAX
        );
    }

    #[test]
    fn serialized_event_sequence_rejects_zero_and_missing_values() {
        let log = BusinessEventContext::new(actor(), ACTION).unwrap().log(content()).unwrap();
        let mut event = serde_json::to_value(log.structured_event.unwrap()).unwrap();
        assert_eq!(event["event_sequence"], 1);

        event["event_sequence"] = json!(0);
        assert!(serde_json::from_value::<BusinessAuditEvent>(event.clone()).is_err());

        event.as_object_mut().unwrap().remove("event_sequence");
        assert!(serde_json::from_value::<BusinessAuditEvent>(event).is_err());
    }

    #[test]
    fn records_registered_chinese_facts_and_original_snapshots() {
        let context = BusinessEventContext::new(actor(), ACTION)
            .unwrap()
            .with_actor_name_snapshot(Some("周晓彤".to_string()))
            .unwrap()
            .with_command_id(Some("command-1".to_string()))
            .unwrap()
            .with_request_id(Some("request-1".to_string()))
            .unwrap();
        let log = context.log(content()).unwrap();
        assert_eq!(log.base.id, context.event_id());
        assert_eq!(log.action, ACTION.code);
        assert!(log.success);
        assert_eq!(
            log.message.as_deref(),
            Some(
                "周晓彤执行确认服务履约，业务对象：FW202610040001，执行结果：成功；确认状态：待确认 → 已确认；服务结果：服务失败；服务数量：1.234567"
            )
        );

        let event = log.structured_event.unwrap();
        assert_eq!(event.schema_version, 1);
        assert_eq!(event.action_version, 1);
        assert_eq!(event.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("FW202610040001"));
        assert_eq!(event.command_id.as_deref(), Some("command-1"));
        assert_eq!(event.request_id.as_deref(), Some("request-1"));
        assert_eq!(event.result, BusinessEventResult::Succeeded);
        assert_eq!(event.occurred_at, log.base.created_at);
        assert_eq!(event.facts[0].value, code("failure", "服务失败"));
    }

    #[test]
    fn historical_logs_deserialize_without_names_or_extra_null_response_fields() {
        let historical = json!({
            "id":"legacy-1", "version":1, "created_at":123, "updated_at":123, "deleted_at":0,
            "actor_id":"actor-1", "actor_account":"sales", "actor_type":"admin",
            "action":"service_fulfillment.confirm", "resource_type":"service_fulfillment",
            "resource_id":"service-1", "success":true, "message":"原历史记录",
        });
        let log: AuditLog = serde_json::from_value(historical.clone()).unwrap();
        assert_eq!(log.structured_event, None);
        assert_eq!(serde_json::to_value(&log).unwrap(), historical);

        let response = serde_json::to_value(AuditLogItem::from(log)).unwrap();
        assert!(response.get("structured_event").is_none());
        assert_eq!(response["message"], "原历史记录");
    }

    #[test]
    fn optional_unknown_snapshots_stay_missing() {
        let mut data = content();
        data.target_number = None;
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let event = context.log(data).unwrap().structured_event.unwrap();
        assert_eq!(event.actor_name_snapshot, None);
        assert_eq!(event.resource_number_snapshot, None);

        let value = serde_json::to_value(event).unwrap();
        assert!(value.get("actor_name_snapshot").is_none());
        assert!(value.get("resource_number_snapshot").is_none());
        assert!(value.get("command_id").is_none());
        assert!(value.get("request_id").is_none());
    }

    #[test]
    fn rejects_unregistered_projection_and_forged_status_labels() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let mut unregistered = content();
        unregistered.facts.push(AuditFact { field: "password".to_string(), value: AuditValue::Changed });
        assert!(context.log(unregistered).is_err());

        let mut forged = content();
        forged.facts[0].value = code("failure", "银行卡完整值");
        assert!(context.log(forged).is_err());

        let mut wrong_type = content();
        wrong_type.facts[0].value = AuditValue::Amount { value: Amount::from_str("12.34").unwrap() };
        assert!(context.log(wrong_type).is_err());
    }

    #[test]
    fn quantity_and_amount_keep_fixed_decimal_transport() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let mut data = content();
        data.facts.push(AuditFact {
            field: "amount".to_string(),
            value: AuditValue::Amount { value: Amount::from_str("12.30").unwrap() },
        });
        let log = context.log(data).unwrap();
        assert!(log.message.as_deref().unwrap().contains("确认金额：12.30元"));
        let response = serde_json::to_value(AuditLogItem::from(log)).unwrap();
        assert_eq!(
            response["structured_event"]["facts"][1]["value"],
            json!({"kind":"quantity", "value":"1.234567"})
        );
        assert_eq!(
            response["structured_event"]["facts"][2]["value"],
            json!({"kind":"amount", "value":"12.30"})
        );
        assert!(Quantity::from_str("1.2345678").is_err());
    }

    #[test]
    fn sensitive_change_records_only_marker_and_rejects_raw_values() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let mut data = content();
        data.facts.push(AuditFact { field: "bank_details".to_string(), value: AuditValue::Changed });
        let log = context.log(data).unwrap();
        assert!(log.message.as_deref().unwrap().contains("银行资料：已变更"));
        let response = serde_json::to_value(log).unwrap();
        assert_eq!(response["structured_event"]["facts"][2]["value"], json!({"kind":"changed"}));
        assert!(serde_json::from_value::<AuditValue>(json!({"kind":"text", "value":"secret"})).is_err());
        assert!(
            serde_json::from_value::<AuditValue>(json!({"kind":"changed", "value":"ciphertext"})).is_err()
        );
    }

    #[test]
    fn rejects_metadata_actor_target_and_duplicate_projection_before_persistence() {
        let invalid = AuditAction { label: "English action", ..ACTION };
        assert!(BusinessEventContext::new(actor(), invalid).is_err());
        let invalid = AuditAction { version: 0, ..ACTION };
        assert!(BusinessEventContext::new(actor(), invalid).is_err());
        let invalid = AuditAction { allowed_fields: DUPLICATED_FIELDS, ..ACTION };
        assert!(BusinessEventContext::new(actor(), invalid).is_err());

        let invalid_actor = AuditActor::new(" ".to_string(), "sales".to_string(), AccountKind::Admin);
        assert!(BusinessEventContext::new(invalid_actor, ACTION).is_err());

        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let mut data = content();
        data.target_id = " ".to_string();
        assert!(context.log(data).is_err());
        let mut data = content();
        data.facts.push(data.facts[0].clone());
        assert!(context.log(data).is_err());
        assert!(context.with_request_id(Some("request\nforged".to_string())).is_err());
    }

    #[test]
    fn projection_keeps_duplicate_unknown_and_value_error_order() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let duplicate = AuditFact { field: "confirmation_status".into(), value: AuditValue::Changed };
        let unknown = AuditFact { field: "unregistered".into(), value: AuditValue::Changed };
        for (facts, expected) in [
            (vec![duplicate.clone(), unknown.clone()], "审计投影字段重复"),
            (vec![unknown, duplicate], "业务字段未登记在审计白名单中"),
        ] {
            let mut data = content();
            data.facts = facts;
            assert!(matches!(
                context.log(data),
                Err(Error::ValidationError(message)) if message == expected
            ));
        }
    }

    #[test]
    fn unknown_and_rejected_are_explicit_and_not_successful_commands() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        for (result, label) in
            [(BusinessEventResult::Unknown, "结果待确认"), (BusinessEventResult::Rejected, "已拒绝")]
        {
            let mut data = content();
            data.result = result;
            let log = context.log(data).unwrap();
            assert!(!log.success);
            assert!(log.message.as_deref().unwrap().contains(label));
            assert_eq!(log.structured_event.unwrap().result, result);
        }
    }

    #[test]
    fn valid_result_with_no_changes_is_recorded_and_round_trips() {
        let context = BusinessEventContext::new(actor(), ACTION).unwrap();
        let mut data = content();
        data.field_changes.clear();
        data.facts.clear();
        let log = context.log(data).unwrap();
        let json = serde_json::to_value(&log).unwrap();
        let restored: AuditLog = serde_json::from_value(json).unwrap();
        assert_eq!(restored, log);
        assert!(restored.structured_event.unwrap().field_changes.is_empty());
    }
}
