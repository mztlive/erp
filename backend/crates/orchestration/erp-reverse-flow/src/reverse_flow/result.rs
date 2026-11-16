//! 命令回包使用原命令资格；命令内沿用原执行器，独立恢复使用新授权快照。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::Permission;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::{
    CustomerRefundView, PaymentReversalView, ReceiptReversalView, SupplierRefundView,
};
use persistence_core::{Executor, Transactional};

use super::ReturnsProcess;
use crate::Result;

impl ReturnsProcess {
    /// 在同一快照按原命令权限重验后读取精确结果，不追加详情菜单资格。
    /// # 参数
    /// id、actor 为命令实际结果和操作人；permission、target 为调用处固定的原命令入口及目标。
    /// # 返回
    /// 当前授权快照中的完整结果视图。
    /// # 错误
    /// 原命令资格失效、来源无权、记录不存在或数据库错误时拒绝。
    pub(super) async fn customer_refund_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
    ) -> Result<CustomerRefundView> {
        let process = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.customer_refund_result_read(&id, &actor, &permission, target, executor).await
                })
            })
            .await
    }
    /// 原执行器内按原命令资格读取完整结果。
    /// # 参数
    /// id、actor 为实际结果与操作人，permission、target 为固定命令入口，executor 为原事务。
    /// # 返回
    /// 同一授权快照中的完整命令结果。
    /// # 错误
    /// 当前资格不足、来源无权、记录缺失或读取失败时拒绝。
    pub(super) async fn customer_refund_result_read(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<CustomerRefundView> {
        self.rbac.identity.require(actor, permission, target, executor).await?;
        Ok(ReturnsReadService::new(self.db.clone(), self.rbac.identity.clone())
            .load_customer_refund_view(id, executor)
            .await?)
    }
    /// 在同一快照按原命令权限重验后读取精确结果，不追加详情菜单资格。
    /// # 参数
    /// id、actor 为命令实际结果和操作人；permission、target 为调用处固定的原命令入口及目标。
    /// # 返回
    /// 当前授权快照中的完整结果视图。
    /// # 错误
    /// 原命令资格失效、来源无权、记录不存在或数据库错误时拒绝。
    pub(super) async fn supplier_refund_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
    ) -> Result<SupplierRefundView> {
        let process = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.supplier_refund_result_read(&id, &actor, &permission, target, executor).await
                })
            })
            .await
    }
    /// 原执行器内按原命令资格读取完整结果。
    /// # 参数
    /// id、actor 为实际结果与操作人，permission、target 为固定命令入口，executor 为原事务。
    /// # 返回
    /// 同一授权快照中的完整命令结果。
    /// # 错误
    /// 当前资格不足、来源无权、记录缺失或读取失败时拒绝。
    pub(super) async fn supplier_refund_result_read(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<SupplierRefundView> {
        self.rbac.identity.require(actor, permission, target, executor).await?;
        Ok(ReturnsReadService::new(self.db.clone(), self.rbac.identity.clone())
            .load_supplier_refund_view(id, executor)
            .await?)
    }
    /// 在同一快照按原命令权限重验后读取精确结果，不追加详情菜单资格。
    /// # 参数
    /// id、actor 为命令实际结果和操作人；permission、target 为调用处固定的原命令入口及目标。
    /// # 返回
    /// 当前授权快照中的完整结果视图。
    /// # 错误
    /// 原命令资格失效、来源无权、记录不存在或数据库错误时拒绝。
    pub(super) async fn receipt_reversal_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
    ) -> Result<ReceiptReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.receipt_reversal_result_read(&id, &actor, &permission, target, executor).await
                })
            })
            .await
    }
    /// 原执行器内按原命令资格读取完整结果。
    /// # 参数
    /// id、actor 为实际结果与操作人，permission、target 为固定命令入口，executor 为原事务。
    /// # 返回
    /// 同一授权快照中的完整命令结果。
    /// # 错误
    /// 当前资格不足、来源无权、记录缺失或读取失败时拒绝。
    pub(super) async fn receipt_reversal_result_read(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversalView> {
        self.rbac.identity.require(actor, permission, target, executor).await?;
        Ok(ReturnsReadService::new(self.db.clone(), self.rbac.identity.clone())
            .load_receipt_reversal_view(id, executor)
            .await?)
    }
    /// 在同一快照按原命令权限重验后读取精确结果，不追加详情菜单资格。
    /// # 参数
    /// id、actor 为命令实际结果和操作人；permission、target 为调用处固定的原命令入口及目标。
    /// # 返回
    /// 当前授权快照中的完整结果视图。
    /// # 错误
    /// 原命令资格失效、来源无权、记录不存在或数据库错误时拒绝。
    pub(super) async fn payment_reversal_result(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
    ) -> Result<PaymentReversalView> {
        let process = self.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    process.payment_reversal_result_read(&id, &actor, &permission, target, executor).await
                })
            })
            .await
    }
    /// 原执行器内按原命令资格读取完整结果。
    /// # 参数
    /// id、actor 为实际结果与操作人，permission、target 为固定命令入口，executor 为原事务。
    /// # 返回
    /// 同一授权快照中的完整命令结果。
    /// # 错误
    /// 当前资格不足、来源无权、记录缺失或读取失败时拒绝。
    pub(super) async fn payment_reversal_result_read(
        &self,
        id: &str,
        actor: &AuditActor,
        permission: &Permission,
        target: AccessTarget,
        executor: &mut dyn Executor,
    ) -> Result<PaymentReversalView> {
        self.rbac.identity.require(actor, permission, target, executor).await?;
        Ok(ReturnsReadService::new(self.db.clone(), self.rbac.identity.clone())
            .load_payment_reversal_view(id, executor)
            .await?)
    }
}
