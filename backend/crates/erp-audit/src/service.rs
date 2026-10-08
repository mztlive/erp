use application_core::{AuditActor, Page};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

use crate::Error;
use crate::catalog::registered_action;
pub use crate::dto::{AuditLogItem, AuditLogListParams};
use crate::entity::{
    AuditLog, AuditLogData, BusinessEventContent, BusinessEventContext, BusinessEventResult,
};
use crate::error::Result;
use crate::repository::prelude::*;
use crate::repository::{AuditExt, AuditLogFilter};

/// 由审计领域消费 [`AuditActor`] 构造可持久化审计日志。
pub trait AuditActorLogs {
    /// 在业务写入前构造并验证成功资源审计日志。
    ///
    /// 生成新的稳定 ID，并保留操作人上已经捕获的名称快照和请求号。最终摘要来自已登记动作，不接受自由正文。
    ///
    /// # 参数
    /// * `action` - 已登记的稳定动作代码。
    /// * `resource_type` - 与动作配对的资源类型代码。
    /// * `resource_id` - 资源业务 ID。
    ///
    /// # 返回
    /// 返回 `success` 为真、并带结构化事件的审计日志。
    ///
    /// # 错误
    /// `resource_id` 去空白后为空时返回 `Error::ValidationError`。
    /// 操作人、动作或资源类型为空或超长，或资源 ID 超长时返回 `Error::Logic`。
    /// 动作未登记、与资源类型不匹配、登记元数据无效，或身份、目标含控制字符时返回 `Error::ValidationError`。
    fn resource_log(self, action: &str, resource_type: &str, resource_id: String) -> Result<AuditLog>;

