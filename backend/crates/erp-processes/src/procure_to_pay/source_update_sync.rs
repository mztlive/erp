//! 采购依据更新记录与待办的对账（第一期 §6.3“销售变更对采购的影响”第 3、4、11 条）。
//!
//! 每次供给覆盖或采购状态变化后按当前事实重算受影响采购：新受影响的开立记录并派
//! 待办，仍受影响的刷新原因；不再受影响的草稿等待重新提交，其他按事实判定处理方式
//! 并关闭待办。

use std::collections::HashMap;

use erp_core::common::time::Instant;
use erp_core::ids::WorkItemId;
use erp_procurement::entity::purchase_order::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseSourceImpact, PurchaseSourceResolution, PurchaseSourceUpdate,
    PurchaseSourceUpdateStatus, SourceUpdateOpening,
};
use erp_procurement::repository::purchase_source_update::{list_by_sales_order, purchase_source_updates};
use erp_read_models::purchase_center::source_impact::PurchaseSourceState;
use erp_sales::entity::sales_order::SalesOrder;
use erp_workflow::WorkItemExt;
use erp_workflow::entity::work_item::{
    AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemStatus, WorkItemType,
};
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use crate::reverse_flow::return_finance::sync_return_finance;
use crate::{Error, Result};

/// 销售变更生效触发的待办原因代码。
const SALES_CHANGE_REASON: &str = "SALES_CHANGE_EFFECTIVE";
/// 其他供给变化触发的待办原因代码。
const SOURCE_CHANGED_REASON: &str = "SALES_SOURCE_CHANGED";

/// 本次对账的固定上下文。
pub(super) struct SourceUpdateSync<'a> {
    /// 业务数据库。
    pub db: &'a Database,
    /// 来源销售单。
    pub order: &'a SalesOrder,
    /// 同一批事实得到的覆盖与受影响采购。
    pub state: &'a PurchaseSourceState,
    /// 触发的销售变更单；非销售变更路径为空。
    pub trigger: Option<&'a str>,
    /// 对账时间。
    pub now: Instant,
}

