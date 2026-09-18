use application_core::{AuditActor, CommandReceipt, CommandReceiptFact, CommandReceiptMatch};
use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{AccountKind, Result};
use serde::{Deserialize, Serialize};

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
}

impl AuditLog {
    /// 由已鉴权操作人构造成功资源审计的数据。
    ///
    /// 资源 ID 空白视为缺失，调用方仍需传入业务资源 ID。
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `action` - 审计动作
    /// * `resource_type` - 资源类型稳定代码
    /// * `resource_id` - 资源业务 ID，空白视为缺失
    /// * `message` - 业务说明
    ///
    /// # 返回
    /// 返回已组装的成功资源审计创建数据。
    ///
    /// # 错误
    /// 资源 ID 为空时返回校验错误。
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

    /// 由命令收据构造必须与业务写入同事务持久化的成功收据审计数据。
    ///
    /// # 参数
    /// * `receipt` - 已提交的业务命令收据
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `resource_id` - 资源业务 ID，空白视为缺失
    ///
    /// # 返回
    /// 返回已组装的成功收据审计创建数据。
    ///
    /// # 错误
    /// 当前账号与收据操作人不一致，或资源 ID 为空时返回错误。
    pub fn receipt_audit_data(
        receipt: &CommandReceipt,
        actor: AuditActor,
        resource_id: String,
    ) -> AuditResult<AuditLogData> {
        if actor.id() != receipt.actor_id() {
            return Err(AuditError::Forbidden("当前账号不能复用其他账号的操作号".to_string()));
        }
        Self::success_resource_data(
            actor,
            receipt.action(),
            receipt.resource_type(),
            resource_id,
            Some(receipt.message(None)),
        )
    }

    /// 按候选顺序选择已提交命令收据。
    ///
    /// 不依赖数据库：调用方先批量读取最小事实，本函数只做按序查找与载荷比对。
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
    pub fn pick_committed_resource_id(
        receipt: &CommandReceipt,
        candidates: &[String],
        facts: &[CommandReceiptFact],
    ) -> AuditResult<Option<String>> {
        for candidate in candidates {
            let Some(fact) = facts.iter().find(|fact| &fact.id == candidate) else {
                continue;
            };
            return match receipt.match_fact(fact) {
                CommandReceiptMatch::SamePayload(resource_id) => Ok(Some(resource_id)),
                CommandReceiptMatch::DifferentPayload => {
                    Err(AuditError::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string()))
                },
                CommandReceiptMatch::Corrupted => {
                    Err(AuditError::Internal("业务命令收据格式无效".to_string()))
                },
            };
        }
        Ok(None)
    }

    /// 从审计日志构造命令收据比对所需的最小事实。
    ///
    /// # 参数
    /// 无额外参数，消费审计日志实体。
    ///
    /// # 返回
    /// 返回收据回放比对使用的最小事实。
    pub fn receipt_fact(self) -> CommandReceiptFact {
        CommandReceiptFact::new(self.base.id, self.actor_id, self.action, self.resource_type)
            .with_resource_id_opt(self.resource_id)
            .with_success(self.success)
            .with_message_opt(self.message)
    }

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
    use application_core::AuditActor;
    use erp_core::AccountKind;
    use serde::Serialize;

    use super::{AuditLog, AuditLogData};

    #[derive(Serialize)]
    struct CommandPayload {
        amount: u32,
        idempotency_key: String,
    }

    fn audit_actor() -> AuditActor {
        AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin)
    }

    fn receipt_for(actor: &AuditActor, key: &str, amount: u32) -> application_core::CommandReceipt {
        let payload = CommandPayload { amount, idempotency_key: key.to_string() };
        application_core::CommandReceipt::from_payload(
            "receipt-command-",
            actor.id(),
            "customer_receipt.commit",
            "customer_receipt",
            &payload.idempotency_key,
            &payload,
        )
        .expect("收据构造必须成功")
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
    fn receipt_audit_data_rejects_foreign_actor() {
        let owner = audit_actor();
        let receipt = receipt_for(&owner, "foreign-actor-key", 100);
        let other = AuditActor::new("admin-2".to_string(), "other".to_string(), AccountKind::Admin);

        assert!(AuditLog::receipt_audit_data(&receipt, other, "receipt-1".to_string()).is_err());
    }

    #[test]
    fn pick_committed_resource_id_returns_none_without_match() {
        let actor = audit_actor();
        let receipt = receipt_for(&actor, "pick-none-key", 100);

        assert_eq!(
            AuditLog::pick_committed_resource_id(&receipt, &["missing-1".to_string()], &[]).unwrap(),
            None
        );
    }

    #[test]
    fn pick_committed_resource_id_replays_matching_resource() {
        let actor = audit_actor();
        let receipt = receipt_for(&actor, "pick-hit-key", 100);
        let audit = AuditLog::new(
            receipt.id().to_string(),
            AuditLog::receipt_audit_data(&receipt, actor, "receipt-1".to_string()).unwrap(),
        )
        .unwrap();
        let candidates = vec![audit.base.id.clone()];
        let facts = vec![audit.receipt_fact()];

        assert_eq!(
            AuditLog::pick_committed_resource_id(&receipt, &candidates, &facts).unwrap(),
            Some("receipt-1".to_string())
        );
    }

    #[test]
    fn pick_committed_resource_id_conflicts_on_same_id_different_payload() {
        let actor = audit_actor();
        let receipt = receipt_for(&actor, "pick-conflict-key", 100);
        let audit = AuditLog::new(
            receipt.id().to_string(),
            AuditLog::receipt_audit_data(&receipt, actor.clone(), "receipt-1".to_string()).unwrap(),
        )
        .unwrap();
        let changed = receipt_for(&actor, "pick-conflict-key", 200);
        let candidates = vec![audit.base.id.clone()];
        let facts = vec![audit.receipt_fact()];

        assert!(AuditLog::pick_committed_resource_id(&changed, &candidates, &facts).is_err());
    }

    #[test]
    fn pick_committed_resource_id_rejects_corrupted_fact() {
        let actor = audit_actor();
        let receipt = receipt_for(&actor, "pick-corrupted-key", 100);
        let audit = AuditLog::new(
            receipt.id().to_string(),
            AuditLog::receipt_audit_data(&receipt, actor, "receipt-1".to_string()).unwrap(),
        )
        .unwrap();
        let candidates = vec![audit.base.id.clone()];
        let mut fact = audit.receipt_fact();
        fact.success = false;

        assert!(AuditLog::pick_committed_resource_id(&receipt, &candidates, &[fact]).is_err());
    }
}
