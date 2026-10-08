//! 主体命令跨域审计持久化的消费方端口。

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 主体可通过端口持久化的、已准备好的成功资源审计。
///
/// 主体不依赖 `erp-audit` 的类型。组合根适配器把该事实转换成 `AuditLog`，并写到同一个 [`Executor`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedPartyAudit {
    /// 稳定的审计文档 ID。
    pub id: String,
    /// 构造时捕获的乐观锁版本。
    pub version: u64,
    /// 构造时捕获的创建时间戳。
    pub created_at: u64,
    /// 构造时捕获的更新时间戳。
    pub updated_at: u64,
    /// 构造时捕获的软删除标记。
    pub deleted_at: u64,
    /// 操作人账号 ID。
    pub actor_id: String,
    /// 操作人登录账号。
    pub actor_account: String,
    /// 事件发生时已捕获的安全操作人名称；未知保持缺失。
    pub actor_name_snapshot: Option<String>,
    /// 认证请求的安全关联标识；无请求时保持缺失。
    pub request_id: Option<String>,
    /// 同一业务命令内事件的正整数序号。
    pub event_sequence: NonZeroU32,
    /// 操作人类型。
    pub actor_type: AccountKind,
    /// 业务动作名称。
    pub action: String,
    /// 资源类型。
    pub resource_type: String,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 是否成功；主体只准备成功的资源审计。
    pub success: bool,
    /// 可选的业务说明。
    pub message: Option<String>,
}

impl PreparedPartyAudit {
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

    /// 从已经校验的审计实体快照捕获主体侧字段。
    ///
    /// # 参数
    /// * `base` - 构造时的持久化元数据。
    /// * `actor_id` - 操作人 ID。
    /// * `actor_account` - 操作人登录账号。
    /// * `actor_type` - 操作人类型。
    /// * `action` - 动作名称。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `success` - 是否成功。
    /// * `message` - 可选说明。
    ///
    /// # 返回
    /// 返回稍后持久化的预制审计事实。
    ///
    /// # 错误
    /// 不返回错误；输入字段由调用方预先校验。
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
            actor_name_snapshot: None,
            request_id: None,
            event_sequence: NonZeroU32::MIN,
            actor_type,
            action,
            resource_type,
            resource_id,
            success,
            message,
        }
    }

    /// 捕获已鉴权操作人及其安全名称，构造成功资源事件。
    ///
    /// # 参数
    /// * `actor` - 已鉴权身份及名称快照。
    /// * `action` - 业务动作。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 业务资源 ID。
    ///
    /// # 返回
    /// 返回保留发生时名称的预制审计事实。
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

/// 主体用来在调用方执行器上准备并持久化资源审计的端口。
#[async_trait]
pub trait PartyAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已鉴权操作人。
    /// * `action` - 业务动作。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 业务资源 ID。
    ///
    /// # 返回
    /// 返回可交给 [`Self::persist`] 的预制审计事实。
    ///
    /// # 错误
    /// 无法形成该审计事实时返回对应错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedPartyAudit>;

    /// 在调用方选定的执行器上写入此前准备好的审计。
    ///
    /// # 参数
    /// * `audit` - 已准备的审计事实。
    /// * `executor` - 调用方选定的数据访问执行器。
    ///
    /// # 返回
    /// 写入完成后返回 `Ok(())`。
    ///
    /// # 错误
    /// 持久化失败时返回对应错误。
    async fn persist(&self, audit: &PreparedPartyAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// 组合根未注入适配器时使用的失败关闭审计端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl PartyAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedPartyAudit> {
        PreparedPartyAudit::resource(actor, action, resource_type, resource_id)
    }

    /// 组合根未注入适配器时拒绝写入。
    ///
    /// # 参数
    /// * `_audit` - 已准备的审计事实；本实现不读取。
    /// * `_executor` - 调用方执行器；本实现不使用。
    ///
    /// # 返回
    /// 不返回成功。
    ///
    /// # 错误
    /// 始终返回 `Internal`，提示审计端口未接线。
    async fn persist(&self, _audit: &PreparedPartyAudit, _executor: &mut dyn Executor) -> Result<()> {
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
            PreparedPartyAudit::resource(named, "party.update", "party", "object-1".into()).unwrap();
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence, NonZeroU32::MIN);
        assert_eq!(prepared.actor_id, "actor");
        assert_eq!(prepared.resource_id.as_deref(), Some("object-1"));
        let unknown =
            PreparedPartyAudit::resource(actor(), "party.update", "party", "object-1".into()).unwrap();
        assert_eq!(unknown.actor_name_snapshot, None);
        assert_eq!(unknown.request_id, None);
        assert_eq!(unknown.event_sequence, NonZeroU32::MIN);
        assert!(PreparedPartyAudit::resource(actor(), "party.update", "party", " ".into(),).is_err());
    }
}
