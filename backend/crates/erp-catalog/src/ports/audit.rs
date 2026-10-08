//! 商品命令跨域持久化审计的消费端口。

use std::num::NonZeroU32;

use application_core::AuditActor;
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// Prepared successful resource audit that catalog can persist through a port.
///
/// Catalog never depends on `erp-audit` types. Composition-root adapters convert
/// this fact into an `AuditLog` and write it on the same [`Executor`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedCatalogAudit {
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
    /// Success flag; catalog only prepares successful resource audits.
    pub success: bool,
    /// Optional business message.
    pub message: Option<String>,
}

impl PreparedCatalogAudit {
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

    /// 捕获已校验目录事件的身份、持久化元数据及安全业务字段。
    ///
    /// # 参数
    /// * `base` - 事件持久化元数据。
    /// * `actor_id` - 操作人 ID。
    /// * `actor_account` - 操作人登录账号。
    /// * `actor_type` - 操作人类型。
    /// * `action` - 业务动作。
    /// * `resource_type` - 资源类型。
    /// * `resource_id` - 资源 ID。
    /// * `success` - 成功标记。
    /// * `message` - 业务说明。
    ///
    /// # 返回
    /// 返回稍后持久化的预制审计事实。
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
    /// * `message` - 业务说明；安全投影由组合层执行。
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
        Ok(Self::from_validated(
            &base,
            actor_id,
            actor_account,
            actor_type,
            action.to_string(),
            resource_type.to_string(),
            Some(resource_id),
            true,
            message,
        )
        .with_actor_name_snapshot(actor_name_snapshot)
        .with_request_id(request_id))
    }
}

/// Port catalog uses to prepare and persist resource audits on a caller executor.
#[async_trait]
pub trait CatalogAuditPort: Send + Sync {
    /// 在事务前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已鉴权操作人
    /// * `action` - 业务动作
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    ///
    /// # 返回
    /// 返回可在调用方执行器上持久化的审计事实。
    ///
    /// # 错误
    /// 资源 ID 为空等校验失败时返回对应错误。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedCatalogAudit>;

    /// 在事务前校验并准备一条带业务说明的成功资源审计。
    ///
    /// # 参数
    /// * `actor` - 已鉴权操作人
    /// * `action` - 业务动作
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    /// * `message` - 可选业务说明
    ///
    /// # 返回
    /// 返回带业务说明的预制审计事实。
    ///
    /// # 错误
    /// 资源 ID 为空等校验失败时返回对应错误。
    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedCatalogAudit>;

    /// 在调用方选定的执行器上持久化已准备的审计。
    ///
    /// # 参数
    /// * `audit` - 事先准备好的审计事实
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 写入完成时无返回值。
    ///
    /// # 错误
    /// 端口未接线或写入失败时返回对应错误。
    async fn persist(&self, audit: &PreparedCatalogAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// Fail-closed audit port used when composition has not injected an adapter.
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl CatalogAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedCatalogAudit> {
        PreparedCatalogAudit::resource(actor, action, resource_type, resource_id, None)
    }

    fn resource_log_with_message(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
        message: Option<String>,
    ) -> Result<PreparedCatalogAudit> {
        PreparedCatalogAudit::resource(actor, action, resource_type, resource_id, message)
    }

    async fn persist(&self, _audit: &PreparedCatalogAudit, _executor: &mut dyn Executor) -> Result<()> {
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
            PreparedCatalogAudit::resource(named, "product.update", "product", "object-1".into(), None)
                .unwrap();
        assert_eq!(prepared.actor_name_snapshot.as_deref(), Some("周晓彤"));
        assert_eq!(prepared.request_id.as_deref(), Some("request-original"));
        assert_eq!(prepared.event_sequence, NonZeroU32::MIN);
        assert_eq!(prepared.actor_id, "actor");
        assert_eq!(prepared.resource_id.as_deref(), Some("object-1"));
        let unknown =
            PreparedCatalogAudit::resource(actor(), "product.update", "product", "object-1".into(), None)
                .unwrap();
        assert_eq!(unknown.actor_name_snapshot, None);
        assert_eq!(unknown.request_id, None);
        assert_eq!(unknown.event_sequence, NonZeroU32::MIN);
        assert!(
            PreparedCatalogAudit::resource(actor(), "product.update", "product", " ".into(), None,).is_err()
        );
    }
}
