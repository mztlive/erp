//! 客户命令跨域审计持久化的消费端口。

use std::num::NonZeroU32;

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
    /// 事件发生时已捕获的安全操作人名称；未知保持缺失。
    pub actor_name_snapshot: Option<String>,
    /// 认证请求的安全关联标识；无请求时保持缺失。
    pub request_id: Option<String>,
    /// 同一业务命令内事件的正整数序号。
    pub event_sequence: NonZeroU32,
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
    /// 携带构造时已经校验的安全操作人名称，不查询当前账号。
    ///
    /// # 参数
    /// * `name` - 事件发生时名称快照；未知保持 `None`。
    ///
    /// # 返回
    /// 返回包含名称快照的预制审计事实。
    ///
    /// # 错误
    /// 无；持久化适配器重新验证安全快照。
    pub fn with_actor_name_snapshot(mut self, name: Option<String>) -> Self {
        self.actor_name_snapshot = name;
        self
    }

    /// 携带认证上下文已校验的请求标识，不生成替代标识。
    ///
    /// # 参数
    /// * `request_id` - 当前请求的安全标识；无请求保持 `None`。
    ///
    /// # 返回
    /// 返回包含原请求标识的预制审计事实。
    ///
    /// # 错误
    /// 无；持久化适配器重新验证请求标识。
    pub fn with_request_id(mut self, request_id: Option<String>) -> Self {
        self.request_id = request_id;
        self
    }

    /// 保留已校验业务事件在命令内的原序号。
    ///
    /// # 参数
    /// * `event_sequence` - 业务事件的正整数序号。
    ///
    /// # 返回
    /// 返回包含原序号的预制审计事实。
    ///
    /// # 错误
    /// 无；类型保证序号大于零。
    pub fn with_event_sequence(mut self, event_sequence: NonZeroU32) -> Self {
        self.event_sequence = event_sequence;
        self
    }

    /// 从已校验的审计实体快照捕获客户侧字段。
    ///
    /// 参数对象 [`ValidatedAuditSnapshot`] 收敛九个调用参数（erp-customer-012），
    /// 避免调用方传参顺序易错且难以扩展；本函数为新代码的参数对象入口。
    ///
    /// # 参数
    /// * `snapshot` - 已校验的审计实体快照
    ///
    /// # 返回
    /// 返回稍后持久化的不透明审计事实。
    ///
    /// # 错误
    /// 无；输入字段由调用方预先校验。
    pub fn from_snapshot(snapshot: ValidatedAuditSnapshot) -> Self {
        Self {
            id: snapshot.base.id.clone(),
            version: snapshot.base.version,
            created_at: snapshot.base.created_at,
            updated_at: snapshot.base.updated_at,
            deleted_at: snapshot.base.deleted_at,
            actor_id: snapshot.actor_id,
            actor_account: snapshot.actor_account,
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            actor_type: snapshot.actor_type,
            action: snapshot.action,
            resource_type: snapshot.resource_type,
            resource_id: snapshot.resource_id,
            success: snapshot.success,
            message: snapshot.message,
        }
    }

    /// 从已校验的审计实体快照捕获客户侧字段。
    ///
    /// 组合层调用方可使用九参入口捕获完整事件字段；
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
    ///
    /// # 错误
    /// 无；输入字段由调用方预先校验。
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

    /// 由已认证操作人构造一条成功的资源审计。
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
        let actor_name_snapshot = actor.actor_name_snapshot().map(str::to_string);
        let request_id = actor.request_id().map(str::to_string);
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
        })
        .with_actor_name_snapshot(actor_name_snapshot)
        .with_request_id(request_id))
    }
}

/// Port customer uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait CustomerAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已通过鉴权的审计操作人
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源业务 ID
    ///
    /// # 返回
    /// 返回可在调用方执行器上持久化的审计事实。
    ///
    /// # 错误
    /// 资源 ID 为空或审计事实无法准备时返回对应错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedCustomerAudit>;

    /// 在调用方选择的执行器上持久化已准备的审计。
    ///
    /// # 参数
    /// * `audit` - 已准备的审计事实
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 审计写入完成。
    ///
    /// # 错误
    /// 审计持久化失败时返回对应错误。
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

#[cfg(test)]
mod tests {
    use super::*;

    fn actor() -> AuditActor {
        AuditActor::new("actor".into(), "login".into(), AccountKind::Admin)
    }

    #[test]
    fn resource_captures_known_name_and_leaves_unknown_name_missing() {
        let named = actor()
            .with_actor_name_snapshot(Some("周晓彤".into()))
            .unwrap()
            .with_request_id(Some("request-original".into()))
            .unwrap();
        let prepared =
            PreparedCustomerAudit::resource(named, "customer.update", "customer", "object-1".into()).unwrap();
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence, NonZeroU32::MIN);
        assert_eq!(prepared.actor_id, "actor");
        assert_eq!(prepared.resource_id.as_deref(), Some("object-1"));
        let unknown =
            PreparedCustomerAudit::resource(actor(), "customer.update", "customer", "object-1".into())
                .unwrap();
        assert_eq!(unknown.actor_name_snapshot, None);
        assert_eq!(unknown.request_id, None);
        assert_eq!(unknown.event_sequence, NonZeroU32::MIN);
        assert!(
            PreparedCustomerAudit::resource(actor(), "customer.update", "customer", " ".into(),).is_err()
        );
    }
}
