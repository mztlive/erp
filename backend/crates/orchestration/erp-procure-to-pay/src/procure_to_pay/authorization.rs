//! 采购单写命令的授权快照与事务内账号、范围重验。

use application_core::AuditActor;
use erp_permissions::Permission;
use erp_procurement::PurchaseAccess;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use persistence_core::Executor;

use super::PurchaseOrderProcess;
use crate::Result;

impl PurchaseOrderProcess {
    /// 使用已装配的采购范围能力构造写命令检查器。
    ///
    /// # 参数
    /// * `actor` - 已认证操作人
    /// * `entry` - 中央登记的采购权限常量
    ///
    /// # 返回
    /// 返回可在事务内重验对象范围的检查器。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 每次检查均重新解析服务端授权，不得复用列表上下文。
    pub(super) fn command_access(&self, actor: &AuditActor, entry: &Permission) -> PurchaseCommandAccess {
        PurchaseCommandAccess {
            access: self.ports.access(self.db.clone()),
            actor: actor.clone(),
            entry: entry.clone(),
        }
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
    entry: Permission,
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
        self.access.require_object(&self.actor, &self.entry, id, executor).await.map_err(crate::Error::from)
    }
}