impl SourceUpdateSync<'_> {
    /// 对账销售单下全部依据更新记录与待办。
    ///
    /// # 参数
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部记录与待办同步成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 记录状态、待办字段或仓储写入失败时返回错误。
    pub(super) async fn run(&self, executor: &mut dyn Executor) -> Result<()> {
        let mut open = list_by_sales_order(self.db, &self.order.base.id, executor)
            .await?
            .into_iter()
            .filter(|update| update.status == PurchaseSourceUpdateStatus::Open)
            .map(|update| (update.purchase_order_id.clone(), update))
            .collect::<HashMap<_, _>>();
        for purchase in &self.state.facts.purchase_orders {
            let impact = self.state.impact_of(&purchase.base.id);
            let update = open.remove(&purchase.base.id);
            match (sync_action(impact.is_some(), update.is_some(), purchase.stable.status), impact, update) {
                (SyncAction::Refresh, Some(impact), Some(update)) => {
                    self.refresh(update, impact, executor).await?
                },
                (SyncAction::Open, Some(impact), _) => self.open(purchase, impact, executor).await?,
                (SyncAction::AwaitSubmission, _, Some(update)) => {
                    self.await_submission(update, executor).await?
                },
                (SyncAction::Close, _, Some(update)) => self.close(update, executor).await?,
                _ => {},
            }
            sync_return_finance(self.db, &purchase.base.id, executor).await?;
            super::sync_payment_arrangement(self.db, &purchase.base.id, false, executor).await?;
        }
        for update in open.into_values() {
            self.close(update, executor).await?;
        }
        Ok(())
    }

    /// 当前销售版本 ID。
    fn revision_id(&self) -> &str {
        &self.state.coverage.revision.base.id
    }

    /// 为新受影响采购开立记录并派待办。
    async fn open(
        &self,
        purchase: &PurchaseOrder,
        impact: &PurchaseSourceImpact,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let opening = SourceUpdateOpening {
            order: purchase,
            impact,
            sales_order_revision_id: self.revision_id(),
            trigger_sales_change_order_id: self.trigger,
        };
        let mut update = PurchaseSourceUpdate::open(next_id(), opening);
        if let Some(owner) = purchase.owner_user_id.as_deref() {
            let item = self.work_item(purchase, owner, &update)?;
            self.db.work_items().create(&item, executor).await?;
            update.attach_work_item(item.base.id.clone());
        }
        purchase_source_updates(self.db).create(&update, executor).await?;
        Ok(())
    }

    /// 刷新仍受影响采购的记录与待办摘要。
    async fn refresh(
        &self,
        mut update: PurchaseSourceUpdate,
        impact: &PurchaseSourceImpact,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !update.refresh(impact, self.revision_id(), self.trigger).map_err(Error::Logic)? {
            return Ok(());
        }
        self.save_open(update, executor).await
    }

    /// 草稿已不再受影响，转为等待重新提交并更新待办摘要。
    async fn await_submission(
        &self,
        mut update: PurchaseSourceUpdate,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if !update.await_submission(self.revision_id()).map_err(Error::Logic)? {
            return Ok(());
        }
        self.save_open(update, executor).await
    }

    /// 保存仍开放的记录，并同步待办摘要。
    async fn save_open(&self, mut update: PurchaseSourceUpdate, executor: &mut dyn Executor) -> Result<()> {
        purchase_source_updates(self.db).update(&mut update, executor).await?;
        let Some(mut item) = self.open_work_item(&update, executor).await? else {
            return Ok(());
        };
        item.update_impact_summary(Some(update.impact_summary(&self.order.order_no)))
            .map_err(Error::Logic)?;
        self.db.work_items().update(&mut item, executor).await?;
        Ok(())
    }

    /// 按事实判定处理方式并关闭记录与待办。
    async fn close(&self, mut update: PurchaseSourceUpdate, executor: &mut dyn Executor) -> Result<()> {
        let purchase = self
            .state
            .facts
            .purchase_orders
            .iter()
            .find(|purchase| purchase.base.id == update.purchase_order_id);
        let (resolution, document) = derive_resolution(&update, purchase, self.revision_id());
        update.resolve(resolution, document, self.now).map_err(Error::Logic)?;
        purchase_source_updates(self.db).update(&mut update, executor).await?;
        if let Some(mut item) = self.open_work_item(&update, executor).await? {
            item.complete_when_source_updated(self.now).map_err(Error::Logic)?;
            self.db.work_items().update(&mut item, executor).await?;
        }
        Ok(())
    }

    /// 读取记录关联且仍开放的待办。
    async fn open_work_item(
        &self,
        update: &PurchaseSourceUpdate,
        executor: &mut dyn Executor,
    ) -> Result<Option<WorkItem>> {
        let Some(id) = update.work_item_id.as_deref() else {
            return Ok(None);
        };
        let item = self.db.work_items().find_by_id(id, executor).await?;
        Ok(item.filter(|item| item.status == WorkItemStatus::Open))
    }

    /// 构造采购负责人的依据更新待办。
    fn work_item(
        &self,
        purchase: &PurchaseOrder,
        owner: &str,
        update: &PurchaseSourceUpdate,
    ) -> Result<WorkItem> {
        let reason = if self.trigger.is_some() { SALES_CHANGE_REASON } else { SOURCE_CHANGED_REASON };
        WorkItem::new_with_responsibility_key(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseSourceUpdate,
                business_object_type: "purchase_order".to_string(),
                business_object_id: purchase.base.id.clone(),
                subject_version: self.revision_id().to_string(),
                owner_role: "role-procurement".to_string(),
                owner_organization_id: purchase.business_org_unit_id.clone(),
                owner_user_id: owner.to_string(),
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::High,
                due_at: None,
                reason_code: Some(reason.to_string()),
                impact_summary: Some(update.impact_summary(&self.order.order_no)),
            },
            format!("purchase_source_update:{}", purchase.base.id),
        )
        .map_err(Error::Logic)
    }
}

/// 一张参与覆盖的采购本次对账要做的事。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SyncAction {
    /// 新受影响：开立记录并派待办。
    Open,
    /// 仍受影响：刷新原因与待办摘要。
    Refresh,
    /// 草稿已不再受影响：等待重新提交审批，待办保持开放。
    AwaitSubmission,
    /// 不再受影响且已离开草稿：按事实判定处理方式并关闭待办。
    Close,
    /// 未受影响且没有开放记录。
    Skip,
}

/// 按受影响判定、开放记录与采购状态决定对账动作。
///
/// # 参数
/// * `affected` - 采购按当前事实是否受影响
/// * `has_open` - 采购是否有开放记录
/// * `status` - 采购当前状态
///
/// # 返回
/// 返回本次对账动作。
///
/// # 错误
/// 无。
fn sync_action(affected: bool, has_open: bool, status: PurchaseOrderStatus) -> SyncAction {
    match (affected, has_open) {
        (true, true) => SyncAction::Refresh,
        (true, false) => SyncAction::Open,
        (false, true) if status == PurchaseOrderStatus::Draft => SyncAction::AwaitSubmission,
        (false, true) => SyncAction::Close,
        (false, false) => SyncAction::Skip,
    }
}

