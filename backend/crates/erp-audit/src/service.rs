pub use application_core::CommandReceipt;
use application_core::{AuditActor, CommandReceiptFact, CommandReceiptMatch, Page};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::NoTransaction;
use validator::Validate;

pub use crate::dto::{AuditLogItem, AuditLogListParams};
use crate::entity::{AuditLog, AuditLogData};
use crate::error::{Error, Result};
use crate::repository::{AuditExt, AuditLogFilter};

/// 由审计领域消费 [`AuditActor`] 构造可持久化审计日志。
pub trait AuditActorLogs {
    /// 在业务写入前构造并验证成功资源审计日志。
    fn resource_log(self, action: &str, resource_type: &str, resource_id: String) -> Result<AuditLog>;

    /// 使用服务端生成的稳定 ID 构造成功资源审计日志。
    fn resource_log_with_id(
        self,
        id: String,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog>;

    /// 在业务写入前构造并验证带业务说明的成功资源审计日志。
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
        build_resource_log(id, self, action, resource_type, resource_id, message)
    }

    fn resource_log_with_message(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog> {
        build_resource_log(next_id(), self, action, resource_type, resource_id, message)
    }
}

/// 构造成功资源审计日志的唯一入口（erp-audit-001）。
///
/// 调用方传入的 `id` 决定身份来源（调用方指定或服务端生成）；
/// 资源 ID 非空校验与实体组装只在此一处实现。
///
/// # 参数
/// * `id` - 审计日志稳定 ID
/// * `actor` - 操作人事实
/// * `action` - 审计动作
/// * `resource_type` - 资源类型稳定代码
/// * `resource_id` - 资源业务 ID，空白视为缺失
/// * `message` - 业务说明
///
/// # 返回
/// 返回已验证的成功资源审计日志。
///
/// # 错误
/// 资源 ID 为空或实体规范化失败时返回错误。
fn build_resource_log(
    id: String,
    actor: AuditActor,
    action: &str,
    resource_type: &str,
    resource_id: String,
    message: Option<String>,
) -> Result<AuditLog> {
    if resource_id.trim().is_empty() {
        return Err(Error::ValidationError("资源ID不能为空".to_string()));
    }
    let (actor_id, actor_account, actor_type) = actor.into_parts();
    AuditLog::new(
        id,
        AuditLogData {
            actor_id,
            actor_account,
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message,
        },
    )
    .map_err(Into::into)
}

/// 命令收据的 Service I/O 适配。
#[allow(async_fn_in_trait)]
pub trait CommandReceiptServiceExt {
    /// 查询并校验已经提交的同一业务命令。
    async fn committed_resource_id(&self, db: &Database) -> Result<Option<String>>;

    /// 构造必须与业务写入同事务持久化的成功收据审计。
    fn audit(&self, actor: AuditActor, resource_id: String) -> Result<AuditLog>;
}

impl CommandReceiptServiceExt for CommandReceipt {
    async fn committed_resource_id(&self, db: &Database) -> Result<Option<String>> {
        let candidates = self.id_candidates();
        let facts = db.audit_logs().find_command_receipts_by_ids(&candidates, &mut NoTransaction).await?;
        pick_receipt(self, &candidates, &facts)
    }

    fn audit(&self, actor: AuditActor, resource_id: String) -> Result<AuditLog> {
        if actor.id() != self.actor_id() {
            return Err(Error::Forbidden("当前账号不能复用其他账号的操作号".to_string()));
        }
        actor.resource_log_with_id(
            self.id().to_string(),
            self.action(),
            self.resource_type(),
            resource_id,
            Some(self.message(None)),
        )
    }
}

/// 返回同一操作号被不同请求占用时的稳定冲突说明。
fn command_conflict() -> Error {
    Error::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string())
}

/// 按候选顺序选择已提交命令收据（erp-audit-004）。
///
/// 不依赖数据库：调用方先批量读取最小事实，本函数只做按序查找与载荷比对，
/// 便于无 DB 单测覆盖候选排序与冲突映射。
///
/// # 参数
/// * `receipt` - 待匹配的命令收据
/// * `candidates` - 按优先级排序的候选 ID
/// * `facts` - 已读取的最小收据事实
///
/// # 返回
/// 载荷一致时返回已提交资源 ID；无命中返回 `None`。
///
/// # 错误
/// 同一操作号被不同载荷占用时返回冲突；事实损坏时返回内部错误。
fn pick_receipt(
    receipt: &CommandReceipt,
    candidates: &[String],
    facts: &[CommandReceiptFact],
) -> Result<Option<String>> {
    for candidate in candidates {
        let Some(fact) = facts.iter().find(|fact| &fact.id == candidate) else {
            continue;
        };
        return match receipt.match_fact(fact) {
            CommandReceiptMatch::SamePayload(resource_id) => Ok(Some(resource_id)),
            CommandReceiptMatch::DifferentPayload => Err(command_conflict()),
            CommandReceiptMatch::Corrupted => Err(Error::Internal("业务命令收据格式无效".to_string())),
        };
    }
    Ok(None)
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
    /// * `db` - 数据库实例
    ///
    /// # 返回值
    /// 返回审计日志服务实例
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 写入审计日志。
    ///
    /// # 参数
    /// * `data` - 审计日志数据
    ///
    /// # 返回值
    /// 返回写入后的审计日志实体
    pub async fn create(&self, data: AuditLogData) -> Result<AuditLog> {
        let id = next_id();
        let log = AuditLog::new(id, data)?;
        self.db.audit_logs().create(&log, &mut NoTransaction).await?;
        Ok(log)
    }

    /// 获取审计日志列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回值
    /// 返回分页后的审计日志集合
    pub async fn audit_log_list(&self, params: &AuditLogListParams) -> Result<Page<AuditLogItem>> {
        params.validate()?;
        let filter = AuditLogFilter::from(params.normalized());
        let page = self.db.audit_logs().search_logs(&filter, &mut NoTransaction).await?;
        let items = page.items.into_iter().map(Into::into).collect();
        Ok(Page::new(items, page.total))
    }
}

