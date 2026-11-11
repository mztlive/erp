//! 采购单写命令的授权快照与事务内账号、范围重验。

use application_core::AuditActor;
use erp_identity::{AuthorizationService, Permission, SharedRbacService};
use erp_procurement::PurchaseAccess;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use persistence_core::{Executor, NoTransaction};

use super::PurchaseOrderProcess;
use crate::{Error, Result};

/// 当前命令的统一授权入口；每次写事务都重新检查，不缓存授权结果。
pub(super) struct PurchaseOrderAuthorization {
    pub(super) rbac: SharedRbacService,
    service: AuthorizationService,
    permission: Permission,
}

impl PurchaseOrderAuthorization {
    /// 在原业务事务核验当前命令的功能资格。
    /// # 参数
    /// actor 为真实操作人，executor 为原业务事务。
    /// # 返回
    /// 当前账号及有效角色具备本命令资格时成功。
    /// # 错误
    /// 身份失效、无权或设施读取失败时返回原错误。
    pub(super) async fn require(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        if !self.service.has_permission(actor, &self.permission, executor).await? {
            return Err(Error::Forbidden(format!("当前账号缺少 {} 权限", self.permission)));
        }
        Ok(())
    }
}

impl PurchaseOrderProcess {
    /// 装配固定命令的授权入口并执行首次资格检查。
    /// # 参数
    /// actor 为当前身份，permission_code 为服务端固定功能权限。
    /// # 返回
    /// 必须在写事务内再次require的命令授权器。
    /// # 错误
    /// 账号、权限或设施读取失败时拒绝。
    pub(super) async fn authorize_actor_permission(
        &self,
        actor: &AuditActor,
        permission_code: &str,
    ) -> Result<PurchaseOrderAuthorization> {
        let rbac = self.require_rbac()?.clone();
        let permission = Permission::parse(permission_code)
            .map_err(|error| Error::Internal(format!("采购单权限不变量损坏: {error}")))?;
        let service = crate::authorization::features(self.db.clone(), rbac.clone());
        let authorization = PurchaseOrderAuthorization { rbac, service, permission };
        authorization.require(actor, &mut NoTransaction).await?;
        Ok(authorization)
    }

    /// 使用已装配的采购范围能力构造写命令检查器。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `action` - 已注册的采购动作
    ///
    /// # 返回
    /// 返回可在事务内重验对象范围的检查器。
    ///
    /// # 错误
    /// 构造不执行 I/O，不返回错误；对象范围由后续检查重验。
    ///
    /// # 关键业务约束
    /// 每次检查均重新解析服务端授权，不得复用列表上下文。
    pub(super) fn command_access(&self, actor: &AuditActor, action: &'static str) -> PurchaseCommandAccess {
        PurchaseCommandAccess { access: self.ports.access(self.db.clone()), actor: actor.clone(), action }
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
