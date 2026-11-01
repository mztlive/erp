//! 付款安排读取同一事务内的采购、应付和开放待办。
use std::sync::Arc;

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::PurchaseOrderId;
use erp_core::money::Amount;
use erp_finance::dto::payable::payment_request::PurchasePaymentRequestQuery;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use erp_procurement::entity::payment_arrangement::PaymentArrangementState;
use erp_procurement::entity::purchase_order::PurchaseOrder;
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::purchase_source_update::find_open_for_purchase;
use erp_supplier::{SupplierPaymentTerm, split_encoded_payment_term_snapshot};
use erp_workflow::repository::prelude::*;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::PurchaseOrderReadService;
use crate::finance::document_files::FulfillmentRequestView;
use crate::{Error, Result};

/// 供写用例在原事务内同步待办的权威事实；不提供独立授权。
pub struct PaymentArrangementFacts {
    pub order: PurchaseOrder,
    pub account: Option<PayableAccount>,
    pub task: Option<WorkItem>,
    pub state: PaymentArrangementState,
    pub payment_blocked: bool,
}
/// 采购详情和工作台共用付款安排视图。
#[derive(Serialize)]
pub struct PaymentArrangementView {
    pub state: PaymentArrangementState,
    pub payment_blocked: bool,
    pub available_amount: Amount,
    pub reserved_amount: Amount,
    pub paid_amount: Amount,
    pub payment_term_label: String,
    pub prepayment_gap: Option<Amount>,
    pub task_id: Option<String>,
    pub task_version: Option<u64>,
    pub follow_up_at: Option<Instant>,
    pub can_schedule: bool,
    pub requests: Vec<FulfillmentRequestView>,
}
/// 读取一张已确定采购的安排事实，不扫描无关采购或待办。
/// # 参数
/// 数据库、采购身份及调用方事务执行器。
/// # 返回
/// 采购、可选应付和开放安排任务。
/// # 错误
/// 采购不存在、重复开放责任或仓储读取失败时返回错误。
pub async fn arrangement_facts(
    db: &Database,
    id: &str,
    executor: &mut dyn Executor,
) -> Result<PaymentArrangementFacts> {
    let order = db
        .purchase_orders()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".into()))?;
    let account = db.payable_accounts().find_by_purchase_order(&PurchaseOrderId::new(id), executor).await?;
    let tasks = db.work_items().list_active_by_object("purchase_order", id, executor).await?;
    let mut tasks =
        tasks.into_iter().filter(|item| item.work_item_type == WorkItemType::PurchasePaymentArrangement);
    let task = tasks.next();
    if tasks.next().is_some() {
        return Err(Error::ConflictError("采购付款安排责任重复，请联系管理员".into()));
    }
    let blocked = find_open_for_purchase(db, id, executor).await?.is_some_and(|value| value.blocks_payment);
    let state = account.as_ref().map_or(PaymentArrangementState::Inactive, |account| {
        PaymentArrangementState::resolve(
            order.stable.status,
            account.open_total,
            account.payment_request_reserved_total,
            blocked,
        )
    });
    Ok(PaymentArrangementFacts { order, account, task, state, payment_blocked: blocked })
}
impl PurchaseOrderReadService {
    /// 读取当前账号有权查看的采购付款安排。
    /// # 参数
    /// 已认证账号与采购单身份。
    /// # 返回
    /// 仅本采购金额和本采购在合并申请内的分配，不授予其他来源访问权。
    /// # 错误
    /// 无采购详情资格、来源缺失或读取失败时拒绝。
    pub async fn payment_arrangement(&self, actor: &AuditActor, id: &str) -> Result<PaymentArrangementView> {
        let this = Self::with_scope(self.db.clone(), Arc::clone(&self.data_scope));
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.access().resolve_detail(&actor, &id, executor).await?;
                    let facts = arrangement_facts(&this.db, &id, executor).await?;
                    arrangement_view(&this.db, facts, actor.id(), executor).await
                })
            })
            .await
    }
}
/// 仅从已授权采购及其精确关联组装展示。
async fn arrangement_view(
    db: &Database,
    facts: PaymentArrangementFacts,
    actor: &str,
    executor: &mut dyn Executor,
) -> Result<PaymentArrangementView> {
    let mut view = base_view(&facts, actor);
    let Some(account) = facts.account else {
        return Ok(view);
    };
    view.available_amount =
        account.open_total.checked_sub(account.payment_request_reserved_total).max(Amount::zero());
    view.reserved_amount = account.payment_request_reserved_total;
    view.paid_amount = account.settled_total;
    fill_terms(db, &facts.order, &account, &mut view, executor).await?;
    let requests = db
        .purchase_payment_requests()
        .scope_candidates(
            &PurchasePaymentRequestQuery {
                payable_account_id: Some(account.base.id.clone()),
                ..Default::default()
            },
            executor,
        )
        .await?;
    if requests.len() > 10_000 {
        return Err(Error::ValidationError("本采购单付款申请过多，请联系管理员核对".into()));
    }
    view.requests = requests
        .iter()
        .filter_map(|request| {
            FulfillmentRequestView::for_purchase(
                request,
                &PurchaseOrderId::new(&facts.order.base.id),
                &account.base.id,
            )
        })
        .collect();
    Ok(view)
}

/// 任务身份和余额展示使用同一份读取事实。
fn base_view(facts: &PaymentArrangementFacts, actor: &str) -> PaymentArrangementView {
    PaymentArrangementView {
        state: facts.state,
        payment_blocked: facts.payment_blocked,
        available_amount: Amount::zero(),
        reserved_amount: Amount::zero(),
        paid_amount: Amount::zero(),
        payment_term_label: String::new(),
        prepayment_gap: None,
        task_id: facts.task.as_ref().map(|item| item.base.id.clone()),
        task_version: facts.task.as_ref().map(|item| item.base.version),
        follow_up_at: facts.task.as_ref().and_then(|item| item.due_at),
        can_schedule: facts.task.as_ref().is_some_and(|item| item.owner_user_id.as_deref() == Some(actor)),
        requests: Vec::new(),
    }
}

/// 加载当前生效版本的付款条件及履约前实付缺口。
async fn fill_terms(
    db: &Database,
    order: &PurchaseOrder,
    account: &PayableAccount,
    view: &mut PaymentArrangementView,
    executor: &mut dyn Executor,
) -> Result<()> {
    if let Some(id) = &order.stable.current_revision_id {
        let revision = db
            .purchase_order_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::ConflictError("采购生效版本不存在，请刷新后重试".into()))?;
        let code = split_encoded_payment_term_snapshot(&revision.payment_term_snapshot.payment_term_code)
            .payment_term_code;
        view.payment_term_label = SupplierPaymentTerm::parse(&code)
            .map(SupplierPaymentTerm::label)
            .unwrap_or_else(|_| "付款条件待核对".into());
        view.prepayment_gap = revision
            .payment_term_snapshot
            .required_prepayment(account.gross_total)?
            .map(|required| required.checked_sub(account.settled_total).max(Amount::zero()));
    }
    Ok(())
}
