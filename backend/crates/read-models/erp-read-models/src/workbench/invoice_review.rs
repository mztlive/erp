//! 实例授权的开票核对资料；当前票款与不可变提交材料分开读取。
use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use erp_finance::dto::receivable::invoice_review::InvoiceReviewAmounts;
use erp_finance::entity::receivable::SalesInvoiceRequest;
use erp_finance::entity::receivable::invoice_request_source::InvoiceRequestSource;
use erp_finance::repository::ReceivableExt;
use erp_finance::repository::prelude::*;
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_workflow::WorkflowAuthorizationPort;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use erp_workflow::entity::approval_integration::{ApprovalMaterialFile, ApprovalSubjectSnapshot};
use erp_workflow::service::approval::execution::ApprovalRuntimeService;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;
use mongodb::Database;
use persistence_core::{Executor, Transactional};
use serde::Serialize;

use crate::finance::receivable::invoice_request_source;
use crate::sales_center::materials::revision_materials;
use crate::{Error, Result};

pub(crate) mod materials;
mod records;
mod source;
use records::InvoiceReviewRecords;

/// 本次审批唯一应收的当前核对结果，不包含客户其他往来。
#[derive(Debug, Serialize)]
pub struct InvoiceApprovalReview {
    pub read_at: Instant,
    pub account_label: String,
    pub gross_total: Amount,
    pub invoiceable_total: Amount,
    pub invoiced_total: Amount,
    pub settled_total: Amount,
    pub open_total: Amount,
    pub amounts: InvoiceReviewAmounts,
    pub source: ApprovalRelatedSalesSnapshot,
    pub sales_status: String,
    pub fulfillment_status: String,
    pub sales_owner: Option<String>,
    pub warnings: Vec<String>,
    pub requests: Vec<RelatedInvoiceRequest>,
    pub request_count: usize,
    pub records: InvoiceReviewRecords,
}

/// 同一应收最近申请的必要核对字段；不返回任意单据读取入口。
#[derive(Debug, Serialize)]
pub struct RelatedInvoiceRequest {
    pub request_no: String,
    pub is_current: bool,
    pub amount: Amount,
    pub invoiced_amount: Amount,
    pub status: String,
    pub created_at: u64,
}

/// 先执行审批实例授权，再读取唯一来源当前票款；不授予台账读取资格。
/// # 参数
/// 数据库、审批运行服务、当前账号及审批实例身份。
/// # 返回
/// 同一事务快照下的额度、回款、销售来源与最近申请。
/// # 错误
/// 未授权、非开票审批、申请已重提或来源不一致时拒绝。
pub async fn invoice_approval_review<A: WorkflowAuthorizationPort>(
    db: &Database,
    runtime: &ApprovalRuntimeService<A>,
    actor: &AuditActor,
    instance_id: &str,
) -> Result<InvoiceApprovalReview> {
    let materials = runtime.materials(actor, instance_id).await?;
    if materials.document_type != "sales_invoice_request" {
        return Err(Error::ValidationError("当前审批不是开票申请".into()));
    }
    let database = db.clone();
    let result = db
        .client()
        .clone()
        .with_transaction(move |executor| {
            Box::pin(async move { current(&database, &materials, executor).await })
        })
        .await?;
    // 响应前重验实例资格，避免读取期间责任资格失效。
    runtime.materials(actor, instance_id).await?;
    Ok(result)
}

async fn current(
    db: &Database,
    materials: &ApprovalMaterialsView,
    executor: &mut dyn Executor,
) -> Result<InvoiceApprovalReview> {
    let request = request(db, &materials.document_id, materials.subject_version, executor).await?;
    if materials.display.root_document_id != request.sales_order_id.as_ref() {
        return Err(Error::ConflictError("本次审批与销售来源不一致".into()));
    }
    let account = db
        .receivable_accounts()
        .find_by_id(request.receivable_account_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("申请绑定的应收不存在".into()))?;
    source::validate_binding(&request, &account)?;
    let reserved = db.sales_invoice_requests().reserved_for_account(&account.base.id, executor).await?;
    for row in &reserved {
        source::validate_binding(row, &account)?;
    }
    let amounts =
        InvoiceReviewAmounts::from_requests(account.open_invoiceable_total, &request.base.id, &reserved);
    let (order, _, source) = source::load(db, &request, executor).await?;
    let identity = invoice_request_source::load(db, &account, executor).await?;
    let warnings = warnings(&request, &materials.display.source_sales, &source, &amounts, &identity);
    let sales_owner = owner_name(db, &order.sales_owner_user_id, executor).await?;
    let (requests, request_count) = related_requests(&request, &reserved);
    Ok(InvoiceApprovalReview {
        read_at: Instant::now(),
        account_label: format!("{} · 第 {} 笔应收", order.order_no, account.account_seq),
        gross_total: account.gross_total,
        invoiceable_total: account.invoiceable_total,
        invoiced_total: account.invoiced_total,
        settled_total: account.settled_total,
        open_total: account.open_total,
        amounts,
        source,
        sales_status: order.commercial_status.label().into(),
        fulfillment_status: order.fulfillment_progress.label().into(),
        sales_owner,
        warnings,
        requests,
        request_count,
        records: records::load(db, &account, executor).await?,
    })
}

