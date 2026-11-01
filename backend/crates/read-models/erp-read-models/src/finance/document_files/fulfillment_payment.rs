//! 履约任务仅获得其采购单的付款金额与申请进度，不扩展整份合并申请的读取资格。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::PurchaseOrderId;
use erp_core::money::Amount;
use erp_finance::dto::payable::payment_request::PurchasePaymentRequestQuery;
use erp_finance::entity::payable::{PurchasePaymentRequest, PurchasePaymentRequestStatus};
use erp_finance::repository::prelude::*;
use erp_finance::repository::{PayableExt, PurchasePaymentRequestExt};
use erp_procurement::entity::purchase_order::PurchaseOrderRevision;
use erp_procurement::repository::PurchaseOrderExt;
use erp_supplier::{SupplierPaymentTerm, split_encoded_payment_term_snapshot};
use erp_workflow::WorkItemType;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use super::BusinessFinanceFiles;
use crate::ports::work_item_authorization::WorkItemAuthorizationReadPort;
use crate::{Error, Result};

/// 采购整单金额；与销售金额、当前发货批次数量无关。
#[derive(Debug, Serialize)]
pub struct FulfillmentPaymentView {
    pub gross_amount: Amount,
    pub paid_amount: Amount,
    pub open_amount: Amount,
    pub payment_term_label: String,
    pub requests: Vec<FulfillmentRequestView>,
}

/// 合并付款申请只返回当前采购来源行，不含其他来源、银行账号或申请总额。
#[derive(Debug, Serialize)]
pub struct FulfillmentRequestView {
    pub request_id: String,
    pub request_no: String,
    pub status: PurchasePaymentRequestStatus,
    pub requested_amount: Amount,
    pub paid_amount: Amount,
    pub remaining_amount: Amount,
    pub closed_amount: Amount,
    pub desired_payment_date: BusinessDate,
}

impl BusinessFinanceFiles {
    /// 在读取前后核验任务资格，按任务真实采购来源读取付款摘要。
    /// # 参数
    /// 当前账号、任务身份及工作流授权端口。
    /// # 返回
    /// 当前采购单的应付余额、冻结付款条件与逐申请金额。
    /// # 错误
    /// 无任务资格、非采购履约、缺少正式来源或读取期间资格变化时拒绝。
    pub async fn fulfillment_payment(
        &self,
        actor: &AuditActor,
        task_id: &str,
        auth: &impl WorkItemAuthorizationReadPort,
    ) -> Result<FulfillmentPaymentView> {
        let fact = auth.authorize(task_id, actor).await?;
        if fact.work_item_type != WorkItemType::FulfillmentOperation {
            return Err(Error::NotFound("采购履约任务不存在或无权查看".into()));
        }
        let expected = fact.clone();
        let this = self.clone();
        let view = self
            .db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let purchase_id = this
                        .purchase_source(
                            &expected.business_object_type,
                            &expected.business_object_id,
                            executor,
                        )
                        .await?;
                    this.purchase_payment(&purchase_id, executor).await
                })
            })
            .await?;
        if auth.authorize(task_id, actor).await? != fact {
            return Err(Error::ConflictError("履约任务资格已变化，请刷新后重试".into()));
        }
        Ok(view)
    }

    async fn purchase_payment(
        &self,
        purchase_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<FulfillmentPaymentView> {
        let account = self
            .db
            .payable_accounts()
            .find_by_purchase_order(purchase_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购应付信息暂不可用".into()))?;
        let revision = self.payment_revision(purchase_id, executor).await?;
        let code = split_encoded_payment_term_snapshot(&revision.payment_term_snapshot.payment_term_code)
            .payment_term_code;
        let payment_term_label = SupplierPaymentTerm::parse(&code)
            .map(SupplierPaymentTerm::label)
            .unwrap_or_else(|_| "付款条件待核对".into());
        let requests = self.payment_requests(purchase_id, &account.base.id, executor).await?;
        Ok(FulfillmentPaymentView {
            gross_amount: revision.gross_amount,
            paid_amount: account.settled_total,
            open_amount: account.open_total,
            payment_term_label,
            requests,
        })
    }

    async fn payment_revision(
        &self,
        purchase_id: &PurchaseOrderId,
        executor: &mut dyn Executor,
    ) -> Result<PurchaseOrderRevision> {
        let order = self
            .db
            .purchase_orders()
            .find_by_id(purchase_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".into()))?;
        let revision_id = order
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::ConflictError("采购付款条件暂不可用".into()))?;
        let revision = self
            .db
            .purchase_order_revisions()
            .find_by_id(revision_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购生效版本不存在".into()))?;
        if &revision.purchase_order_id != purchase_id {
            return Err(Error::ConflictError("采购付款条件来源不匹配".into()));
        }
        Ok(revision)
    }

    async fn payment_requests(
        &self,
        purchase_id: &PurchaseOrderId,
        account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FulfillmentRequestView>> {
        let candidates = self
            .db
            .purchase_payment_requests()
            .scope_candidates(
                &PurchasePaymentRequestQuery {
                    payable_account_id: Some(account_id.into()),
                    ..Default::default()
                },
                executor,
            )
            .await?;
        if candidates.len() > 10_000 {
            return Err(Error::ValidationError("本采购单付款申请过多，请联系管理员核对".into()));
        }
        Ok(candidates
            .iter()
            .filter_map(|request| FulfillmentRequestView::for_purchase(request, purchase_id, account_id))
            .collect())
    }
}

#[cfg(test)]
mod tests;

impl FulfillmentRequestView {
    /// 仅提取精确采购及应付身份匹配的申请分配。
    /// # 参数
    /// 关联申请、已授权采购身份及其应付子账身份。
    /// # 返回
    /// 匹配时返回本采购行的金额与申请进度，否则为空。
    /// # 错误
    /// 无；不返回其他采购的金额、附件或账户资料。
    pub(crate) fn for_purchase(
        request: &PurchasePaymentRequest,
        purchase_id: &PurchaseOrderId,
        account_id: &str,
    ) -> Option<Self> {
        let line = request.lines.iter().find(|line| {
            line.purchase_order_id == purchase_id.as_ref() && line.payable_account_id == account_id
        })?;
        Some(Self {
            request_id: request.base.id.clone(),
            request_no: request.request_no.clone(),
            status: request.status,
            requested_amount: line.requested_amount,
            paid_amount: line.paid_amount,
            remaining_amount: line.remaining(),
            closed_amount: line.closed_amount,
            desired_payment_date: request.data.desired_payment_date,
        })
    }
}