#[cfg(test)]
mod tests {
    use application_core::{AuditActor, CommandReceiptFact, CommandReceiptMatch};
    use erp_core::AccountKind;
    use serde::Serialize;

    use super::{AuditActorLogs, CommandReceipt, CommandReceiptServiceExt as _, pick_receipt};

    #[derive(Serialize)]
    struct CommandPayload {
        amount: u32,
        idempotency_key: String,
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
        assert!(log.message.is_none());
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

        assert_eq!(log.message.as_deref(), Some("恢复销售"));
    }

    #[test]
    fn audit_actor_validates_before_transaction() {
        let result = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("", "customer", "customer-1".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn audit_actor_rejects_empty_resource_id() {
        let result = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
            .resource_log("customer.create", "customer", "  ".to_string());

        assert!(result.is_err());
    }

    #[test]
    fn command_receipt_hides_raw_key_and_replays_matching_resource() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let payload = CommandPayload { amount: 100, idempotency_key: "raw-operation-key".to_string() };
        let receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &payload.idempotency_key,
            &payload,
        )
        .unwrap();
        let audit = receipt.audit(actor, "receipt-1".to_string()).unwrap();

        assert!(!audit.base.id.contains("raw-operation-key"));
        assert!(!audit.message.as_deref().unwrap().contains("raw-operation-key"));
        let fact = CommandReceiptFact::new(audit.base.id, audit.actor_id, audit.action, audit.resource_type)
            .with_resource_id_opt(audit.resource_id)
            .with_success(audit.success)
            .with_message_opt(audit.message);
        assert_eq!(receipt.match_fact(&fact), CommandReceiptMatch::SamePayload("receipt-1".to_string()));
    }

    #[test]
    fn command_receipt_rejects_same_key_with_different_payload() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let first = CommandPayload { amount: 100, idempotency_key: "operation-key".to_string() };
        let changed = CommandPayload { amount: 200, idempotency_key: "operation-key".to_string() };
        let first_receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &first.idempotency_key,
            &first,
        )
        .unwrap();
        let audit = first_receipt.audit(actor.clone(), "receipt-1".to_string()).unwrap();
        let changed_receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &changed.idempotency_key,
            &changed,
        )
        .unwrap();

        let fact = CommandReceiptFact::new(audit.base.id, audit.actor_id, audit.action, audit.resource_type)
            .with_resource_id_opt(audit.resource_id)
            .with_success(audit.success)
            .with_message_opt(audit.message);
        assert_eq!(changed_receipt.match_fact(&fact), CommandReceiptMatch::DifferentPayload);
    }

    #[test]
    fn pick_receipt_returns_none_without_db_when_no_candidate_matches() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let payload = CommandPayload { amount: 100, idempotency_key: "pick-none-key".to_string() };
        let receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &payload.idempotency_key,
            &payload,
        )
        .unwrap();

        let candidates = vec!["missing-1".to_string()];
        assert_eq!(pick_receipt(&receipt, &candidates, &[]).unwrap(), None);
    }

    #[test]
    fn pick_receipt_replays_matching_resource_without_db() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let payload = CommandPayload { amount: 100, idempotency_key: "pick-hit-key".to_string() };
        let receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &payload.idempotency_key,
            &payload,
        )
        .unwrap();
        let audit = receipt.audit(actor, "receipt-1".to_string()).unwrap();
        let candidates = vec![audit.base.id.clone()];
        let facts = vec![
            CommandReceiptFact::new(
                audit.base.id.clone(),
                audit.actor_id.clone(),
                audit.action.clone(),
                audit.resource_type.clone(),
            )
            .with_resource_id_opt(audit.resource_id.clone())
            .with_success(audit.success)
            .with_message_opt(audit.message.clone()),
        ];

        assert_eq!(pick_receipt(&receipt, &candidates, &facts).unwrap(), Some("receipt-1".to_string()));
    }

    #[test]
    fn pick_receipt_conflicts_without_db_on_same_id_different_payload() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let first = CommandPayload { amount: 100, idempotency_key: "pick-conflict-key".to_string() };
        let changed = CommandPayload { amount: 200, idempotency_key: "pick-conflict-key".to_string() };
        let first_receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &first.idempotency_key,
            &first,
        )
        .unwrap();
        let audit = first_receipt.audit(actor.clone(), "receipt-1".to_string()).unwrap();
        let changed_receipt = CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &changed.idempotency_key,
            &changed,
        )
        .unwrap();
        let candidates = vec![audit.base.id.clone()];
        let facts = vec![
            CommandReceiptFact::new(
                audit.base.id.clone(),
                audit.actor_id.clone(),
                audit.action.clone(),
                audit.resource_type.clone(),
            )
            .with_resource_id_opt(audit.resource_id.clone())
            .with_success(audit.success)
            .with_message_opt(audit.message.clone()),
        ];

        assert!(pick_receipt(&changed_receipt, &candidates, &facts).is_err());
    }
}
