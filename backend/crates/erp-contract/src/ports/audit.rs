//! 合同命令跨域持久化审计的消费方端口。

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that contract can persist through a port.
///
/// Contract never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedContractAudit {
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
    /// Success flag; contract only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl PreparedContractAudit {
    /// 从已校验的审计实体快照摘取合同侧字段。
    ///
    /// 名称快照与请求编号留空，事件序号为 1；二者由后续方法填入。
    ///
    /// # 参数
    /// * `base` - 已构造审计的持久化元数据。
    /// * `actor_id` - 操作人 ID。
    /// * `actor_account` - 登录账号。
    /// * `actor_type` - 操作人种类。
    /// * `action` - 动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `success` - 是否成功。
    /// * `message` - 可选说明。
    ///
    /// # 返回
    /// 供稍后持久化的审计事实。
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
        Self {
            id: base.id.clone(),
            version: base.version,
            created_at: base.created_at,
            updated_at: base.updated_at,
            deleted_at: base.deleted_at,
            actor_id,
            actor_account,
            actor_type,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            action,
            resource_type,
            resource_id,
            success,
            message,
        }
    }

    /// 携带已校验的安全操作人名称，不读取当前账号资料。
    ///
    /// # 参数
    /// * `name` - 认证时已经校验的安全名称快照；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带名称快照的审计事实。
    ///
    /// # 错误
    /// 无；组合层在构造结构化事件时再次校验该快照。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Self {
        self.actor_name_snapshot = name;
        self
    }

    /// 携带已校验的请求关联编号，不另行生成。
    ///
    /// # 参数
    /// * `request_id` - 调用时的安全请求编号；缺失时保持 `None`。
    ///
    /// # 返回
    /// 返回保留原持久化元数据并携带请求关联的审计事实。
    ///
    /// # 错误
    /// 无；组合层在构造结构化事件时再次校验编号。
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
    /// 无；参数类型保证序号非零。
    pub fn with_event_sequence(mut self, event_sequence: NonZeroU32) -> Self {
        self.event_sequence = event_sequence;
        self
    }

    /// 由已认证操作人构建成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人；名称快照与请求编号取自该值。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    ///
    /// # 返回
    /// 成功标志为真、说明为空的待持久化审计事实。事件序号为 1。
    ///
    /// # 错误
    /// `resource_id` 去空白后为空时返回 `ValidationError`。
    pub fn resource(
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<Self> {
        if resource_id.trim().is_empty() {
            return Err(Error::ValidationError("资源ID不能为空".to_string()));
        }
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
        let (actor_id, actor_account, actor_type) = actor.into_parts();
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_validated(
            &base,
            actor_id,
            actor_account,
            actor_type,
            action.to_string(),
            resource_type.to_string(),
            Some(resource_id),
            true,
            None,
        )
        .with_actor_name_snapshot(actor_name_snapshot)
        .with_request_id(request_id))
    }
}

/// Port contract uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait ContractAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人。
    /// * `action` - 业务动作名。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    ///
    /// # 返回
    /// 待在同一执行器上持久化的审计事实。
    ///
    /// # 错误
    /// 资源 ID 为空时返回错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedContractAudit>;

    /// 在调用方选定的执行器上持久化先前准备好的审计。
    ///
    /// # 参数
    /// * `audit` - 已准备的审计事实。
    /// * `executor` - 调用方选定的执行器。
    ///
    /// # 返回
    /// 无返回值。持久化完成。
    ///
    /// # 错误
    /// 适配器写入失败时返回对应错误。
    async fn persist(&self, audit: &PreparedContractAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl ContractAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedContractAudit> {
        PreparedContractAudit::resource(actor, action, resource_type, resource_id)
    }

    async fn persist(&self, _audit: &PreparedContractAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