    /// 使用调用方给出的稳定 ID 构造成功资源审计日志。
    ///
    /// `message` 只参与长度校验，成功后会被结构化中文摘要替换。保留操作人上已经捕获的名称快照和请求号。
    ///
    /// # 参数
    /// * `id` - 调用方提供的审计日志 ID。
    /// * `action` - 已登记的稳定动作代码。
    /// * `resource_type` - 与动作配对的资源类型代码。
    /// * `resource_id` - 资源业务 ID。
    /// * `message` - 可选自由说明，不会写入最终摘要。
    ///
    /// # 返回
    /// 返回使用该 `id`、`success` 为真并带结构化事件的审计日志。
    ///
    /// # 错误
    /// `resource_id` 去空白后为空时返回 `Error::ValidationError`。
    /// 操作人、动作或资源类型为空或超长，资源 ID 或 `message` 超长时返回 `Error::Logic`。
    /// 动作未登记、与资源类型不匹配、登记元数据无效，或身份、目标含控制字符时返回 `Error::ValidationError`。
    fn resource_log_with_id(
        self,
        id: String,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog>;

    /// 生成新的稳定 ID，并构造成功资源审计日志。
    ///
    /// `message` 只参与长度校验，成功后会被结构化中文摘要替换。保留操作人上已经捕获的名称快照和请求号。
    ///
    /// # 参数
    /// * `action` - 已登记的稳定动作代码。
    /// * `resource_type` - 与动作配对的资源类型代码。
    /// * `resource_id` - 资源业务 ID。
    /// * `message` - 可选自由说明，不会写入最终摘要。
    ///
    /// # 返回
    /// 返回新 ID 的成功审计日志。
    ///
    /// # 错误
    /// `resource_id` 去空白后为空时返回 `Error::ValidationError`。
    /// 操作人、动作或资源类型为空或超长，资源 ID 或 `message` 超长时返回 `Error::Logic`。
    /// 动作未登记、与资源类型不匹配、登记元数据无效，或身份、目标含控制字符时返回 `Error::ValidationError`。
    fn resource_log_with_message(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog>;
}

impl AuditActorLogs for AuditActor {
    fn resource_log(self, action: &str, resource_type: &str, resource_id: String) -> Result<AuditLog> {
        self.resource_log_with_message(action, resource_type, resource_id, None)
    }

    fn resource_log_with_id(
        self,
        id: String,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog> {
        let actor_name_snapshot = self.actor_name_snapshot().map(str::to_string);
        let request_id = self.request_id().map(str::to_string);
        let data = AuditLog::success_resource_data(self, action, resource_type, resource_id, message)?;
        let mut prepared = prepare_business_log(&AuditLog::new(id, data)?)?;
        if let Some(event) = &mut prepared.structured_event {
            event.actor_name_snapshot = actor_name_snapshot;
            event.request_id = request_id;
            prepared.message = Some(event.message());
        }
        Ok(prepared)
    }

    fn resource_log_with_message(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog> {
        self.resource_log_with_id(next_id(), action, resource_type, resource_id, message)
    }
}

/// 将普通资源日志转换为已登记的安全业务事件；自由正文不属于事实投影。
///
/// # 参数
/// * `log` - 事务前已准备的动作、操作人与目标快照。
///
/// # 返回
/// 返回保留事件身份和时间的结构化中文记录。已有结构化事件时只重写摘要。
///
/// # 错误
/// 动作未登记、与资源类型不匹配或登记元数据无效时返回 `Error::ValidationError`。
/// 已有结构化事件在 schema、动作版本、动作、资源、操作人或成功标记上与日志不一致时同样拒绝。
/// 没有结构化事件时，操作人或目标无法通过静态校验，或目标缺失，也会拒绝。
pub fn prepare_business_log(log: &AuditLog) -> Result<AuditLog> {
    let action = registered_action(&log.action, &log.resource_type)?;
    if let Some(event) = &log.structured_event {
        if event.schema_version != 1
            || event.action_version == 0
            || event.action_code != log.action
            || event.resource_type != log.resource_type
            || Some(event.resource_id.as_str()) != log.resource_id.as_deref()
            || event.actor_id != log.actor_id
            || event.actor_account != log.actor_account
            || event.actor_type != log.actor_type
            || log.success != (event.result == BusinessEventResult::Succeeded)
        {
            return Err(Error::ValidationError("结构化审计与业务身份不一致".to_string()));
        }
        let mut prepared = log.clone();
        prepared.message = Some(event.message());
        return Ok(prepared);
    }
    let actor = AuditActor::new(log.actor_id.clone(), log.actor_account.clone(), log.actor_type);
    let context = BusinessEventContext::new(actor, action)?;
    let mut prepared = context.log(BusinessEventContent {
        target_id: log
            .resource_id
            .clone()
            .ok_or_else(|| Error::ValidationError("业务审计目标不能为空".into()))?,
        target_number: None,
        result: if log.success { BusinessEventResult::Succeeded } else { BusinessEventResult::Rejected },
        field_changes: Vec::new(),
        facts: Vec::new(),
    })?;
    prepared.base = log.base.clone();
    if let Some(event) = &mut prepared.structured_event {
        event.occurred_at = log.base.created_at;
    }
    Ok(prepared)
}

/// 从已验证业务日志提取事务外尝试所需的安全静态上下文。
///
/// 不读取自由正文。目标缺失时保持缺失。
///
/// # 参数
/// * `log` - 普通事件准备记录。
///
/// # 返回
/// 返回保留安全目标、命令及请求关联的尝试上下文。没有结构化事件时不复制命令、请求和名称快照。
///
/// # 错误
/// 动作未登记、与资源类型不匹配或登记元数据无效时拒绝。
/// 操作人身份、目标、业务编号、命令编号、请求编号或操作人名称超长或含控制字符时拒绝。
pub fn attempt_context(log: &AuditLog) -> Result<BusinessEventContext> {
    let actor = AuditActor::new(log.actor_id.clone(), log.actor_account.clone(), log.actor_type);
    let mut context = BusinessEventContext::new(actor, registered_action(&log.action, &log.resource_type)?)?;
    let event = log.structured_event.as_ref();
    context = context.with_target(
        log.resource_id.clone(),
        event.and_then(|event| event.resource_number_snapshot.clone()),
    )?;
    if let Some(event) = event {
        context = context
            .with_command_id(event.command_id.clone())?
            .with_request_id(event.request_id.clone())?
            .with_actor_name_snapshot(event.actor_name_snapshot.clone())?;
    }
    Ok(context)
}

/// 审计日志服务
///
/// 提供审计日志的写入与查询能力。
pub struct AuditLogService {
    db: Database,
}

impl AuditLogService {
    /// 创建审计日志服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例。
    ///
    /// # 返回
    /// 返回绑定该数据库的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 写入一条审计日志。
    ///
    /// 不把自由正文转换成结构化业务事件。
    ///
    /// # 参数
    /// * `data` - 审计日志创建数据。
    ///
    /// # 返回
    /// 返回已写入、且 `structured_event` 为空的审计日志。
    ///
    /// # 错误
    /// `AuditLog::new` 校验失败时返回 `Error::Logic`。仓储写入失败时按持久化错误映射为回执重复、冲突、暂态事务、结果未知或 `RepositoryError`。
    pub async fn create(&self, data: AuditLogData) -> Result<AuditLog> {
        let id = next_id();
        let log = AuditLog::new(id, data)?;
        self.db.audit_logs().create(&log, &mut NoTransaction).await?;
        Ok(log)
    }

    /// 按查询参数返回审计日志分页。
    ///
    /// # 参数
    /// * `params` - 列表查询参数。
    ///
    /// # 返回
    /// 返回校验通过并完成文本归一化后的 `AuditLogItem` 页；`total` 来自仓储计数。
    ///
    /// # 错误
    /// `params` 校验失败时返回 `Error::ValidationError`。仓储查询失败时返回对应错误。
    pub async fn audit_log_list(&self, params: &AuditLogListParams) -> Result<Page<AuditLogItem>> {
        params.validate()?;
        let filter = AuditLogFilter::from(params);
        let page = self.db.audit_logs().search_logs(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(Into::into).collect();
        Ok(Page::new(items, page.total))
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::AuditActorLogs;

    #[test]
    fn audit_actor_builds_valid_success_resource_log() {
        let log = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("customer.create", "customer", "customer-1".to_string())
            .unwrap();

        assert_eq!(log.actor_id, "admin-1");
        assert_eq!(log.actor_account, "root");
        assert_eq!(log.actor_type, AccountKind::Admin);
        assert_eq!(log.action, "customer.create");
        assert_eq!(log.resource_type, "customer");
        assert_eq!(log.resource_id.as_deref(), Some("customer-1"));
        assert!(log.success);
        assert!(log.message.as_deref().unwrap().contains("创建客户"));
    }

    #[test]
    fn audit_actor_preserves_validated_business_message() {
        let log = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log_with_message(
                "product.update",
                "product",
                "product-1".to_string(),
                Some("恢复销售".to_string()),
            )
            .unwrap();

        assert!(log.message.as_deref().unwrap().contains("修改商品"));
        assert!(!log.message.as_deref().unwrap().contains("恢复销售"));
    }

    #[test]
    fn audit_actor_validates_before_transaction() {
        let result = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("", "customer", "customer-1".to_string());

        assert!(result.is_err());
    }
}

#[cfg(test)]
mod ordinary_factory_tests {
    use erp_core::AccountKind;

    use super::*;
    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "account".into(), AccountKind::Admin)
    }
    #[test]
    fn registered_factory_discards_uncontrolled_message_and_preserves_safe_snapshot() {
        let log = actor()
            .resource_log_with_id(
                "event".into(),
                "customer.update",
                "customer",
                "customer".into(),
                Some("secret request body / bank / ciphertext".into()),
            )
            .unwrap()
            .with_command_id(Some("command".into()))
            .unwrap()
            .with_resource_number(Some("KH-0001".into()))
            .unwrap();
        let prepared = prepare_business_log(&log).unwrap();
        assert_eq!(prepared.base, log.base);
        let event = prepared.structured_event.as_ref().unwrap();
        assert_eq!(event.command_id.as_deref(), Some("command"));
        assert_eq!(event.resource_number_snapshot.as_deref(), Some("KH-0001"));
        assert_eq!(event.actor_name_snapshot, None);
        assert!(prepared.message.as_deref().unwrap().contains("KH-0001"));
        assert!(!serde_json::to_string(&prepared).unwrap().contains("secret request"));
        let attempt = attempt_context(&prepared).unwrap().attempt(crate::AuditAttemptResult::Rejected);
        assert_eq!(attempt.resource_id.as_deref(), Some("customer"));
        assert_eq!(attempt.command_id.as_deref(), Some("command"));
    }
    #[test]
    fn unknown_factory_and_mismatched_structured_identity_fail_closed() {
        assert!(actor().resource_log("customer.update:customer", "customer", "customer".into()).is_err());
        assert!(actor().resource_log("customer.update", "supplier", "customer".into()).is_err());
        let mut log = actor().resource_log("customer.update", "customer", "customer".into()).unwrap();
        log.structured_event.as_mut().unwrap().actor_id = "another actor".into();
        assert!(prepare_business_log(&log).is_err());
    }
    #[test]
    fn snapshot_setters_reject_control_characters_and_missing_structured_event() {
        let log = actor().resource_log("customer.update", "customer", "customer".into()).unwrap();
        assert!(log.clone().with_command_id(Some("invalid\nkey".into())).is_err());
        assert!(log.clone().with_resource_number(Some("invalid\nnumber".into())).is_err());
        let mut missing = log;
        missing.structured_event = None;
        assert!(missing.with_command_id(Some("command".into())).is_err());
    }

    /// 普通工厂在三元组转换前捕获本次名称，并保留事件原身份与时间。
    #[test]
    fn ordinary_factory_preserves_current_actor_name_and_event_metadata() {
        let named = actor()
            .with_actor_name_snapshot(Some("  王慧敏  ".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap();
        let log = named
            .resource_log_with_id("name-event".into(), "customer.update", "customer", "customer".into(), None)
            .unwrap();
        let prepared = prepare_business_log(&log).unwrap();
        let persisted: AuditLog = serde_json::from_str(&serde_json::to_string(&prepared).unwrap()).unwrap();
        assert_eq!(persisted.base, log.base);
        let event = persisted.structured_event.as_ref().unwrap();
        assert_eq!(event.actor_name_snapshot.as_deref(), Some("王慧敏"));
        assert_eq!(event.request_id.as_deref(), Some("request-original"));
        assert_eq!(event.actor_id, "actor");
        assert_eq!(event.actor_account, "account");
        assert!(persisted.message.as_deref().unwrap().contains("王慧敏"));
        let attempt = attempt_context(&persisted).unwrap().attempt(crate::AuditAttemptResult::Unknown);
        assert_eq!(attempt.actor_name_snapshot.as_deref(), Some("王慧敏"));
        assert_eq!(attempt.request_id.as_deref(), Some("request-original"));
        assert_eq!(event.occurred_at, persisted.base.created_at);
    }
}
