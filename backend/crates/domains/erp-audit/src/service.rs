use application_core::AuditActor;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;

use crate::Error;
use crate::catalog::registered_action;
use crate::entity::{
    AuditLog, AuditLogData, BusinessEventContent, BusinessEventContext, BusinessEventResult,
};
use crate::error::Result;
use crate::repository::AuditExt;

/// 由审计领域消费 [`AuditActor`] 构造可持久化审计日志。
pub trait AuditActorLogs {
    /// 为已鉴权操作生成带新稳定 ID 的成功审计，保留本次名称快照和请求号，摘要由动作目录生成，不接受自由正文。
    /// 空资源 ID、未登记或类型不匹配的动作、无效登记配置、身份或目标含控制字符返回 `Error::ValidationError`；操作人、动作或资源类型为空或超长、资源 ID 超长返回 `Error::Logic`。
    fn resource_log(self, action: &str, resource_type: &str, resource_id: String) -> Result<AuditLog>;

    /// 用调用方给定的稳定 ID 构造成功资源审计，保留操作人名称和请求快照；传入说明只校验长度，最终以结构化中文摘要替换。
    /// 空资源 ID、未登记或不适用的动作、无效配置、身份或目标含控制字符返回 `Error::ValidationError`；操作人、动作或资源类型为空或超长，以及资源 ID 或说明超长返回 `Error::Logic`。
    fn resource_log_with_id(
        self,
        id: String,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog>;

    /// 为成功资源操作分配新 ID 并生成结构化审计，保留当次名称与请求号；可传入说明供长度校验，但不会替代目录摘要。
    /// 空资源 ID、未登记或不适用的动作、无效配置、身份或目标含控制字符返回 `Error::ValidationError`；操作人、动作或资源类型为空或超长，以及资源 ID 或说明超长返回 `Error::Logic`。
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

/// 将普通资源日志整理成已登记的结构化事件，保留事件 ID 和时间，自由说明不进入事实。已有事件通过一致性校验后只重算中文摘要。
/// 未登记或不匹配的动作、无效目录、事件 schema 或版本无效、事件与日志身份或成功标记不一致，均返回 `Error::ValidationError`；未结构化记录的目标缺失或身份非法也拒绝。
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
            return Err(Error::ValidationError("审计记录与本次操作不一致，请联系管理员".to_string()));
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

/// 从业务日志取得事务外留痕所需的操作人、动作及已知目标，不读取自由说明；命令号、请求号及名称快照只来自已有结构化事件。动作配对无效、身份或快照超长或含控制字符时拒绝，未知目标保持缺失。
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
/// 提供内部审计日志写入；展示读取由 `AuditLogReadService` 授权。
pub struct AuditLogService {
    db: Database,
}

impl AuditLogService {
    /// 绑定内部审计写入所用的数据库；需要面向用户展示时由 `AuditLogReadService` 单独授权。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分配新 ID 并直接写入登录等内部审计，返回已保存的日志，自由正文不会转换成结构化事件。日志校验失败返回 `Error::Logic`；仓储错误保留回执重复、冲突、暂态事务、结果未知或 `RepositoryError` 分类。
    pub async fn create(&self, data: AuditLogData) -> Result<AuditLog> {
        let id = next_id();
        let log = AuditLog::new(id, data)?;
        self.db.audit_logs().create(&log, &mut NoTransaction).await?;
        Ok(log)
    }
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use erp_core::AccountKind;

    use super::AuditActorLogs;

    #[test]
    fn company_bootstrap_builds_safe_party_event_and_rejects_wrong_target() {
        let actor = AuditActor::new(
            "system:company-bootstrap".into(),
            "system:company-bootstrap".into(),
            AccountKind::Admin,
        );
        let log = actor.clone().resource_log("company.bootstrap", "party", "party-1".into()).unwrap();
        let event = log.structured_event.as_ref().unwrap();
        assert!(log.success);
        assert_eq!(event.action_code, "company.bootstrap");
        assert_eq!(event.resource_type, "party");
        assert_eq!(event.resource_id, "party-1");
        assert!(log.message.as_deref().unwrap().contains("同步内置我方公司"));

        assert!(actor.clone().resource_log("company.bootstrap", "company", "party-1".into()).is_err());
        assert!(actor.resource_log("company.bootstrap", "party", " ".into()).is_err());
    }

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

    // 验证「维持原采购」能为采购单生成安全审计，但同一动作不得记到销售单。
    #[test]
    fn purchase_source_maintain_builds_audit_for_purchase_only() {
        let log = actor()
            .resource_log("purchase_order.maintain_source", "purchase_order", "purchase-1".into())
            .unwrap();
        let prepared = prepare_business_log(&log).unwrap();
        assert_eq!(prepared.action, "purchase_order.maintain_source");
        assert_eq!(prepared.resource_type, "purchase_order");
        assert_eq!(prepared.resource_id.as_deref(), Some("purchase-1"));
        assert!(prepared.success);
        assert!(prepared.message.as_deref().unwrap().contains("登记维持原采购"));

        let error = actor()
            .resource_log("purchase_order.maintain_source", "sales_order", "sales-1".into())
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
    }

    #[test]
    fn purchase_change_void_builds_audit_for_change_only() {
        let log = actor()
            .resource_log("purchase_change_order.void", "purchase_change_order", "change-1".into())
            .unwrap();
        let prepared = prepare_business_log(&log).unwrap();
        assert_eq!(prepared.action, "purchase_change_order.void");
        assert_eq!(prepared.resource_type, "purchase_change_order");
        assert_eq!(prepared.resource_id.as_deref(), Some("change-1"));
        assert!(prepared.success);
        assert!(prepared.message.as_deref().unwrap().contains("作废采购变更单"));

        let error = actor()
            .resource_log("purchase_change_order.void", "purchase_order", "purchase-1".into())
            .unwrap_err();
        assert!(matches!(error, Error::ValidationError(_)));
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
    fn delivery_arrangement_audit_records_sales_target_without_private_address() {
        let log = actor()
            .resource_log("sales_order.delivery_arrangement", "sales_order", "sales-1".into())
            .unwrap();
        let prepared = prepare_business_log(&log).unwrap();
        assert!(prepared.success);
        assert_eq!(prepared.resource_id.as_deref(), Some("sales-1"));
        assert!(prepared.message.as_deref().unwrap().contains("更新收货安排"));
        assert!(prepared.structured_event.as_ref().unwrap().facts.is_empty());
        assert!(
            actor()
                .resource_log("sales_order.delivery_arrangement", "delivery", "delivery-1".into())
                .is_err()
        );
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

    // 验证普通工厂在身份拆解前取得当前操作人名称，结构化后仍保留原事件 ID 与时间。
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
