//! 独立业务审计事件：动作和安全字段必须由业务边界显式登记。

use std::collections::HashSet;
use std::num::NonZeroU32;

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::AccountKind;
use erp_core::money::{Amount, Quantity};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use id_generator::next_id;
use serde::{Deserialize, Deserializer, Serialize};

use super::{AuditAttempt, AuditAttemptKind, AuditAttemptResult, AuditLog, AuditLogData};
use crate::error::{Error, Result};

/// 业务事件持久化 schema 版本。
const EVENT_SCHEMA_VERSION: u16 = 1;
/// 动作、字段及枚举代码上限。
const CODE_MAX_LEN: usize = 128;
/// 名称、中文标签及业务编号上限。
const DISPLAY_MAX_LEN: usize = 128;
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
    /// 从封闭的安全值变体反序列化，拒绝未知字段和任意文本值。
    ///
    /// # 参数
    /// * `deserializer` - serde 反序列化器。
    ///
    /// # 返回
    /// 返回对应的 `AuditValue`。`Changed` 只接受空对象。
    ///
    /// # 错误
    /// `kind` 未知、字段类型不匹配、出现未知字段，或 `Changed` 附带字段时返回反序列化错误。
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
    /// 校验领域登记的稳定动作、中文标签及安全字段白名单。
    ///
    /// # 参数
    /// 无额外参数。
    ///
    /// # 返回
    /// 元数据有效时返回 `Ok(())`。
    ///
    /// # 错误
    /// 代码、中文标签、版本、字段类型或字段/枚举代码重复时返回校验错误。
    pub fn validate(&self) -> Result<()> {
        validate_code(self.code)?;
        validate_code(self.resource_type)?;
        if self.resource_type.len() > 64 {
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

    /// 未登记字段不能进入审计投影。
    fn field(&self, code: &str) -> Result<&AuditField> {
        self.allowed_fields
            .iter()
            .find(|field| field.code == code)
            .ok_or_else(|| validation_error("业务字段未登记在审计白名单中"))
    }
}

impl BusinessEventContext {
    /// 在业务写入前验证操作人和动作，并生成可供回执关联的事件编号。
    ///
    /// # 参数
    /// * `actor` - 已通过调用入口鉴权的操作人。
    /// * `action` - 由领域显式登记的稳定动作及安全字段。
    ///
    /// # 返回
    /// 返回已完成静态元数据校验的事件上下文。
    ///
    /// # 错误
    /// 操作人 ID 或账号去空白后为空或超长，或名称快照、请求号超长时返回 `Error::Logic`。
    /// 身份含控制字符，或动作代码、标签、版本、字段白名单无效时返回 `Error::ValidationError`。
    pub fn new(actor: AuditActor, action: AuditAction) -> Result<Self> {
        action.validate()?;
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (id, account, kind) = actor.into_parts();
        let id = normalize_required_text(id, "操作人ID不能为空", 128, "操作人ID长度不符合要求")?;
        let account = normalize_required_text(account, "操作人账号不能为空", 64, "操作人账号长度不符合要求")?;
        validate_plain_text(&id)?;
        validate_plain_text(&account)?;
        let actor_name_snapshot = normalized_snapshot(actor_name_snapshot, "操作人名称")?;
        let request_id = normalized_snapshot(request_id, "请求编号")?;
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

    /// 登记执行前已经明确的对象身份，供独立尝试记录使用。
    /// # 参数
    /// * `target_id` - 业务对象身份；未知明确保持缺失。
    /// * `target_number` - 执行前已取得的安全编号快照。
    /// # 返回
    /// 返回带安全目标的上下文。
    /// # 错误
    /// 目标或业务编号超长时返回 `Error::Logic`。含控制字符时返回 `Error::ValidationError`。空白视为缺失，不报错。
    pub fn with_target(mut self, target_id: Option<String>, target_number: Option<String>) -> Result<Self> {
        self.target_id = normalized_snapshot(target_id, "业务对象编号")?;
        self.target_number = normalized_snapshot(target_number, "业务编号")?;
        Ok(self)
    }

    /// 为已完成事务的失败、拒绝或未知结果建立独立尝试快照。
    /// # 参数
    /// * `result` - 结构化错误分类，不能填写成功。
    /// # 返回
    /// 返回新身份的安全尝试实体，目标缺失保持缺失。
    /// # 错误
    /// 不返回错误。上下文已在写入前完成校验。
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

    /// 返回本次新鲜执行所关联的审计事件编号。
    ///
    /// # 参数
    /// 无额外参数。
    ///
    /// # 返回
    /// 返回事件编号的借用，可同事务保存于独立命令回执。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn event_id(&self) -> &str {
        &self.event_id
    }

    /// 登记同一命令内按原写入顺序分配的事件序号。
    /// # 参数
    /// * `sequence` - 从1开始的已预检序号。
    /// # 返回
    /// 返回保留稳定事件身份的上下文。
    /// # 错误
    /// 序号为零时拒绝，批次溢出由调用方在首个业务写入前拒绝。
    pub fn with_event_sequence(mut self, sequence: u32) -> Result<Self> {
        self.event_sequence =
            NonZeroU32::new(sequence).ok_or_else(|| validation_error("审计事件序号必须为正整数"))?;
        Ok(self)
    }

    /// 登记发生时已知的安全操作人名称快照。
    ///
    /// # 参数
    /// * `name` - 当时的显示名；未知保持 `None`。
    ///
    /// # 返回
    /// 返回保存名称快照的上下文。
    ///
    /// # 错误
    /// 名称超长时返回 `Error::Logic`。含控制字符时返回 `Error::ValidationError`。空白视为缺失，不报错。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Result<Self> {
        self.actor_name_snapshot = normalized_snapshot(name, "操作人名称")?;
        Ok(self)
    }

    /// 登记独立业务命令的稳定关联编号。
    ///
    /// # 参数
    /// * `command_id` - 独立命令编号；无命令合同的入口保持 `None`。
    ///
    /// # 返回
    /// 返回保存命令关联的上下文。
    ///
    /// # 错误
    /// 编号超长时返回 `Error::Logic`。含控制字符时返回 `Error::ValidationError`。空白视为缺失，不报错。
    pub fn with_command_id(mut self, command_id: Option<String>) -> Result<Self> {
        self.command_id = normalized_snapshot(command_id, "命令编号")?;
        Ok(self)
    }

    /// 登记调用入口提供的请求关联编号。
    ///
    /// # 参数
    /// * `request_id` - 请求追踪号；非请求入口可以保持 `None`。
    ///
    /// # 返回
    /// 返回保存请求关联的上下文。
    ///
    /// # 错误
    /// 编号超长时返回 `Error::Logic`。含控制字符时返回 `Error::ValidationError`。空白视为缺失，不报错。
    pub fn with_request_id(mut self, request_id: Option<String>) -> Result<Self> {
        self.request_id = normalized_snapshot(request_id, "请求编号")?;
        Ok(self)
    }

    /// 在业务执行后、事务提交前校验安全投影并构造中文审计。
    ///
    /// # 参数
    /// * `content` - 明确的目标、业务结果和安全字段变化/事实。
    ///
    /// # 返回
    /// 返回兼容旧字段并附带结构化事件的审计日志。
    ///
    /// # 错误
    /// 目标为空、超长或含控制字符，业务编号超长或含控制字符，投影超过上限、字段重复或未登记，
    /// 字段值不符合登记类型，或审计消息等文本无法通过 `AuditLog::new` 时返回错误。
    /// 文本超长映射为 `Error::Logic`，其余校验为 `Error::ValidationError`。
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

    /// 持久化前先收紧目标和编号，避免自由文本进入结构化事件。
    fn event(&self, content: BusinessEventContent) -> Result<BusinessAuditEvent> {
        let resource_id =
            normalize_required_text(content.target_id, "资源ID不能为空", 64, "资源ID长度不符合要求")?;
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

    /// 限制投影规模，并阻止重复或未登记字段进入事件。
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

    /// 同一事件里每个字段代码只能出现一次。
    fn checked_field<'a>(&'a self, code: &str, used: &mut HashSet<String>) -> Result<&'a AuditField> {
        if !used.insert(code.to_string()) {
            return Err(validation_error("审计投影字段重复"));
        }
        self.action.field(code)
    }
}

