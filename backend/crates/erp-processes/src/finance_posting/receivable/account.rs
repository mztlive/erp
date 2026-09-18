//! Receivable account creation coordinating finance, workflow tasks and audit.

use application_core::AuditActor;
use erp_audit::{AuditActorLogs, AuditExt};
use erp_core::common::time::Instant;
use erp_core::ids::{ReceivableAccountId, ReceivableEntryId, SalesOrderRevisionId};
use erp_finance::dto::receivable::CreateReceivableAccountRequest;
use erp_finance::entity::receivable::{
    AccountReviewStatus, EntryDirection, ReceivableAccount, ReceivableAccountData, ReceivableEntry,
    ReceivableEntryData, ReceivableEntryType,
};
use erp_finance::repository::ReceivableExt;
use erp_finance::service::receivable::mapping::zero_amount;
use erp_read_models::finance::dto::ReceivableAccountView;
use erp_sales::repository::SalesOrderExt;
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::ReceivableProcess;
use crate::{Error, Result};

impl ReceivableProcess {
    // -----------------------------------------------------------------------
    // 应收往来子账
    // -----------------------------------------------------------------------

    /// 建立应收往来子账与原始应收分录（跨集合事务写入）。
    ///
    /// 校验来源销售单存在（D13 Repository），同事务写入子账与分录，
    /// 保证「子账 + 原始应收」原子可见（数据模型 §6.8）。业务幂等唯一
    /// `(receivable_account_id, source_fact_type, source_document_id,
    /// source_revision_id, entry_type, source_sequence)` 由唯一索引保证，
    /// 重复提交落入 409。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建子账的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 来源销售单不存在
    /// * `ConflictError` - 业务唯一键重复（唯一索引透出）
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_receivable_account(
        &self,
        req: CreateReceivableAccountRequest,
        actor: &AuditActor,
    ) -> Result<ReceivableAccountView> {
        req.validate()?;
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&req.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let review_status = AccountReviewStatus::resolve_initial(
            req.review_status,
            sales_business_type_fact(sales_order.business_type),
        )
        .map_err(|error| Error::ValidationError(error.to_string()))?;
        if sales_order.business_type == erp_sales::entity::sales_order::BusinessType::Voucher
            && sales_order.stable.current_revision_id.as_deref()
                != Some(req.source_sales_order_revision_id.as_str())
        {
            return Err(Error::ConflictError("卡券应收必须绑定来源销售单的当前正式版本".to_string()));
        }
        let account_id = ReceivableAccountId::new(next_id());
        let entry_id = ReceivableEntryId::new(next_id());
        let posted_at = Instant::now();
        let account = ReceivableAccount::new(
            account_id.clone(),
            ReceivableAccountData {
                sales_order_id: req.sales_order_id.clone().into(),
                account_seq: req.account_seq,
                customer_id: req.customer_id.clone(),
                counterparty_party_id: req.counterparty_party_id.clone(),
                source_sales_order_revision_id: SalesOrderRevisionId::new(
                    &req.source_sales_order_revision_id,
                ),
                review_status,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: req.gross_total,
                settled_total: zero_amount(),
                invoiceable_total: req.invoiceable_total.unwrap_or(req.gross_total),
                invoiced_total: zero_amount(),
            },
            actor.id(),
        )?;
        let entry = ReceivableEntry::new(
            entry_id,
            ReceivableEntryData {
                receivable_account_id: account_id.clone(),
                entry_type: ReceivableEntryType::Original,
                direction: EntryDirection::Increase,
                amount: account.gross_total,
                due_date: req.due_date,
                source_fact_type: "sales_order".to_string(),
                source_document_id: req.sales_order_id.clone(),
                source_revision_id: req.source_sales_order_revision_id,
                source_sequence: req.source_sequence,
                posted_at,
            },
        )?;
        let audit = actor.clone().resource_log(
            "receivable_account.create",
            "receivable_account",
            account_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    db.receivable().create_receivable_with_entry(&account, &entry, executor).await?;
                    db.audit_logs().create(&audit, executor).await?;
                    Ok::<(), crate::Error>(())
                })
            })
            .await?;

        self.read.receivable_account_detail(&account_id).await.map_err(crate::Error::from)
    }
}

/// Extract the stable sales business-kind fact consumed by finance.
///
/// All persisted source variants map explicitly; no fallback changes the review policy.
fn sales_business_type_fact(
    value: erp_sales::entity::sales_order::BusinessType,
) -> erp_finance::entity::receivable::SalesBusinessTypeFact {
    match value {
        erp_sales::entity::sales_order::BusinessType::GoodsService => {
            erp_finance::entity::receivable::SalesBusinessTypeFact::GoodsService
        },
        erp_sales::entity::sales_order::BusinessType::Voucher => {
            erp_finance::entity::receivable::SalesBusinessTypeFact::Voucher
        },
    }
}
