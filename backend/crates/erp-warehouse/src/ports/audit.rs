//! 仓库命令跨域持久化审计的消费端口。

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that warehouse can persist through a port.
///
/// Warehouse never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedWarehouseAudit {
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
    /// Safe actor display-name snapshot captured when the command was authenticated.
    pub actor_name_snapshot: Option<String>,
    /// Request correlation captured at invocation; absent outside a request.
    pub request_id: Option<String>,
    /// Positive event order within one business command.
    pub event_sequence: NonZeroU32,
    /// Business action name.
    pub action: String,
    /// Resource type.
    pub resource_type: String,
    /// Resource id.
    pub resource_id: Option<String>,
    /// Success flag; warehouse only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl PreparedWarehouseAudit {
    /// 从已校验的审计实体快照抄录仓库侧字段。
    ///
    /// 操作人名称、请求编号不在参数中，结果里保持 `None`；事件序号为 `1`。
    ///
    /// # 参数
    /// * `base` - 已构造审计的持久化元数据。
    /// * `actor_id` - 操作人 ID。
    /// * `actor_account` - 操作人登录账号。
    /// * `actor_type` - 操作人类型。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `success` - 成功标记。
    /// * `message` - 可选业务消息。
    ///
    /// # 返回
    /// 返回稍后持久化的审计事实。
    ///
    /// # 错误
    /// 不返回错误。
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
        Self::from_facts(&WarehouseAuditFacts {
            base: base.clone(),
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

    /// 从审计事实结构体抄录仓库侧字段。
    ///
    /// 操作人名称与请求编号记为 `None`，事件序号记为 `1`。
    ///
    /// # 参数
    /// * `facts` - 审计事实参数结构体。
    ///
    /// # 返回
    /// 返回稍后持久化的审计事实。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn from_facts(facts: &WarehouseAuditFacts) -> Self {
        Self {
            id: facts.base.id.clone(),
            version: facts.base.version,
            created_at: facts.base.created_at,
            updated_at: facts.base.updated_at,
            deleted_at: facts.base.deleted_at,
            actor_id: facts.actor_id.clone(),
            actor_account: facts.actor_account.clone(),
            actor_type: facts.actor_type,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            action: facts.action.clone(),
            resource_type: facts.resource_type.clone(),
            resource_id: facts.resource_id.clone(),
            success: facts.success,
            message: facts.message.clone(),
        }
    }

    /// 携带已校验的安全操作人名称，不读取当前账号数据。
    ///
    /// 组合层在构造结构化事件时会再次校验该快照。
    ///
    /// # 参数
    /// * `name` - 认证时已经校验的安全名称快照；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带名称快照的审计事实。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Self {
        self.actor_name_snapshot = name;
        self
    }

    /// 携带已校验的请求关联编号，不在此处生成新编号。
    ///
    /// 组合层在构造结构化事件时会再次校验该编号。
    ///
    /// # 参数
    /// * `request_id` - 调用时的安全请求编号；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带请求关联的审计事实。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    /// 保留已准备事件的正序序号。
    ///
    /// # 参数
    /// * `event_sequence` - 命令内从一开始的非零事件序号。
    ///
    /// # 返回
    /// 返回保留原事件顺序的审计事实。
    ///
    /// # 错误
    /// 不返回错误。参数类型保证序号非零。
    pub fn with_event_sequence(mut self, event_sequence: NonZeroU32) -> Self {
        self.event_sequence = event_sequence;
        self
    }

    /// 由已认证操作人构造一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    ///
    /// # 返回
    /// 返回成功标记为 `true` 的审计事实，并带上操作人名称快照与请求编号。
    ///
    /// # 错误
    /// `resource_id` 去首尾空白后为空时返回 `ValidationError`。
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        Self::resource_with_message(actor, action, resource_type, resource_id, None)
    }

    /// 由已认证操作人构造一条带可选业务消息的成功资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `message` - 可选业务消息；`None` 表示不附带消息。
    ///
    /// # 返回
    /// 返回成功标记为 `true` 的审计事实，并带上操作人名称快照与请求编号。
    ///
    /// # 错误
    /// `resource_id` 去首尾空白后为空时返回 `ValidationError`。
    pub fn resource_with_message(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_facts(&WarehouseAuditFacts {
            base,
            actor_id,
            actor_account,
            actor_type,
            action: action.to_string(),
            resource_type: resource_type.to_string(),
            resource_id: Some(resource_id),
            success: true,
            message,
        })
        .with_actor_name_snapshot(actor_name_snapshot)
        .with_request_id(request_id))
    }
}

/// 审计事实参数结构体（承接 `from_validated` 的字段组，调用点按字段名赋值避免顺序传错）。
#[derive(Debug, Clone)]
pub struct WarehouseAuditFacts {
    /// 已构造审计的持久化元数据。
    pub base: BaseModel,
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
    /// 成功标记；仓库只准备成功的资源审计。
    pub success: bool,
    /// 可选业务消息。
    pub message: Option<String>,
}

/// Port warehouse uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait WarehouseAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    ///
    /// # 返回
    /// 返回可在调用方执行器上持久化的审计事实。
    ///
    /// # 错误
    /// 校验失败或无法准备审计时返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedWarehouseAudit>;

    /// 在事务开始前校验并准备一条带业务消息的成功资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `message` - 可选业务消息。
    ///
    /// # 返回
    /// 返回可在调用方执行器上持久化的审计事实。
    ///
    /// # 错误
    /// 校验失败或无法准备审计时返回错误。
    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedWarehouseAudit>;

    /// 在调用方选定的执行器上持久化已准备的审计。
    ///
    /// # 参数
    /// * `audit` - 已准备的审计事实。
    /// * `executor` - 调用方执行器。
    ///
    /// # 返回
    /// 无返回值。持久化完成。
    ///
    /// # 错误
    /// 持久化失败时返回错误。
    async fn persist(&self, audit: &PreparedWarehouseAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl WarehouseAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedWarehouseAudit> {
        PreparedWarehouseAudit::resource(actor, action, resource_type, resource_id)
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedWarehouseAudit> {
        PreparedWarehouseAudit::resource_with_message(actor, action, resource_type, resource_id, message)
    }

    async fn persist(&self, _audit: &PreparedWarehouseAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
