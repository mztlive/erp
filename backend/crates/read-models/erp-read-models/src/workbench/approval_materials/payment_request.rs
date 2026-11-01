//! 付款申请审批材料只追溯本次逐采购冻结的销售版本与直接对账附件。

use std::collections::{BTreeMap, BTreeSet};

use erp_finance::entity::payable::{PaymentRequestSalesSource, PurchasePaymentRequest};
use erp_finance::repository::PurchasePaymentRequestSnapshotExt;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::DocumentType;
use erp_workflow::entity::approval_integration::display_snapshot::{
    ApprovalBriefSource, ApprovalRelatedSalesSnapshot,
};
use erp_workflow::entity::approval_integration::{ApprovalMaterialFile, ApprovalSubjectSnapshot};
use mongodb::Database;
use persistence_core::Executor;

use super::super::brief::{BRIEF_LINE_LIMIT, BriefLine, push_section};
use crate::sales_center::materials::revision_materials;
use crate::{Error, Result};

pub(super) async fn direct_ids(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Vec<String>> {
    if snapshot.document_type != DocumentType::PurchasePaymentRequest {
        return Ok(Vec::new());
    }
    let request = frozen_request(db, snapshot, executor).await?;
    Ok(request.data.attachment_ids)
}

pub(super) async fn capture(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<(Vec<ApprovalRelatedSalesSnapshot>, Vec<ApprovalMaterialFile>)> {
    let request = frozen_request(db, snapshot, executor).await?;
    let mut sources = BTreeMap::new();
    let mut files = BTreeSet::new();
    let mut materials = Vec::new();
    for line in &request.lines {
        for source in &line.source_snapshot.sales_sources {
            let key = (source.sales_order_id.clone(), source.sales_revision_id.clone());
            sources.entry(key).or_insert(source);
        }
        files.extend(line.source_snapshot.allowed_attachment_ids.iter().cloned());
    }
    let mut displays = Vec::new();
    for source in sources.into_values() {
        let revision = db
            .sales_order_revisions()
            .find_by_id(&source.sales_revision_id, executor)
            .await?
            .filter(|revision| {
                revision.sales_order_id.as_ref() == source.sales_order_id
                    && revision.settlement_party_id.as_ref() == request.payer_party_id
            })
            .ok_or_else(|| Error::ConflictError("付款申请冻结销售来源或付款主体不一致".into()))?;
        let permitted = revision_materials(db, &revision, executor).await?.files;
        materials.extend(permitted.into_iter().filter(|file| files.contains(file.file_asset_id.as_ref())));
        displays.push(sales_display(source)?);
    }
    if materials.iter().map(|file| file.file_asset_id.to_string()).collect::<BTreeSet<_>>() != files {
        return Err(Error::ConflictError("付款申请来源允许文件与正式销售材料不一致".into()));
    }
    Ok((displays, materials))
}

async fn frozen_request(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<PurchasePaymentRequest> {
    db.purchase_payment_request_snapshots()
        .find_for_version(&snapshot.business_object_id, snapshot.subject_version, executor)
        .await?
        .filter(|frozen| {
            frozen.request_id == snapshot.business_object_id
                && frozen.request.base.id == snapshot.business_object_id
                && frozen.request.submission_version == snapshot.subject_version
                && frozen.approval_instance_id == snapshot.approval_process_instance_id.as_ref()
                && frozen.request.approval_instance_id.as_deref()
                    == Some(snapshot.approval_process_instance_id.as_ref())
        })
        .map(|frozen| frozen.request)
        .ok_or_else(|| Error::ConflictError("付款申请原提交材料与审批实例不一致".into()))
}

fn sales_display(source: &PaymentRequestSalesSource) -> Result<ApprovalRelatedSalesSnapshot> {
    let preview = &source.preview;
    let document_no = preview
        .get("sales_order_no")
        .and_then(|value| value.as_str())
        .ok_or_else(|| Error::ConflictError("付款申请销售预览缺少单号".into()))?;
    let revision_no = preview
        .get("revision_no")
        .and_then(|value| value.as_u64())
        .and_then(|value| u32::try_from(value).ok())
        .ok_or_else(|| Error::ConflictError("付款申请销售预览缺少正式版本".into()))?;
    let all_lines = preview
        .get("lines")
        .and_then(|value| value.as_array())
        .ok_or_else(|| Error::ConflictError("付款申请销售预览缺少完整明细".into()))?;
    let mut sections = Vec::new();
    push_section(&mut sections, "我方结算主体", Some(&source.payer_party_name), false);
    let lines = all_lines
        .iter()
        .take(BRIEF_LINE_LIMIT)
        .map(|line| BriefLine {
            title: line.get("item_name").and_then(|value| value.as_str()).unwrap_or("商品名称未保存").into(),
            quantity: line
                .get("gross_amount")
                .and_then(|value| value.as_str())
                .map(|amount| format!("销售成交金额 ¥{amount}")),
            due_label: None,
        })
        .collect();
    Ok(ApprovalRelatedSalesSnapshot {
        document_id: source.sales_order_id.clone(),
        document_no: document_no.into(),
        revision_id: source.sales_revision_id.clone(),
        revision_no,
        source: ApprovalBriefSource {
            customer: preview.get("customer_name").and_then(|value| value.as_str()).map(str::to_owned),
            amount_label: preview
                .pointer("/totals/gross")
                .and_then(|value| value.as_str())
                .map(|value| format!("¥{value}")),
            lines,
            more_count: u32::try_from(all_lines.len().saturating_sub(BRIEF_LINE_LIMIT)).unwrap_or(u32::MAX),
            submitter_name: None,
            list_summary: format!("销售正式版本 {revision_no}"),
            extra_sections: sections,
        },
    })
}