/// 由采购当前事实推出不再受影响的处理方式。
///
/// # 参数
/// * `update` - 待关闭的开放记录
/// * `purchase` - 仍参与覆盖的采购；已作废时为空
/// * `current_revision_id` - 销售当前版本
///
/// # 返回
/// 返回处理方式及其形成的单据。
///
/// # 错误
/// 无。
fn derive_resolution(
    update: &PurchaseSourceUpdate,
    purchase: Option<&PurchaseOrder>,
    current_revision_id: &str,
) -> (PurchaseSourceResolution, Option<String>) {
    let Some(purchase) = purchase else {
        return (PurchaseSourceResolution::Voided, None);
    };
    let submitted = matches!(
        purchase.stable.status,
        PurchaseOrderStatus::PendingFinanceReview | PurchaseOrderStatus::InApproval
    );
    if submitted && update.awaiting_submission {
        return (PurchaseSourceResolution::Resubmitted, purchase.current_submission_id.clone());
    }
    if purchase.sales_order_revision_id.as_ref() != current_revision_id {
        return (PurchaseSourceResolution::NoLongerAffected, None);
    }
    if submitted {
        return (PurchaseSourceResolution::Resubmitted, purchase.current_submission_id.clone());
    }
    (PurchaseSourceResolution::Changed, purchase.stable.current_revision_id.clone())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_core::money::Quantity;
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrder, PurchaseOrderData, PurchaseOrderStatus,
        PurchaseSourceImpact, PurchaseSourceResolution, PurchaseSourceUpdate, PurchaseType, SourceImpactKind,
        SourceImpactReason, SourceUpdateOpening,
    };

    use super::{SyncAction, derive_resolution, sync_action};

    fn purchase(status: PurchaseOrderStatus, source: &str) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new("po-1"),
            PurchaseOrderData {
                business_org_unit_id: "org".into(),
                purchase_no: "PO-1".into(),
                sales_order_id: SalesOrderId::new("so-1"),
                sales_order_revision_id: SalesOrderRevisionId::new(source),
                creation_basis_id: "basis".into(),
                supplier_id: SupplierAccountId::new("sup-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer".into(),
                target_warehouse_id: Some(WarehouseId::new("wh-1")),
            },
            "buyer",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        order.stable.status = status;
        order.current_submission_id = Some("sub-2".into());
        order.stable.current_revision_id = Some("por-2".into());
        order
    }

    fn record(awaiting_submission: bool) -> PurchaseSourceUpdate {
        let order = purchase(PurchaseOrderStatus::Draft, "rev-1");
        let impact = PurchaseSourceImpact {
            purchase_order_id: "po-1".into(),
            reasons: vec![SourceImpactReason {
                sales_order_line_id: "sol-1".into(),
                line_no: 1,
                item_name: "礼盒".into(),
                kind: SourceImpactKind::Zeroed { baseline_quantity: Quantity::from_str("4").unwrap() },
            }],
        };
        let mut update = PurchaseSourceUpdate::open(
            "psu-1".into(),
            SourceUpdateOpening {
                order: &order,
                impact: &impact,
                sales_order_revision_id: "rev-2",
                trigger_sales_change_order_id: Some("sco-1"),
            },
        );
        if awaiting_submission {
            update.await_submission("rev-2").unwrap();
        }
        update
    }

    /// 作废、修改原单、采购变更与他单调减分别推出对应处理方式。
    #[test]
    fn resolution_is_derived_from_purchase_facts() {
        let open = record(false);
        assert_eq!(derive_resolution(&open, None, "rev-2"), (PurchaseSourceResolution::Voided, None));
        assert_eq!(
            derive_resolution(&open, Some(&purchase(PurchaseOrderStatus::InApproval, "rev-2")), "rev-2"),
            (PurchaseSourceResolution::Resubmitted, Some("sub-2".into()))
        );
        assert_eq!(
            derive_resolution(&open, Some(&purchase(PurchaseOrderStatus::Effective, "rev-2")), "rev-2"),
            (PurchaseSourceResolution::Changed, Some("por-2".into()))
        );
        assert_eq!(
            derive_resolution(&open, Some(&purchase(PurchaseOrderStatus::Effective, "rev-1")), "rev-2"),
            (PurchaseSourceResolution::NoLongerAffected, None)
        );
    }

    /// 受影响开立或刷新；不再受影响的草稿等待重新提交，其他状态关闭。
    #[test]
    fn sync_action_follows_impact_record_and_status() {
        use PurchaseOrderStatus::*;

        assert_eq!(sync_action(true, false, InApproval), SyncAction::Open);
        assert_eq!(sync_action(true, true, Draft), SyncAction::Refresh);
        assert_eq!(sync_action(false, true, Draft), SyncAction::AwaitSubmission);
        assert_eq!(sync_action(false, true, InApproval), SyncAction::Close);
        assert_eq!(sync_action(false, true, Effective), SyncAction::Close);
        assert_eq!(sync_action(false, false, Draft), SyncAction::Skip);
    }

    /// 等待重新提交的草稿提交审批后记为修改原单，即使仍引用原销售版本。
    #[test]
    fn awaiting_draft_submitted_is_resubmitted() {
        let awaiting = record(true);

        assert_eq!(
            derive_resolution(&awaiting, Some(&purchase(PurchaseOrderStatus::InApproval, "rev-1")), "rev-2"),
            (PurchaseSourceResolution::Resubmitted, Some("sub-2".into()))
        );
        assert_eq!(
            derive_resolution(
                &awaiting,
                Some(&purchase(PurchaseOrderStatus::PendingFinanceReview, "rev-2")),
                "rev-2"
            ),
            (PurchaseSourceResolution::Resubmitted, Some("sub-2".into()))
        );
    }
}