impl BusinessAuditEvent {
    /// 按事件发生时的安全快照生成中文展示说明。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回稳定中文动作、执行结果及已登记字段事实。
    /// # 错误
    /// 不返回错误。
    pub fn message(&self) -> String {
        let actor = self.actor_name_snapshot.as_deref().unwrap_or(&self.actor_account);
        let target = self.resource_number_snapshot.as_deref().unwrap_or(&self.resource_id);
        let result = match self.result {
            BusinessEventResult::Succeeded => "成功",
            BusinessEventResult::Rejected => "已拒绝",
            BusinessEventResult::Unknown => "结果待确认",
        };
        let mut parts =
            vec![format!("{actor}执行{}，业务对象：{target}，执行结果：{result}", self.action_label)];
        parts.extend(self.field_changes.iter().map(|change| {
            format!("{}：{} → {}", change.field_label, change.before.display(), change.after.display())
        }));
        parts.extend(self.facts.iter().map(|fact| format!("{}：{}", fact.field_label, fact.value.display())));
        parts.join("；")
    }
}

impl AuditValue {
    /// 展示只使用登记过的中文标签和安全值，金额保留两位小数。
    fn display(&self) -> String {
        match self {
            Self::Code { label, .. } => label.clone(),
            Self::Quantity { value } => value.to_decimal().normalize().to_string(),
            Self::Amount { value } => format!("{:.2}元", value.to_decimal()),
            Self::Changed => "已变更".to_string(),
        }
    }
}

/// 枚举字段必须有不重复的合法状态，避免空白名单或重复代码。
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

/// 拒绝与登记类型不符的值，避免任意文本入库。
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

/// 稳定代码限制为有限字符集，避免自由文本充当动作或字段身份。
fn validate_code(code: &str) -> Result<()> {
    if code.is_empty()
        || code.len() > CODE_MAX_LEN
        || !code.bytes().all(|byte| byte.is_ascii_alphanumeric() || b"._:-".contains(&byte))
    {
        return Err(validation_error("审计稳定代码格式不符合要求"));
    }
    Ok(())
}

/// 展示标签必须是有限长度的中文，避免空白或控制字符。
fn validate_label(label: &str) -> Result<()> {
    if label.trim() != label
        || label.chars().count() > DISPLAY_MAX_LEN
        || !label.chars().any(|ch| ('\u{4e00}'..='\u{9fff}').contains(&ch))
    {
        return Err(validation_error("审计中文标签不符合要求"));
    }
    validate_plain_text(label)
}

/// 快照只保留有限安全文本，空白表示当时未知。
fn normalized_snapshot(value: Option<String>, field: &str) -> Result<Option<String>> {
    let value = normalize_optional_text(value, field, DISPLAY_MAX_LEN)?;
    if let Some(value) = &value {
        validate_plain_text(value)?;
    }
    Ok(value)
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

    /// 构造器直接捕获当次鉴权名称，成功事件与独立尝试保留同一冻结快照。
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
