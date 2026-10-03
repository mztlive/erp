//! 采购变更编辑快照只对来源采购单的完整写资格开放成本。

use application_core::AuditActor;
use erp_procurement::dto::purchase_order::PurchaseChangeDraftView;
use erp_procurement::service::purchase_order::PurchaseOrderService;
use persistence_core::Transactional;

use super::super::PurchaseOrderProcess;
use crate::Result;

impl PurchaseOrderProcess {
    /// 在统一授权快照内读取原采购变更草稿的完整冻结目标。
    ///
    /// # 参数
    /// * `id` - 原采购变更单主键。
    /// * `actor` - 已认证操作人。
    /// # 返回
    /// 返回完整目标行、付款条件、原原因和当前版本。
    /// # 错误
    /// 缺更新或提交资格、来源对象不可操作、非草稿或基准失效时拒绝。
    /// # 关键业务约束
    /// 同角色须同时具备采购更新和提交权限；两个动作范围均须覆盖来源单。
    /// 历史参与或仅有详情资格不能读取此编辑成本快照。
    pub async fn change_draft(&self, id: &str, actor: &AuditActor) -> Result<PurchaseChangeDraftView> {
        let db = self.db.clone();
        let access = crate::adapters::purchase_access(db.clone(), self.require_rbac()?.clone());
        let id = id.to_string();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let domain = PurchaseOrderService::new(db);
                    let change = domain.load_change(&id, executor).await?;
                    access
                        .require_object(
                            &actor,
                            "update",
                            change.purchase_order_id.as_ref(),
                            &["purchase_order:submit".into()],
                            executor,
                        )
                        .await?;
                    let order = access
                        .require_object(
                            &actor,
                            "submit",
                            change.purchase_order_id.as_ref(),
                            &["purchase_order:update".into()],
                            executor,
                        )
                        .await?;
                    Ok(domain.change_draft(&change, &order, executor).await?)
                })
            })
            .await
    }
}