async fn owner_name(db: &Database, user_id: &str, executor: &mut dyn Executor) -> Result<Option<String>> {
    Ok(db
        .accounts()
        .list_work_item_party_accounts(&[user_id.to_owned()], executor)
        .await?
        .into_iter()
        .next()
        .map(|row| row.name))
}

/// 仅返回真实占用额度的申请，未提交草稿不构成审批依据。
fn related_requests(
    request: &SalesInvoiceRequest,
    reserved: &[SalesInvoiceRequest],
) -> (Vec<RelatedInvoiceRequest>, usize) {
    let mut rows = reserved.iter().collect::<Vec<_>>();
    rows.sort_by(|left, right| {
        right.base.created_at.cmp(&left.base.created_at).then_with(|| right.base.id.cmp(&left.base.id))
    });
    let visible = rows
        .into_iter()
        .take(10)
        .map(|row| RelatedInvoiceRequest {
            request_no: row.request_no.clone(),
            is_current: row.base.id == request.base.id,
            amount: row.data.amount,
            invoiced_amount: row.invoiced_amount,
            status: row.status.label().into(),
            created_at: row.base.created_at,
        })
        .collect();
    (visible, reserved.len())
}

async fn request(
    db: &Database,
    id: &str,
    version: u32,
    executor: &mut dyn Executor,
) -> Result<SalesInvoiceRequest> {
    db.sales_invoice_requests()
        .find_by_id(id, executor)
        .await?
        .filter(|row| row.approval_subject_version == version)
        .ok_or_else(|| Error::ConflictError("申请不存在或已重新提交，请刷新工作台".into()))
}

fn warnings(
    request: &SalesInvoiceRequest,
    frozen: &[ApprovalRelatedSalesSnapshot],
    source: &ApprovalRelatedSalesSnapshot,
    amounts: &InvoiceReviewAmounts,
    identity: &InvoiceRequestSource,
) -> Vec<String> {
    let mut warnings = Vec::new();
    if !identity.effective {
        warnings.push("来源销售单当前不满足开票来源条件，请核对后处理。".into());
    }
    if identity.invoice_title != request.data.invoice_title || identity.tax_number != request.data.tax_number
    {
        warnings.push("提交的开票抬头或税号与当前结算主体档案不一致，请核对资料变更。".into());
    }
    if frozen.iter().any(|row| row.revision_id != source.revision_id) {
        warnings.push("申请提交后销售版本已变更，请对照提交资料与当前销售依据。".into());
    }
    if amounts.shortage > Amount::zero() {
        warnings
            .push(format!("当前开票占用超出未开票额度 {} 元，请核对销售调整与其他申请。", amounts.shortage));
    }
    warnings
}

