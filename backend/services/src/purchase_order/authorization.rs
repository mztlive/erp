//! 采购单写命令的授权快照与事务内账号重验。

use database::{AccessControlExt, Executor, NoTransaction};
use entities::Permission;

use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::{subject, SharedRbacService};

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 已校验且必须绑定到采购单写事务提交的授权上下文。
pub(super) struct PurchaseOrderAuthorization {
    /// 与策略版本一致的共享授权源。
    pub(super) rbac: SharedRbacService,
    /// 操作人权限判定使用的稳定策略版本。
    pub(super) policy_revision: u64,
}

impl PurchaseOrderService {
    /// 为采购单写命令形成稳定的账号与权限快照。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `permission_code` - 当前命令要求的静态权限
    ///
    /// # 返回
    /// 返回账号可登录且拥有指定权限时使用的授权源与稳定策略版本。
    ///
    /// # 错误
    /// 未注入 RBAC、账号不存在或已停用、身份变化、权限不足或策略持续变化时返回错误。
    ///
    /// # 关键业务约束
    /// 调用方必须把返回版本传给 `run_authorized_policy_transaction`，不得用普通事务提交写命令。
    pub(super) async fn authorize_actor_permission(
        &self,
        actor: &AuditActor,
        permission_code: &str,
    ) -> Result<PurchaseOrderAuthorization> {
        let rbac = self.require_rbac()?;
        let permission = Permission::parse(permission_code)
            .map_err(|error| Error::Internal(format!("采购单权限不变量损坏: {error}")))?;
        for _ in 0..AUTHORIZATION_SNAPSHOT_ATTEMPTS {
            let before = rbac.current_policy_revision().await?;
            ensure_purchase_order_actor_account(&self.db, actor, &mut NoTransaction).await?;
            if !rbac
                .enforce(&subject(actor.kind(), actor.id()), &permission)
                .await?
            {
                return Err(Error::Forbidden(format!("当前账号缺少 {permission_code} 权限")));
            }
            let after = rbac.current_policy_revision().await?;
            if before == after {
                return Ok(PurchaseOrderAuthorization {
                    rbac: rbac.clone(),
                    policy_revision: before,
                });
            }
        }
        Err(Error::Rbac(
            "采购单授权策略持续变化，无法形成稳定快照".to_string(),
        ))
    }
}

/// 校验采购单写命令操作人的持久化账号仍可登录且身份未变化。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `actor` - 已认证操作人
/// * `executor` - 数据库执行器，可为采购单写事务会话
///
/// # 返回
/// 账号存在、类型一致且可登录时返回 `Ok(())`。
///
/// # 错误
/// 账号不存在、已停用、身份变化或仓储查询失败时返回错误。
///
/// # 关键业务约束
/// policy revision 不能覆盖账号状态变化，因此每个写事务都必须在会话内再次调用本函数。
pub(super) async fn ensure_purchase_order_actor_account(
    db: &mongodb::Database,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    db.accounts()
        .find_by_id(actor.id(), executor)
        .await?
        .filter(|account| account.kind == actor.kind() && account.can_login())
        .ok_or_else(|| Error::Forbidden("采购单操作账号不存在、已停用或身份已变化".to_string()))?;
    Ok(())
}
