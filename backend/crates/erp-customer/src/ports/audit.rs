//! Consumer port for cross-domain audit persistence from customer commands.

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// `from_validated` 的参数对象（erp-customer-012）。
///
/// 九个调用参数收敛为整体输入，避免调用方传参顺序易错且难以扩展。
/// `base` 保持引用：调用方（组合层 adapter）在转换后仍需使用原实体。
pub struct ValidatedAuditSnapshot<'a> {
    /// 已构造审计的持久化元数据。
    pub base: &'a BaseModel,
    /// 操作人 ID。
    pub actor_id: String,
    /// 操作人登录账号。
    pub actor_account: String,
    /// 操作人类型。
    pub actor_type: AccountKind,
    /// 业务动作名。
    pub action: String,
    /// 资源类型。
    pub resource_type: String,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 成功标记。
    pub success: bool,
    /// 业务说明。
    pub message: Option<String>,
}

/// Prepared successful resource audit that customer can persist through a port.
///
/// Customer never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCustomerAudit {
    /// Stable audit document id.
    pub id: String,
    /// Optimistic-lock version captured at construction.
    pub version: u64,
    /// Creation timestamp captured at construction.
    pub created_at: u64,
    /// Update timestamp captured at construction.
    pub updated_at: u64,
    /// Soft-delete marker captured at construction.
    pub deleted_at: u64,
    /// Actor account id.
    pub actor_id: String,
    /// Actor login account.
    pub actor_account: String,
    /// Actor kind.
    pub actor_type: AccountKind,
    /// Business action name.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: Option<String>,
    /// Success flag; customer only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl PreparedCustomerAudit {
    /// Capture customer-side fields from an already-validated audit entity snapshot.
    ///
    /// 参数对象 [`ValidatedAuditSnapshot`] 收敛九个调用参数（erp-customer-012），
    /// 避免调用方传参顺序易错且难以扩展；本函数为新代码的参数对象入口。
    ///
    /// # 参数
    /// * `snapshot` - 已校验的审计实体快照
    ///
    /// # 返回
    /// 返回稍后持久化的不透明审计事实。
    pub fn from_snapshot(snapshot: ValidatedAuditSnapshot) -> Self {
        Self {
            id: snapshot.base.id.clone(),
            version: snapshot.base.version,
            created_at: snapshot.base.created_at,
            updated_at: snapshot.base.updated_at,
            deleted_at: snapshot.base.deleted_at,
            actor_id: snapshot.actor_id,
            actor_account: snapshot.actor_account,
            actor_type: snapshot.actor_type,
            action: snapshot.action,
            resource_type: snapshot.resource_type,
            resource_id: snapshot.resource_id,
            success: snapshot.success,
            message: snapshot.message,
        }
    }

    /// Capture customer-side fields from an already-validated audit entity snapshot.
    ///
    /// 组合层存量调用方仍使用本九参入口（与 erp-processes adapters 保持兼容）；
    /// 新代码优先使用参数对象入口 [`PreparedCustomerAudit::from_snapshot`]。
    ///
    /// # 参数
    /// * `base` - 已构造审计的持久化元数据
    /// * `actor_id` - 操作人 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人类型
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    /// * `success` - 成功标记
    /// * `message` - 业务说明
    ///
    /// # 返回
    /// 返回稍后持久化的不透明审计事实。
    #[allow(clippy::too_many_arguments)]
    pub fn from_validated(
        base: &BaseModel,
        actor_id: String,
        actor_account: String,
        actor_type: AccountKind,
        action: String,
        resource_type: String,
        resource_id: Option<String>,
        success: bool,
        message: Option<String>,
    ) -> Self {
        Self::from_snapshot(ValidatedAuditSnapshot {
            base,
            actor_id,
            actor_account,
            actor_type,
            action,
            resource_type,
            resource_id,
            success,
            message,
        })
    }

    /// Build a success resource audit from an authenticated actor.
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源业务 ID，空白视为缺失
    ///
    /// # 返回
    /// 返回已校验的成功资源审计预制事实。
    ///
    /// # 错误
    /// 资源 ID 为空时返回校验错误。
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_snapshot(ValidatedAuditSnapshot {
            base: &base,
            actor_id,
            actor_account,
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message: None,
        }))
    }
}

/// Port customer uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait CustomerAuditPort: Send + Sync {
    /// Validate and prepare a success resource audit before the transaction.
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedCustomerAudit>;

    /// Persist a previously prepared audit on the caller-chosen executor.
    async fn persist(&self, audit: &PreparedCustomerAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl CustomerAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedCustomerAudit> {
        PreparedCustomerAudit::resource(actor, action, resource_type, resource_id)
    }

    async fn persist(&self, _audit: &PreparedCustomerAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