/// 在提交事务中锁定销售版本及其允许附件，不补写旧审批。
/// # 参数
/// 数据库、本次开票审批快照及原提交事务。
/// # 返回
/// 唯一来源销售快照与该版本的材料清单。
/// # 错误
/// 申请版本或来源不匹配时拒绝；材料不可用时明确记录，基础设施错误继续传播。
pub(super) async fn capture(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<(ApprovalRelatedSalesSnapshot, Vec<ApprovalMaterialFile>)> {
    let request = request(db, &snapshot.business_object_id, snapshot.subject_version, executor).await?;
    let account = db
        .receivable_accounts()
        .find_by_id(request.receivable_account_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("开票申请应收不存在".into()))?;
    source::validate_binding(&request, &account)?;
    let (_, revision, mut source) = source::load(db, &request, executor).await?;
    super::brief::push_section(
        &mut source.source.extra_sections,
        "提交时本笔应收",
        Some(&super::presentation::format_yuan(&account.gross_total)),
        true,
    );
    let files = match revision_materials(db, &revision, executor).await {
        Ok(materials) => materials.files,
        Err(error) => {
            super::approval_materials::related_sales::material_unavailable(error)?;
            super::brief::push_section(
                &mut source.source.extra_sections,
                "合同或凭证",
                Some("暂不可读取，请联系销售核对资料后再审批"),
                false,
            );
            Vec::new()
        },
    };
    source.validate()?;
    Ok((source, files))
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{
        CustomerAccountId, PartyId, ReceivableAccountId, SalesInvoiceRequestId, SalesOrderId,
        SalesOrderRevisionId,
    };
    use erp_finance::entity::receivable::{
        AccountReviewStatus, InvoiceRequestData, ReceivableAccount, ReceivableAccountData,
    };
    use erp_workflow::entity::approval_integration::display_snapshot::ApprovalBriefSource;

    use super::*;

    fn fixtures() -> (SalesInvoiceRequest, ReceivableAccount) {
        let mut account = ReceivableAccount::new(
            ReceivableAccountId::new("account"),
            ReceivableAccountData {
                sales_order_id: SalesOrderId::new("sales"),
                account_seq: 1,
                customer_id: CustomerAccountId::new("customer"),
                counterparty_party_id: PartyId::new("party"),
                source_sales_order_revision_id: SalesOrderRevisionId::new("revision"),
                review_status: AccountReviewStatus::NotApplicable,
                reviewed_by: None,
                reviewed_at: None,
                review_evidence_reference: None,
                gross_total: "300".parse().unwrap(),
                settled_total: Amount::zero(),
                invoiceable_total: "300".parse().unwrap(),
                invoiced_total: Amount::zero(),
            },
            "actor",
        )
        .unwrap();
        account.base = BaseModel { id: "account".into(), ..BaseModel::fake() };
        let mut request = SalesInvoiceRequest::new(
            SalesInvoiceRequestId::new("request"),
            &account,
            InvoiceRequestData {
                amount: "165".parse().unwrap(),
                invoice_title: "购方公司".into(),
                tax_number: "TAX".into(),
                invoice_content: "商品".into(),
                reason: "按合同开票".into(),
            },
            "actor",
        )
        .unwrap();
        request.base = BaseModel { id: "request".into(), ..BaseModel::fake() };
        request.submit("300".parse().unwrap()).unwrap();
        (request, account)
    }

    fn source() -> ApprovalRelatedSalesSnapshot {
        ApprovalRelatedSalesSnapshot {
            document_id: "sales".into(),
            document_no: "XS-001".into(),
            revision_id: "revision".into(),
            revision_no: 1,
            source: ApprovalBriefSource::default(),
        }
    }

    #[test]
    fn refuses_mismatched_account_sales_customer_and_settlement_party() {
        let (request, account) = fixtures();
        source::validate_binding(&request, &account).unwrap();
        let mut changed = account.clone();
        changed.base.id = "another-account".into();
        assert!(source::validate_binding(&request, &changed).is_err());
        let mut changed = account.clone();
        changed.sales_order_id = SalesOrderId::new("other-sales");
        assert!(source::validate_binding(&request, &changed).is_err());
        let mut changed = account.clone();
        changed.customer_id = CustomerAccountId::new("other-customer");
        assert!(source::validate_binding(&request, &changed).is_err());
        let mut changed = account;
        changed.counterparty_party_id = PartyId::new("other-party");
        assert!(source::validate_binding(&request, &changed).is_err());
    }

    #[test]
    fn flags_changed_tax_identity_revision_and_overreservation_without_mutating_snapshot() {
        let (request, account) = fixtures();
        let frozen = source();
        let mut current = frozen.clone();
        current.revision_id = "new-revision".into();
        let mut identity = InvoiceRequestSource {
            effective: true,
            invoice_title: request.data.invoice_title.clone(),
            tax_number: request.data.tax_number.clone(),
        };
        let amounts = InvoiceReviewAmounts::from_requests(
            account.open_invoiceable_total,
            &request.base.id,
            std::slice::from_ref(&request),
        );
        assert!(warnings(&request, std::slice::from_ref(&frozen), &frozen, &amounts, &identity).is_empty());
        identity.effective = false;
        identity.tax_number = "NEW-TAX".into();
        let shortage = InvoiceReviewAmounts::from_requests(
            "100".parse().unwrap(),
            &request.base.id,
            std::slice::from_ref(&request),
        );
        let issues = warnings(&request, std::slice::from_ref(&frozen), &current, &shortage, &identity);
        assert!(issues.iter().any(|value| value.contains("来源条件")));
        assert!(issues.iter().any(|value| value.contains("税号")));
        assert!(issues.iter().any(|value| value.contains("销售版本已变更")));
        assert!(issues.iter().any(|value| value.contains("65")));
        assert_eq!(frozen.revision_id, "revision");
        assert_eq!(request.data.tax_number, "TAX");
    }

    #[test]
    fn missing_frozen_source_does_not_invent_a_historical_version_change() {
        let (request, account) = fixtures();
        let amounts = InvoiceReviewAmounts::from_requests(
            account.open_invoiceable_total,
            &request.base.id,
            std::slice::from_ref(&request),
        );
        let identity = InvoiceRequestSource {
            effective: true,
            invoice_title: request.data.invoice_title.clone(),
            tax_number: request.data.tax_number.clone(),
        };
        assert!(warnings(&request, &[], &source(), &amounts, &identity).is_empty());
    }
}
