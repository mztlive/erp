//! 采购审批只从本次不可变提交行追溯销售版本，冻结来源摘要及材料。

use std::collections::BTreeSet;

use erp_core::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderRevisionId, SalesOrderRevisionLineId,
};
use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrderSubmissionLine};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::entity::sales_order::{
    LineType, SalesOrderGoodsServiceLineRevision, SalesOrderRevision, SalesOrderRevisionLine,
};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::entity::approval_integration::display_snapshot::{
    ApprovalBriefSource, ApprovalRelatedSalesSnapshot,
};
use erp_workflow::entity::approval_integration::{ApprovalMaterialFile, ApprovalSubjectSnapshot};
use mongodb::Database;
use persistence_core::Executor;

use super::super::brief::{
    BRIEF_LINE_LIMIT, BriefLine, BriefSection, format_quantity, line_title, push_section,
};
use super::super::presentation::format_yuan;
use crate::sales_center::materials::repository::revision_lines;
use crate::sales_center::materials::revision_materials;
use crate::{Error, Result};

/// 在采购提交原事务冻结准确销售版本与材料，不依赖采购人的全局销售/文件权限。
pub(super) async fn capture(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<(Vec<ApprovalRelatedSalesSnapshot>, Vec<ApprovalMaterialFile>)> {
    let revisions = source_revisions(db, snapshot, executor).await?;
    let mut display = Vec::new();
    let mut files = Vec::new();
    for revision in revisions {
        let mut sales = revision_display(db, &revision, executor).await?;
        match revision_materials(db, &revision, executor).await {
            Ok(materials) => files.extend(materials.files),
            Err(error) => {
                material_unavailable(error)?;
                push_section(
                    &mut sales.source.extra_sections,
                    "合同或凭证",
                    Some("暂不可读取，请联系销售核对资料后再审批"),
                    false,
                );
            },
        }
        display.push(sales);
    }
    Ok((display, files))
}

/// 历史材料单独关闭；来源版本与摘要仍由严格读取证明，基础设施故障继续传播。
pub(super) fn material_unavailable(error: Error) -> Result<()> {
    match error {
        Error::NotFound(_)
        | Error::ConflictError(_)
        | Error::Forbidden(_)
        | Error::ValidationError(_)
        | Error::Logic(_) => Ok(()),
        error => Err(error),
    }
}

/// 按本次采购提交行的稳定销售行关系批量解析来源修订。
async fn source_revisions(
    db: &Database,
    snapshot: &ApprovalSubjectSnapshot,
    executor: &mut dyn Executor,
) -> Result<Vec<SalesOrderRevision>> {
    let order_id = PurchaseOrderId::new(&snapshot.business_object_id);
    let order = db
        .purchase_orders()
        .find_by_id(order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购审批来源采购单不存在".into()))?;
    let submission = db
        .purchase_order_submissions()
        .find_by_order_and_submission_no(&order_id, &snapshot.subject_version.to_string(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购审批精确提交不存在".into()))?;
    let lines = db
        .purchase_order()
        .list_submission_lines(&PurchaseOrderSubmissionId::new(&submission.base.id), executor)
        .await?;
    let ids = lines
        .iter()
        .filter_map(|line| line.sales_order_revision_line_id.as_ref().map(ToString::to_string))
        .collect::<BTreeSet<_>>();
    let sources = revision_lines(db, &ids.iter().cloned().collect::<Vec<_>>(), executor).await?;
    if sources.len() != ids.len() {
        return Err(Error::ConflictError("采购提交引用的销售版本行不存在".into()));
    }
    validate_source_lines(&lines, &sources)?;
    let mut revision_ids =
        sources.iter().map(|line| line.sales_order_revision_id.to_string()).collect::<BTreeSet<_>>();
    if revision_ids.is_empty() {
        revision_ids.insert(order.sales_order_revision_id.to_string());
    }
    let expected_count = revision_ids.len();
    let revisions = db
        .sales_order_revisions()
        .find_revisions_by_ids(&revision_ids.into_iter().collect::<Vec<_>>(), executor)
        .await?;
    if revisions.len() != expected_count
        || revisions.iter().any(|revision| revision.sales_order_id != order.sales_order_id)
    {
        return Err(Error::ConflictError("采购提交关联了其他销售单版本".into()));
    }
    Ok(revisions)
}

/// 商品行必须准确引用同一稳定销售行的实物版本；费用行不制造来源行。
fn validate_source_lines(
    lines: &[PurchaseOrderSubmissionLine],
    sources: &[SalesOrderRevisionLine],
) -> Result<()> {
    for line in lines {
        if line.line_type == PurchaseLineType::LogisticsFee {
            continue;
        }
        let source = line
            .sales_order_revision_line_id
            .as_ref()
            .and_then(|id| sources.iter().find(|source| source.base.id == id.as_ref()));
        if source.is_none_or(|source| {
            source.line_type != LineType::GoodsService
                || Some(&source.sales_order_line_id) != line.sales_order_line_id.as_ref()
        }) {
            return Err(Error::ConflictError("采购提交与销售来源行不匹配".into()));
        }
    }
    Ok(())
}

/// 用不可变正式销售修订读取完整行，再按审批摘要上限截断。
async fn revision_display(
    db: &Database,
    revision: &SalesOrderRevision,
    executor: &mut dyn Executor,
) -> Result<ApprovalRelatedSalesSnapshot> {
    let order = db
        .sales_orders()
        .find_by_id(revision.sales_order_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::ConflictError("采购来源销售单不存在".into()))?;
    let lines = db
        .sales_order_revision_lines()
        .list_lines_by_revision(&SalesOrderRevisionId::new(&revision.base.id), executor)
        .await?;
    let ids = lines.iter().map(|line| SalesOrderRevisionLineId::new(&line.base.id)).collect::<Vec<_>>();
    let goods =
        db.sales_order_goods_service_line_revisions().list_by_revision_line_ids(&ids, executor).await?;
    let brief_lines = lines.iter().map(|line| sales_brief_line(line, &goods)).collect::<Result<Vec<_>>>()?;
    Ok(ApprovalRelatedSalesSnapshot {
        document_id: revision.sales_order_id.to_string(),
        document_no: order.order_no,
        revision_id: revision.base.id.clone(),
        revision_no: revision.revision.revision_no,
        source: revision_brief(revision, brief_lines)?,
    })
}

/// 成交数量、含税销售单价及行金额均来自同一准确正式版本，不回退目录参考价。
fn sales_brief_line(
    line: &SalesOrderRevisionLine,
    goods: &[SalesOrderGoodsServiceLineRevision],
) -> Result<BriefLine> {
    let goods = goods
        .iter()
        .find(|goods| goods.revision_line_id.as_ref() == line.base.id)
        .filter(|_| line.line_type == LineType::GoodsService)
        .ok_or_else(|| Error::ConflictError("来源销售版本明细缺少成交价".into()))?;
    Ok(BriefLine {
        title: line_title(&line.item_name_snapshot, line.spec_snapshot.as_deref()),
        quantity: Some(format!(
            "{} · 销售单价 ¥{} · {}",
            format_quantity(&goods.quantity, Some(&goods.base_unit_code)),
            goods.unit_price_gross.to_decimal(),
            format_yuan(&line.gross_amount)
        )),
        due_label: None,
    })
}

/// 来源销售只展示该修订保存的客户、金额、合同及明细，不查当前草稿。
fn revision_brief(revision: &SalesOrderRevision, mut lines: Vec<BriefLine>) -> Result<ApprovalBriefSource> {
    let more_count = u32::try_from(lines.len().saturating_sub(BRIEF_LINE_LIMIT))
        .map_err(|_| Error::ValidationError("来源销售明细数量超出允许范围".into()))?;
    lines.truncate(BRIEF_LINE_LIMIT);
    let mut sections: Vec<BriefSection> = Vec::new();
    push_section(
        &mut sections,
        "合同",
        revision.contract_snapshot.as_ref().map(|value| value.contract_no.as_str()),
        false,
    );
    push_section(&mut sections, "付款条件", Some(&revision.payment_term_snapshot.payment_term_name), false);
    Ok(ApprovalBriefSource {
        customer: Some(revision.customer_snapshot.customer_name.clone()),
        amount_label: Some(format_yuan(&revision.gross_amount)),
        lines,
        more_count,
        submitter_name: None,
        list_summary: format!("销售版本 {}", revision.revision.revision_no),
        extra_sections: sections,
    })
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderSubmissionLineId, SalesOrderGoodsServiceLineRevisionId,
        SalesOrderLineId, SkuId, SkuRevisionId,
    };
    use erp_procurement::entity::purchase_order::PurchaseOrderSubmissionLineData;
    use erp_sales::entity::sales_order::{
        SalesOrderGoodsServiceLineRevisionData, SalesOrderRevisionLineData,
    };

    use super::*;

    fn source_line() -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new("source-line"),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("revision-1"),
                sales_order_line_id: SalesOrderLineId::new("stable-line"),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: "40".parse().unwrap(),
                net_amount: "40".parse().unwrap(),
                tax_amount: "0".parse().unwrap(),
                sales_tax_rate: "0".parse().unwrap(),
                item_name_snapshot: "礼盒".into(),
                spec_snapshot: None,
                unit_snapshot: Some("盒".into()),
            },
        )
        .unwrap()
    }

    fn purchase_line() -> PurchaseOrderSubmissionLine {
        PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new("purchase-line"),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("purchase-submission"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("confirmation")),
                sku_id: Some(SkuId::new("sku")),
                sku_revision_id: Some(SkuRevisionId::new("sku-revision")),
                product_name_snapshot: Some("礼盒".into()),
                specification_snapshot: Some("标准装".into()),
                quantity: Some("2".parse().unwrap()),
                base_unit_code: Some("盒".into()),
                unit_cost_gross: Some("15".parse().unwrap()),
                gross_amount: "30".parse().unwrap(),
                net_amount: "30".parse().unwrap(),
                tax_amount: "0".parse().unwrap(),
                input_tax_rate: Some("0".parse().unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new("stable-line")),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("source-line")),
                sales_order_submission_line_id: None,
                allocated_quantity: Some("2".parse().unwrap()),
            },
        )
        .unwrap()
    }

    #[test]
    fn purchase_source_requires_an_existing_matching_goods_revision_line() {
        let source = source_line();
        let mut purchase = purchase_line();
        validate_source_lines(std::slice::from_ref(&purchase), std::slice::from_ref(&source)).unwrap();
        purchase.sales_order_revision_line_id = None;
        assert!(
            validate_source_lines(std::slice::from_ref(&purchase), std::slice::from_ref(&source)).is_err()
        );
        purchase = purchase_line();
        purchase.sales_order_line_id = Some(SalesOrderLineId::new("another-stable-line"));
        assert!(validate_source_lines(&[purchase], &[source]).is_err());
    }

    #[test]
    fn source_brief_reads_customer_transaction_price_and_rejects_missing_price() {
        let source = source_line();
        let goods = SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new("goods"),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new("source-line"),
                sku_id: SkuId::new("sku"),
                sku_revision_id: SkuRevisionId::new("sku-revision"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: "2".parse().unwrap(),
                base_unit_code: "盒".into(),
                unit_price_gross: "20".parse().unwrap(),
                pricing_mode: Default::default(),
            },
        )
        .unwrap();
        let brief = sales_brief_line(&source, &[goods]).unwrap();
        assert_eq!(brief.title, "礼盒");
        assert!(brief.quantity.unwrap().contains("销售单价 ¥20"));
        assert!(sales_brief_line(&source, &[]).is_err());
    }

    #[test]
    fn material_failure_is_local_but_infrastructure_failure_is_propagated() {
        material_unavailable(Error::Forbidden("历史附件已隔离".into())).unwrap();
        assert!(matches!(
            material_unavailable(Error::Internal("数据库故障".into())),
            Err(Error::Internal(_))
        ));
    }
}
