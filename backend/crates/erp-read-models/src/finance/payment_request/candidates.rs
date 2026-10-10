//! 申请候选读取显式正式来源和余额，不预填供应商总付款金额。

use std::collections::HashMap;

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{PurchaseOrderId, PurchaseOrderRevisionId, SupplierAccountId};
use erp_core::money::Amount;
use erp_finance::entity::payable::PayableAccount;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_party::repository::prelude::*;
use erp_party::{PartyBankAccountView, PartyExt};
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevision, PurchaseOrderStatus};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_procurement::repository::purchase_order::PurchaseOrderFilter;
use erp_sales::repository::SalesOrderExt;
use erp_supplier::SupplierExt;
use persistence_core::{Executor, Transactional};

use super::{PurchasePaymentRequestCandidate, PurchasePaymentRequestReadService};
use crate::{Error, Result};

impl PurchasePaymentRequestReadService {
    /// 返回当前账号具备申请资格的采购与供应商有效脱敏账户候选。
    ///
    /// # 参数
    /// * `actor` / `supplier_id` - 当前账号与可选供应商过滤。
    /// # 返回
    /// 返回已生效、有正可申请额度且主体可证明的候选。
    /// # 错误
    /// 动作拒绝、查询超限、主体或仓储完整性错误时拒绝。
    pub async fn candidates(
        &self,
        actor: &AuditActor,
        supplier_id: Option<&str>,
    ) -> Result<Vec<PurchasePaymentRequestCandidate>> {
        let this = self.clone();
        let actor = actor.clone();
        let supplier_id = supplier_id.map(str::to_owned);
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(
                    async move { this.candidate_snapshot(&actor, supplier_id.as_deref(), executor).await },
                )
            })
            .await
    }

    async fn candidate_snapshot(
        &self,
        actor: &AuditActor,
        supplier_id: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PurchasePaymentRequestCandidate>> {
        let scope = self.source_scope(actor, "create", executor).await?;
        let filter = PurchaseOrderFilter {
            supplier_id: supplier_id.map(SupplierAccountId::new),
            ..Default::default()
        };
        let versions = self.db.purchase_orders().query_versions(&filter, &scope, executor).await?;
        if versions.len() > 10_000 {
            return Err(Error::ValidationError("付款申请候选超过上限，请选择供应商".into()));
        }
        let ids = versions.into_iter().map(|row| row.id).collect::<Vec<_>>();
        let orders = self.db.purchase_orders().list_active_by_ids(&ids, executor).await?;
        let accounts = self
            .db
            .payable_accounts()
            .find_by_purchase_orders(&ids.iter().map(PurchaseOrderId::new).collect::<Vec<_>>(), executor)
            .await?
            .into_iter()
            .map(|row| (row.source_document_id.clone(), row))
            .collect::<HashMap<_, _>>();
        let mut result = Vec::new();
        for order in orders {
            if !matches!(
                order.stable.status,
                PurchaseOrderStatus::Effective
                    | PurchaseOrderStatus::PartiallyExecuted
                    | PurchaseOrderStatus::Completed
            ) {
                continue;
            }
            let Some(account) = accounts.get(&order.base.id) else { continue };
            if account.open_total.checked_sub(account.payment_request_reserved_total) <= Amount::zero() {
                continue;
            }
            let Some(revision_id) = &order.stable.current_revision_id else { continue };
            let revision = self
                .db
                .purchase_order_revisions()
                .find_by_id(revision_id, executor)
                .await?
                .ok_or_else(|| Error::ConflictError("付款申请候选采购版本不存在".into()))?;
            if !self.candidate_subject_valid(&order, &revision, account, executor).await? {
                continue;
            }
            result.push(self.candidate_view(&order, &revision, account, executor).await?);
        }
        Ok(result)
    }

    async fn candidate_subject_valid(
        &self,
        order: &PurchaseOrder,
        revision: &PurchaseOrderRevision,
        account: &PayableAccount,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let subject = &revision.payment_subject;
        let sales = self
            .db
            .sales_order_revisions()
            .find_by_id(subject.sales_order_revision_id.as_ref(), executor)
            .await?;
        let current = self.db.sales_orders().find_by_id(subject.sales_order_id.as_ref(), executor).await?;
        if current.is_none_or(|current| {
            current.stable.current_revision_id.as_deref() != Some(subject.sales_order_revision_id.as_ref())
        }) {
            return Ok(false);
        }
        Ok(sales.is_some_and(|sales| {
            sales.sales_order_id == order.sales_order_id
                && sales.sales_order_id == subject.sales_order_id
                && sales.settlement_party_id == subject.settlement_party_id
                && sales
                    .settlement_party_snapshot
                    .as_ref()
                    .is_some_and(|name| name.settlement_party_name == subject.settlement_party_name)
                && revision.purchase_order_id.as_ref() == order.base.id
                && account.settlement_party_id.as_deref() == Some(subject.settlement_party_id.as_ref())
                && account.purchase_revision_id.as_deref() == Some(revision.base.id.as_str())
        }))
    }

    async fn candidate_view(
        &self,
        order: &PurchaseOrder,
        revision: &PurchaseOrderRevision,
        account: &PayableAccount,
        executor: &mut dyn Executor,
    ) -> Result<PurchasePaymentRequestCandidate> {
        let bank_accounts = self.candidate_accounts(order, executor).await?;
        let lines = self
            .db
            .purchase_order()
            .list_revision_lines(&PurchaseOrderRevisionId::new(&revision.base.id), executor)
            .await?;
        let purchase_summary = lines
            .iter()
            .take(3)
            .filter_map(|line| line.product_name_snapshot.as_deref())
            .collect::<Vec<_>>()
            .join("、");
        let prepayment_gap = revision
            .payment_term_snapshot
            .required_prepayment(account.gross_total)?
            .map(|required| required.checked_sub(account.settled_total).max(Amount::zero()));
        Ok(PurchasePaymentRequestCandidate {
            purchase_summary,
            prepayment_gap,
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            purchase_revision_id: revision.base.id.clone(),
            supplier_id: order.supplier_id.to_string(),
            supplier_name: revision.supplier_snapshot.supplier_name.clone(),
            payer_party_id: revision.payment_subject.settlement_party_id.to_string(),
            payer_party_name: revision.payment_subject.settlement_party_name.clone(),
            payable_account_id: account.base.id.clone(),
            gross_total: account.gross_total,
            settled_total: account.settled_total,
            open_total: account.open_total,
            reserved_total: account.payment_request_reserved_total,
            available_total: account.open_total.checked_sub(account.payment_request_reserved_total),
            payment_terms: serde_json::to_value(&revision.payment_term_snapshot)
                .map_err(|error| Error::Internal(error.to_string()))?,
            bank_accounts,
        })
    }
    async fn candidate_accounts(
        &self,
        order: &PurchaseOrder,
        executor: &mut dyn Executor,
    ) -> Result<Vec<PartyBankAccountView>> {
        let supplier = self
            .db
            .supplier_accounts()
            .find_by_id(order.supplier_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::ConflictError("付款申请供应商不存在".into()))?;
        Ok(self
            .db
            .party_bank_accounts()
            .list_current_on(&supplier.party_id, BusinessDate::today(), executor)
            .await?
            .into_iter()
            .map(PartyBankAccountView::from)
            .collect())
    }
}
