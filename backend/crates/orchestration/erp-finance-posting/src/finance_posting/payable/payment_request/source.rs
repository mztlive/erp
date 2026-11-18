//! 逐采购证明申请资格，并冻结精确生效版本、销售来源与账户。

mod funding;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::PurchaseOrderRevisionId;
use erp_finance::entity::payable::{
    PayableAccount, PayableSourceType, PaymentRequestAccountSnapshot, PaymentRequestSubmission,
    PurchasePaymentRequestLine,
};
use erp_identity::AuthorizationService;
use erp_party::PartyExt;
use erp_party::repository::prelude::PartyBankAccountRepositoryExt;
use erp_procure_to_pay::procure_to_pay::sync_payment_arrangement;
use erp_procurement::entity::purchase_order::{PurchaseOrder, PurchaseOrderRevision, PurchaseOrderStatus};
use erp_procurement::repository::PurchaseOrderExt;
use erp_read_models::finance::payment_request::freeze_payment_request_source;
use erp_supplier::SupplierExt;
use erp_support::FileAssetExt;
use erp_support::authorization::files::require_owned;

use super::*;

/// 按付款申请动作已授权的采购来源冻结申请依据，不再要求采购提交权限。
/// # 参数
/// db、executor 为原事务；sources 为本次申请动作已授权对象，request、actor 为申请及操作人。
/// reserve 控制是否原子占用应付金额。
/// # 返回
/// 返回已校验当前生效版本、金额、付款主体和账户的完整申请依据。
/// # 错误
/// 来源不在授权结果中、业务事实变化、余额不足、附件或账户无效时拒绝。
pub(super) async fn prepare_submission(
    db: &Database,
    authorization: &AuthorizationService,
    sources: &[PurchaseOrder],
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    reserve: bool,
    executor: &mut dyn Executor,
) -> Result<PaymentRequestSubmission> {
    let mut lines = Vec::with_capacity(request.data.lines.len());
    let mut subject: Option<(String, String, String)> = None;
    for data in &request.data.lines {
        let account = db
            .payable_accounts()
            .find_by_id(&data.payable_account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购应付不存在".into()))?;
        let order = authorized_source(sources, &account.source_document_id)?;
        let revision = current_revision(db, order, executor).await?;
        validate_account(&account, order, &revision, request)?;

        let payer = &revision.payment_subject;
        if subject.as_ref().is_some_and(|(id, _, _)| id != payer.settlement_party_id.as_ref()) {
            return Err(Error::ValidationError("所选采购跨我方付款主体，请按主体分组分别申请".into()));
        }
        // 表头保留首笔所选采购的冻结名称；主体归组按身份，不统一改写各来源的历史名称。
        subject.get_or_insert((
            payer.settlement_party_id.to_string(),
            payer.settlement_party_name.clone(),
            order.business_org_unit_id.clone(),
        ));

        lines.push(freeze_line(db, &account, order, &revision, data.amount, executor).await?);
        if reserve
            && !db
                .payable_accounts()
                .reserve_payment_request(&account.base.id, &data.amount, actor.id(), executor)
                .await?
        {
            return Err(Error::ConflictError("采购可申请余额已变化，请刷新后重新核对".into()));
        }
    }

    if reserve {
        sync_reserved_arrangements(db, &lines, executor).await?;
    }

    validate_attachments(db, authorization, request, actor, executor).await?;
    let (payer_party_id, payer_party_name, org_unit_id) =
        subject.ok_or_else(|| Error::ValidationError("付款申请缺少采购来源".into()))?;
    let (payee_account, deposit_account) =
        funding::selected_accounts(db, request, &payer_party_id, reserve, executor).await?;
    ensure_company(db, &payer_party_id, executor).await?;

    Ok(PaymentRequestSubmission {
        supplier_id: request.data.supplier_id.clone(),
        payer_party_id,
        payer_party_name,
        org_unit_id: Some(org_unit_id),
        payee_account,
        deposit_account,
        lines,
    })
}

/// 只复用本次申请动作已授权的精确采购对象，不以其他来源或默认对象回退。
fn authorized_source<'a>(sources: &'a [PurchaseOrder], id: &str) -> Result<&'a PurchaseOrder> {
    sources
        .iter()
        .find(|order| order.base.id == id)
        .ok_or_else(|| Error::Forbidden("采购应付来源未通过付款申请授权".into()))
}

