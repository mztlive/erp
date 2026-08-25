use database::{repository::AuditLogFilter, AccessControlExt, NoTransaction};
use entities::{AuditLog, AuditLogData};
use id_generator::next_id;
use mongodb::Database;
use serde::Serialize;
use sha2::{Digest, Sha256};
use validator::Validate;

use crate::{
    errors::{Error, Result},
    Page,
};

use self::dto::NormalizedAuditLogListParams;
pub use self::dto::{AuditLogItem, AuditLogListParams};

mod dto;

const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

/// 已通过 HTTP 鉴权的审计操作人。
///
/// 该类型只携带操作人身份；审计动作、资源类型和目标由具体 Service 决定，
/// 避免协议层伪造或遗漏业务审计语义。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditActor {
    actor_id: String,
    actor_account: String,
    actor_type: entities::AccountKind,
}

impl AuditActor {
    /// 创建审计操作人。
    ///
    /// # 参数
    /// * `actor_id` - 操作人账号 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人账号类型
    ///
    /// # 返回值
    /// 返回只包含鉴权身份的审计操作人。
    pub fn new(actor_id: String, actor_account: String, actor_type: entities::AccountKind) -> Self {
        Self {
            actor_id,
            actor_account,
            actor_type,
        }
    }

    /// 返回操作人账号 ID。
    ///
    /// # 返回值
    /// 返回已认证身份中的账号 ID。
    pub fn id(&self) -> &str {
        &self.actor_id
    }

    /// 返回操作人账号类型。
    ///
    /// # 返回值
    /// 返回已认证身份中的后台账号类型。
    pub fn kind(&self) -> entities::AccountKind {
        self.actor_type
    }

    /// 在业务写入前构造并验证成功资源审计日志。
    ///
    /// # 参数
    /// * `action` - Service 确定的动作名
    /// * `resource_type` - Service 确定的资源类型
    /// * `resource_id` - 本次操作目标
    ///
    /// # 返回值
    /// 返回已通过领域校验、可直接持久化的审计日志。
    ///
    /// # 错误
    /// 当操作人或资源审计字段不符合领域约束时返回错误。
    pub(crate) fn resource_log(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<AuditLog> {
        self.resource_log_with_message(action, resource_type, resource_id, None)
    }

    /// 使用服务端生成的稳定 ID 构造成功资源审计日志。
    ///
    /// 该入口供需要数据库唯一键仲裁幂等请求的强类型服务使用。调用方不得把
    /// 原始幂等键写入 `audit_logs`；稳定 ID 必须由不可逆摘要形成。
    ///
    /// # 参数
    /// * `id` - 服务端生成的不可逆稳定审计 ID
    /// * `action` - Service 确定的动作名
    /// * `resource_type` - Service 确定的资源类型
    /// * `resource_id` - 本次操作目标
    /// * `message` - 权限安全的业务说明
    ///
    /// # 返回值
    /// 返回已通过领域校验、可直接持久化的审计日志。
    ///
    /// # 错误
    /// 当操作人、资源或审计字段不符合领域约束时返回错误。
    pub(crate) fn resource_log_with_id(
        self,
        id: String,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        AuditLog::new(
            id,
            AuditLogData {
                actor_id: self.actor_id,
                actor_account: self.actor_account,
                actor_type: self.actor_type,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: Some(resource_id),
                success: true,
                message,
            },
        )
        .map_err(Into::into)
    }

    /// 在业务写入前构造并验证带业务说明的成功资源审计日志。
    ///
    /// # 参数
    /// * `action` - Service 确定的动作名
    /// * `resource_type` - Service 确定的资源类型
    /// * `resource_id` - 本次操作目标
    /// * `message` - 业务变更原因或执行说明
    ///
    /// # 返回值
    /// 返回已通过领域校验、可直接持久化的审计日志。
    ///
    /// # 错误
    /// 当操作人、资源或消息字段不符合领域约束时返回错误。
    pub(crate) fn resource_log_with_message(
        self,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<AuditLog> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        AuditLog::new(
            next_id(),
            AuditLogData {
                actor_id: self.actor_id,
                actor_account: self.actor_account,
                actor_type: self.actor_type,
                action: action.to_string(),
                resource_type: resource_type.to_string(),
                resource_id: Some(resource_id),
                success: true,
                message,
            },
        )
        .map_err(Into::into)
    }
}

/// 使用成功审计行承载的根级业务命令收据。
///
/// 收据 ID 只保存操作人、动作和原始操作号的不可逆摘要；消息只保存规范请求
/// 指纹。业务结果通过审计资源 ID 回放，收据与业务写入必须位于同一事务。
#[derive(Debug, Clone)]
pub(crate) struct CommandReceipt {
    id: String,
    actor_id: String,
    action: String,
    resource_type: String,
    fingerprint: String,
}

impl CommandReceipt {
    /// 为根级命令构造稳定收据身份和请求指纹。
    ///
    /// # 参数
    /// * `prefix` - 固定命令类别前缀
    /// * `actor` - 已认证操作人
    /// * `action` - 固定审计动作
    /// * `resource_type` - 成功结果资源类型
    /// * `idempotency_key` - 客户端操作号，仅参与不可逆摘要
    /// * `payload` - 完整请求载荷，用于拒绝同号异载荷
    ///
    /// # 返回值
    /// 返回不包含原始操作号的稳定命令收据。
    ///
    /// # 错误
    /// 操作号为空或请求载荷无法序列化时返回错误。
    pub(crate) fn new<T: Serialize>(
        prefix: &str,
        actor: &AuditActor,
        action: &str,
        resource_type: &str,
        idempotency_key: &str,
        payload: &T,
    ) -> Result<Self> {
        let key = idempotency_key.trim();
        if key.is_empty() {
            return Err(Error::ValidationError("操作号不能为空".to_string()));
        }
        let payload = serde_json::to_string(payload)
            .map_err(|error| Error::Internal(format!("业务命令请求序列化失败: {error}")))?;
        Ok(Self {
            id: format!(
                "{prefix}{}",
                digest_parts(&[actor.id(), action, resource_type, key])
            ),
            actor_id: actor.id().to_string(),
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            fingerprint: digest_parts(&[action, resource_type, &payload]),
        })
    }

