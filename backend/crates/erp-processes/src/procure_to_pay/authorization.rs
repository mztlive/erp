//! 采购单写命令的授权快照与事务内账号、范围重验。

use application_core::AuditActor;
use erp_identity::{AccessControlExt, Permission, SharedRbacService, subject};
use erp_procurement::PurchaseAccess;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use persistence_core::{Executor, NoTransaction};

use super::PurchaseOrderProcess;
use crate::{Error, Result};

const AUTHORIZATION_SNAPSHOT_ATTEMPTS: usize = 3;

/// 已校验且必须绑定到采购单写事务提交的授权上下文。
pub(super) struct PurchaseOrderAuthorization {
    /// 与策略版本一致的共享授权源。
    pub(super) rbac: SharedRbacService,
    /// 操作人权限判定使用的稳定策略版本。
    pub(super) policy_revision: u64,
}

impl PurchaseOrderProcess {
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
            if !rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await? {
                return Err(Error::Forbidden(format!("当前账号缺少 {permission_code} 权限")));
            }
            let after = rbac.current_policy_revision().await?;
            if before == after {
                return Ok(PurchaseOrderAuthorization { rbac: rbac.clone(), policy_revision: before });
            }
        }
        Err(Error::Rbac("采购单授权策略持续变化，无法形成稳定快照".to_string()))
    }

    /// 构造写命令范围检查器；缺少身份装配时拒绝，不退回路由级授权。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    ///
    /// # 返回
    /// 返回可在事务内重验对象范围的检查器。
    ///
    /// # 错误
    /// 未注入 RBAC 时拒绝。
    ///
    /// # 关键业务约束
    /// 每次检查均重新解析服务端授权，不得复用列表上下文。
    pub(super) fn command_access(
        &self,
        actor: &AuditActor,
        action: &'static str,
    ) -> Result<PurchaseCommandAccess> {
        Ok(PurchaseCommandAccess {
            access: crate::adapters::purchase_access(self.db.clone(), self.require_rbac()?.clone()),
            actor: actor.clone(),
            action,
        })
    }
}

/// 命令上下文只保存请求身份；每次检查均重新解析服务端授权。
#[derive(Clone)]
pub(super) struct PurchaseCommandAccess {
    /// 采购范围解析器。
    access: PurchaseAccess,
    /// 已认证操作人。
    actor: AuditActor,
    /// 本次命令动作。
    action: &'static str,
}

impl PurchaseCommandAccess {
    /// 在调用方事务内读取当前可操作单据；历史参与不会产生写资格。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 对象在范围内时返回采购单。
    ///
    /// # 错误
    /// 不存在或越权时返回 NotFound，不泄露存在性。
    ///
    /// # 关键业务约束
    /// 写命令必须在原事务重验，不得把入口事前检查当作凭证。
    pub(super) async fn current(&self, id: &str, executor: &mut dyn Executor) -> Result<PurchaseOrder> {
        self.access
            .require_object(&self.actor, self.action, id, &[], executor)
            .await
            .map_err(crate::Error::from)
    }

    /// 写入前重新读取当前责任与版本，防止预读取后交接或状态变化。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `expected` - 预读取时的乐观锁版本
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 范围与版本未变化时成功。
    ///
    /// # 错误
    /// 范围失效返回 NotFound；版本变化返回冲突。
    ///
    /// # 关键业务约束
    /// 不得在范围变化后继续写入。
    pub(super) async fn revalidate(
        &self,
        id: &str,
        expected: u64,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let current = self.current(id, executor).await?;
        if current.base.version != expected {
            return Err(Error::ConflictError("采购单责任或版本已变化，请刷新后重试".into()));
        }
        Ok(())
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
