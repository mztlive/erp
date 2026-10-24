//! 支撑领域命令写入跨域审计的消费方端口。

use std::num::NonZeroU32;

use application_core::{AuditActor, AuditContext, impl_audit_context};
use async_trait::async_trait;
use entity_core::BaseModel;
use erp_core::AccountKind;
use persistence_core::Executor;

use crate::error::{Error, Result};

/// 支撑领域可经端口持久化的成功资源审计事实。
///
/// 支撑领域不依赖 `erp-audit` 类型。组合根适配器把该事实转成 `AuditLog`，
/// 并写到同一个 [`Executor`]。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedSupportAudit {
    /// 稳定的审计文档 ID。
    pub id: String,
    /// 构造时捕获的乐观锁版本。
    pub version: u64,
    /// 构造时捕获的创建时间。
    pub created_at: u64,
    /// 构造时捕获的更新时间。
    pub updated_at: u64,
    /// 构造时捕获的软删除标记。
    pub deleted_at: u64,
    /// 操作人账号 ID。
    pub actor_id: String,
    /// 操作人登录账号。
    pub actor_account: String,
    /// 操作人种类。
    pub actor_type: AccountKind,
    /// 命令认证时捕获的安全操作人显示名快照。
    pub actor_name_snapshot: Option<String>,
    /// 调用时捕获的请求关联；请求之外为 `None`。
    pub request_id: Option<String>,
    /// 同一业务命令内从 1 开始的事件序号。
    pub event_sequence: NonZeroU32,
    /// 业务动作名。
    pub action: String,
    /// 资源类型。
    pub resource_type: String,
    /// 资源 ID。
    pub resource_id: Option<String>,
    /// 是否成功；支撑领域只准备成功的资源审计。
    pub success: bool,
    /// 可选业务说明。
    pub message: Option<String>,
}

impl_audit_context!(PreparedSupportAudit);

impl PreparedSupportAudit {
    /// 从已校验的审计实体快照采集支撑侧字段。
    ///
    /// # 参数
    /// * `base` - 已构造审计的持久化元数据
    /// * `actor_id` - 操作人账号 ID
    /// * `actor_account` - 操作人登录账号
    /// * `actor_type` - 操作人种类
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID；缺失时为 `None`
    /// * `success` - 是否成功
    /// * `message` - 可选业务说明
    ///
    /// # 返回
    /// 返回供稍后持久化的审计事实；名称快照与请求编号初始为 `None`，事件序号为 `1`。
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

    /// 由已认证操作人构造成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    ///
    /// # 返回
    /// 返回成功标记为 `true`、并带上操作人名称快照与请求编号的审计事实。
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
        let context = AuditContext::from(actor);
        let id = id_generator::next_id();
        let base = BaseModel::new(id);
        Ok(Self::from_validated(
            &base,
            context.actor_id,
            context.actor_account,
            context.actor_type,
            action.to_string(),
            resource_type.to_string(),
            Some(resource_id),
            true,
            None,
        )
        .with_actor_name_snapshot(context.actor_name_snapshot)
        .with_request_id(context.request_id))
    }
}

/// 支撑领域在调用方执行器上准备并持久化资源审计的端口。
#[async_trait]
pub trait SupportAuditPort: Send + Sync {
    /// 在事务开始前校验并准备一条成功的资源审计。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 业务动作名
    /// * `resource_type` - 资源类型
    /// * `resource_id` - 资源 ID
    ///
    /// # 返回
    /// 返回可交给 [`Self::persist`] 写入的审计事实。
    ///
    /// # 错误
    /// 资源 ID 为空或实现无法准备审计时返回错误。本方法不写库。
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedSupportAudit>;

    /// 把先前准备好的审计写到调用方选定的执行器。
    ///
    /// # 参数
    /// * `audit` - 已准备的审计事实
    /// * `executor` - 调用方选定的执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// 持久化失败时返回错误。
    async fn persist(&self, audit: &PreparedSupportAudit, executor: &mut dyn Executor) -> Result<()>;
}

/// 组合根尚未注入适配器时使用的失败关闭审计端口。
#[derive(Debug, Default, Clone, Copy)]
pub struct FailClosedAuditPort;

#[async_trait]
impl SupportAuditPort for FailClosedAuditPort {
    fn resource_log(
        &self,
        actor: AuditActor,
        action: &str,
        resource_type: &str,
        resource_id: String,
    ) -> Result<PreparedSupportAudit> {
        PreparedSupportAudit::resource(actor, action, resource_type, resource_id)
    }

    /// 拒绝写入未接线的审计端口。
    ///
    /// # 参数
    /// * `_audit` - 已准备的审计事实；本实现不读取
    /// * `_executor` - 调用方执行器；本实现不写入
    ///
    /// # 返回
    /// 不返回成功。
    ///
    /// # 错误
    /// 始终返回 `Internal`（审计端口未接线）。
    async fn persist(&self, _audit: &PreparedSupportAudit, _executor: &mut dyn Executor) -> Result<()> {
        Err(Error::Internal("审计端口未接线".to_string()))
    }
}