    /// 查询并校验已经提交的同一业务命令。
    ///
    /// # 参数
    /// * `db` - 当前业务数据库
    ///
    /// # 返回值
    /// 收据不存在时返回 `None`；存在且请求一致时返回首次业务资源 ID。
    ///
    /// # 错误
    /// 查询失败、同一操作号载荷不一致或收据损坏时返回错误。
    pub(crate) async fn committed_resource_id(&self, db: &Database) -> Result<Option<String>> {
        let Some(audit) = db.audit_logs().find_by_id(&self.id, &mut NoTransaction).await? else {
            return Ok(None);
        };
        self.resource_id_from_audit(&audit).map(Some)
    }

    /// 构造必须与业务写入同事务持久化的成功收据审计。
    ///
    /// # 参数
    /// * `actor` - 当前已认证操作人
    /// * `resource_id` - 首次成功业务资源 ID
    ///
    /// # 返回值
    /// 返回使用稳定 ID 和请求指纹的成功审计实体。
    ///
    /// # 错误
    /// 操作人不一致或审计字段无效时返回错误。
    pub(crate) fn audit(&self, actor: AuditActor, resource_id: String) -> Result<AuditLog> {
        if actor.id() != self.actor_id {
            return Err(Error::Forbidden("当前账号不能复用其他账号的操作号".to_string()));
        }
        actor.resource_log_with_id(
            self.id.clone(),
            &self.action,
            &self.resource_type,
            resource_id,
            Some(format!("{COMMAND_FINGERPRINT_PREFIX}{}", self.fingerprint)),
        )
    }

    /// 校验收据身份、请求指纹并提取首次业务资源 ID。
    fn resource_id_from_audit(&self, audit: &AuditLog) -> Result<String> {
        let identity_matches = audit.success
            && audit.actor_id == self.actor_id
            && audit.action == self.action
            && audit.resource_type == self.resource_type;
        if !identity_matches {
            return Err(command_conflict());
        }
        let fingerprint = audit
            .message
            .as_deref()
            .and_then(|message| message.strip_prefix(COMMAND_FINGERPRINT_PREFIX))
            .ok_or_else(|| Error::Internal("业务命令收据格式无效".to_string()))?;
        if fingerprint != self.fingerprint {
            return Err(command_conflict());
        }
        audit
            .resource_id
            .clone()
            .ok_or_else(|| Error::Internal("业务命令收据缺少结果身份".to_string()))
    }
}

/// 返回同一操作号被不同请求占用时的稳定冲突说明。
fn command_conflict() -> Error {
    Error::ConflictError("同一操作号已用于不同提交，请重新发起操作".to_string())
}

/// 对带长度边界的文本片段计算稳定 SHA-256 摘要。
fn digest_parts(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update((part.len() as u64).to_be_bytes());
        hasher.update(part.as_bytes());
    }
    hex::encode(hasher.finalize())
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
        let NormalizedAuditLogListParams {
            actor_account,
            action,
            resource_type,
            success,
            page,
            page_size,
        } = params.normalized();
        let filter = AuditLogFilter {
            actor_account,
            action,
            resource_type,
            success,
            page,
            page_size,
        };
        let page = self
            .db
            .audit_logs()
            .search_logs(&filter, &mut NoTransaction)
            .await?;
        let items = page.items.into_iter().map(Into::into).collect();
        Ok(Page::new(items, page.total))
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use entities::AccountKind;

    use super::{AuditActor, CommandReceipt};

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
        let payload = CommandPayload {
            amount: 100,
            idempotency_key: "raw-operation-key".to_string(),
        };
        let receipt = CommandReceipt::new(
            "receipt-command-",
            &actor,
            "customer_receipt.commit",
            "customer_receipt",
            &payload.idempotency_key,
            &payload,
        )
        .unwrap();
        let audit = receipt.audit(actor, "receipt-1".to_string()).unwrap();

        assert!(!audit.base.id.contains("raw-operation-key"));
        assert!(!audit.message.as_deref().unwrap().contains("raw-operation-key"));
        assert_eq!(receipt.resource_id_from_audit(&audit).unwrap(), "receipt-1");
    }

    #[test]
    fn command_receipt_rejects_same_key_with_different_payload() {
        let actor = AuditActor::new("admin-1".to_string(), "root".to_string(), AccountKind::Admin);
        let first = CommandPayload {
            amount: 100,
            idempotency_key: "operation-key".to_string(),
        };
        let changed = CommandPayload {
            amount: 200,
            idempotency_key: "operation-key".to_string(),
        };
        let first_receipt = CommandReceipt::new(
            "receipt-command-",
            &actor,
            "customer_receipt.commit",
            "customer_receipt",
            &first.idempotency_key,
            &first,
        )
        .unwrap();
        let audit = first_receipt
            .audit(actor.clone(), "receipt-1".to_string())
            .unwrap();
        let changed_receipt = CommandReceipt::new(
            "receipt-command-",
            &actor,
            "customer_receipt.commit",
            "customer_receipt",
            &changed.idempotency_key,
            &changed,
        )
        .unwrap();

        assert!(changed_receipt.resource_id_from_audit(&audit).is_err());
    }
}
