//! 命令回包在新的快照中重验原命令资格，不复用已结束事务的授权。
use application_core::AuditActor;
use erp_identity::AccessTarget;
use erp_permissions::Permission;
use erp_read_models::returns_center::ReturnsReadService;
use erp_read_models::returns_center::dto::{
    CustomerRefundView, PaymentReversalView, ReceiptReversalView, SupplierRefundView,
};
use persistence_core::Transactional;

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
        let db = self.db.clone();
        let authorization = self.rbac.identity.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require(&actor, &permission, target, executor).await?;
                    Ok(ReturnsReadService::new(db, authorization)
                        .load_customer_refund_view(&id, executor)
                        .await?)
                })
            })
            .await
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
        let db = self.db.clone();
        let authorization = self.rbac.identity.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require(&actor, &permission, target, executor).await?;
                    Ok(ReturnsReadService::new(db, authorization)
                        .load_supplier_refund_view(&id, executor)
                        .await?)
                })
            })
            .await
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
        let db = self.db.clone();
        let authorization = self.rbac.identity.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require(&actor, &permission, target, executor).await?;
                    Ok(ReturnsReadService::new(db, authorization)
                        .load_receipt_reversal_view(&id, executor)
                        .await?)
                })
            })
            .await
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
        let db = self.db.clone();
        let authorization = self.rbac.identity.clone();
        let actor = actor.clone();
        let permission = permission.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization.require(&actor, &permission, target, executor).await?;
                    Ok(ReturnsReadService::new(db, authorization)
                        .load_payment_reversal_view(&id, executor)
                        .await?)
                })
            })
            .await
    }
}