async fn sync_reserved_arrangements(
    db: &Database,
    lines: &[PurchasePaymentRequestLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    for line in lines {
        sync_payment_arrangement(db, &line.purchase_order_id, false, executor).await?;
    }
    Ok(())
}

async fn freeze_line(
    db: &Database,
    account: &PayableAccount,
    order: &PurchaseOrder,
    revision: &PurchaseOrderRevision,
    amount: Amount,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequestLine> {
    Ok(PurchasePaymentRequestLine {
        payable_account_id: account.base.id.clone(),
        purchase_order_id: order.base.id.clone(),
        purchase_revision_id: revision.base.id.clone(),
        requested_amount: amount,
        paid_amount: Amount::zero(),
        closed_amount: Amount::zero(),
        gross_total: account.gross_total,
        settled_total: account.settled_total,
        open_total: account.open_total,
        other_reserved_total: account.payment_request_reserved_total,
        payment_terms: serde_json::to_value(&revision.payment_term_snapshot)
            .map_err(|e| Error::Internal(e.to_string()))?,
        source_snapshot: freeze_payment_request_source(db, order, revision, executor).await?,
    })
}

fn validate_account(
    account: &erp_finance::entity::payable::PayableAccount,
    order: &PurchaseOrder,
    revision: &PurchaseOrderRevision,
    request: &PurchasePaymentRequest,
) -> Result<()> {
    if account.source_type != PayableSourceType::PurchaseOrder
        || account.supplier_id.as_ref() != request.data.supplier_id
        || order.supplier_id.as_ref() != request.data.supplier_id
        || account.gross_total != revision.gross_amount
        || account.purchase_revision_id.as_deref() != Some(revision.base.id.as_str())
        || account.settlement_party_id.as_deref()
            != Some(revision.payment_subject.settlement_party_id.as_ref())
        || account.settlement_party_name.as_deref()
            != Some(revision.payment_subject.settlement_party_name.as_str())
    {
        return Err(Error::ConflictError("采购供应商、正式金额与应付来源不一致".into()));
    }
    if account.open_total < Amount::zero() || account.payment_request_reserved_total > account.open_total {
        return Err(Error::ConflictError("采购应付余额异常，须先处理正式调整及未付占用".into()));
    }
    Ok(())
}

async fn current_revision(
    db: &Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderRevision> {
    if !matches!(
        order.stable.status,
        PurchaseOrderStatus::Effective
            | PurchaseOrderStatus::PartiallyExecuted
            | PurchaseOrderStatus::Completed
    ) {
        return Err(Error::ConflictError("只能对已生效采购发起付款申请".into()));
    }
    let revision_id = order
        .stable
        .current_revision_id
        .as_ref()
        .ok_or_else(|| Error::ConflictError("采购缺少可证明的生效版本".into()))?;
    db.purchase_order()
        .find_revisions_by_ids(&[PurchaseOrderRevisionId::new(revision_id)], executor)
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| Error::NotFound("采购生效版本不存在".into()))
}

async fn selected_account(
    db: &Database,
    request: &PurchasePaymentRequest,
    lock: bool,
    executor: &mut dyn Executor,
) -> Result<PaymentRequestAccountSnapshot> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(&request.data.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".into()))?;
    let accounts =
        db.party_bank_accounts().list_active_on(&supplier.party_id, BusinessDate::today(), executor).await?;
    let account = accounts
        .into_iter()
        .find(|a| a.base.id == request.data.payee_bank_account_id)
        .ok_or_else(|| Error::ConflictError("指定收款账户不属于供应商或已失效".into()))?;

    if lock
        && !db
            .party_bank_accounts()
            .lock_for_payment_request(&account.base.id, account.base.version, executor)
            .await?
    {
        return Err(Error::ConflictError("收款账户已变化，请重新核对并提交".into()));
    }
    Ok(PaymentRequestAccountSnapshot {
        account_id: account.base.id,
        version: account.base.version,
        supplier_party_id: supplier.party_id.to_string(),
        account_name: account.account_name,
        bank_name: account.bank_name,
        masked_account_number: format!("********{}", account.account_number_last4),
    })
}

async fn validate_attachments(
    db: &Database,
    authorization: &AuthorizationService,
    request: &PurchasePaymentRequest,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let ids = request.data.attachment_ids.iter().map(AsRef::as_ref).collect::<Vec<&str>>();
    require_owned(authorization, actor, &ids, executor).await?;

    for id in &request.data.attachment_ids {
        let asset = db
            .file_assets()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("申请附件不存在".into()))?;
        if !asset.is_usable_at(Instant::now()) {
            return Err(Error::Forbidden("申请附件已不可用".into()));
        }
    }
    Ok(())
}

/// 审批及付款重验冻结来源和账户；不读取可变销售草稿。
pub(super) async fn revalidate(
    db: &Database,
    request: &PurchasePaymentRequest,
    executor: &mut dyn Executor,
) -> Result<()> {
    funding::revalidate(db, request, executor).await?;
    ensure_company(db, &request.payer_party_id, executor).await?;

    for line in &request.lines {
        let order = db
            .purchase_orders()
            .find_by_id(&line.purchase_order_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("来源采购不存在".into()))?;
        let revision = current_revision(db, &order, executor).await?;
        if revision.base.id != line.purchase_revision_id
            || revision.payment_subject.settlement_party_id.as_ref() != request.payer_party_id
        {
            return Err(Error::ConflictError(
                "采购正式版本或付款主体已变化，请处理未付部分并重新申请".into(),
            ));
        }
        current_sales::revalidate(db, &order.base.id, &revision.payment_subject, request, executor).await?;

        let account = db
            .payable_accounts()
            .find_by_id(&line.payable_account_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("采购应付不存在".into()))?;
        if account.source_document_id != line.purchase_order_id
            || account.supplier_id.as_ref() != request.supplier_id
            || account.gross_total != line.gross_total
            || account.purchase_revision_id.as_deref() != Some(line.purchase_revision_id.as_str())
            || account.settlement_party_id.as_deref() != Some(request.payer_party_id.as_str())
            || account.settlement_party_name.as_deref()
                != Some(revision.payment_subject.settlement_party_name.as_str())
            || account.open_total < line.remaining()
            || account.payment_request_reserved_total > account.open_total
        {
            return Err(Error::ConflictError(
                "批准采购来源或可付余额已变化，请处理未付部分并重新申请".into(),
            ));
        }
    }
    Ok(())
}

async fn ensure_company(db: &Database, id: &str, executor: &mut dyn Executor) -> Result<()> {
    let party = db
        .parties()
        .find_by_id(id, executor)
        .await?
        .ok_or_else(|| Error::ConflictError("我方付款主体不存在或已失效".into()))?;
    if !party.is_active() || party.company_profile.is_none() {
        return Err(Error::ConflictError("采购付款主体必须为有效的我方公司".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{
        PurchaseOrderId, SalesOrderId, SalesOrderRevisionId, SupplierAccountId, WarehouseId,
    };
    use erp_procurement::entity::facts::PaymentTermFact;
    use erp_procurement::entity::purchase_order::{
        FulfillmentResponsibility, PurchaseOrderData, PurchaseType,
    };

    use super::*;

    fn purchase(id: &str) -> PurchaseOrder {
        let mut order = PurchaseOrder::new(
            PurchaseOrderId::new(id),
            PurchaseOrderData {
                business_org_unit_id: "procurement".into(),
                purchase_no: format!("PO-{id}"),
                sales_order_id: SalesOrderId::new("sales-1"),
                sales_order_revision_id: SalesOrderRevisionId::new("sales-revision-1"),
                creation_basis_id: format!("basis-{id}"),
                supplier_id: SupplierAccountId::new("supplier-1"),
                purchase_type: PurchaseType::Physical,
                payment_term_code: "POSTPAY_NET30".into(),
                fulfillment_responsibility: FulfillmentResponsibility::Warehouse,
                owner_user_id: "buyer-1".into(),
                target_warehouse_id: Some(WarehouseId::new("warehouse-1")),
            },
            "buyer-1",
            |_| Ok(PaymentTermFact::new("POSTPAY_NET30").with_days_after_delivery(30)),
        )
        .unwrap();
        order.stable.status = PurchaseOrderStatus::Effective;
        order.stable.current_revision_id = Some(format!("revision-{id}"));
        order
    }

    #[test]
    fn submission_reuses_the_exact_source_authorized_for_the_payment_action() {
        let sources = [purchase("purchase-2"), purchase("purchase-1")];
        let order = authorized_source(&sources, "purchase-1").unwrap();
        assert_eq!(order.base.id, "purchase-1");
        assert_eq!(order.owner_user_id.as_deref(), Some("buyer-1"));
        assert_eq!(order.stable.status, PurchaseOrderStatus::Effective);
        assert_eq!(order.stable.current_revision_id.as_deref(), Some("revision-purchase-1"));
    }

    #[test]
    fn submission_rejects_sources_outside_the_payment_authorization_result() {
        for sources in [Vec::new(), vec![purchase("purchase-2")]] {
            assert!(matches!(authorized_source(&sources, "purchase-1"), Err(Error::Forbidden(_))));
        }
    }
}
