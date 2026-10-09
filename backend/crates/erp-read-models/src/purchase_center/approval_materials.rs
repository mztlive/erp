//! 采购审批对照仅读取精确采购提交及冻结允许的销售版本，不访问当前草稿。

use erp_core::ids::{PurchaseOrderId, PurchaseOrderSubmissionId, SalesOrderRevisionLineId};
use erp_procurement::entity::purchase_order::{PurchaseLineType, PurchaseOrderSubmissionLine};
use erp_procurement::repository::PurchaseOrderExt;
use erp_procurement::repository::prelude::*;
use erp_sales::entity::sales_order::{LineType, SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine};
use erp_sales::repository::SalesOrderExt;
use erp_sales::repository::prelude::*;
use erp_workflow::entity::approval_integration::display_snapshot::ApprovalRelatedSalesSnapshot;
use erp_workflow::service::approval::execution::runtime_service::ApprovalMaterialsView;
use mongodb::Database;
use persistence_core::NoTransaction;
use serde::Serialize;

use crate::sales_center::materials::repository::revision_lines;
use crate::{Error, Result};

/// 本次采购提交行，以及通过精确行关系证明的客户销售需求。
#[derive(Debug, Serialize)]
pub struct ApprovalPurchaseLine {
    pub id: String,
    pub line_no: u32,
    pub title: String,
    pub specification: Option<String>,
    pub quantity: Option<String>,
    pub unit: Option<String>,
    pub unit_cost_gross: Option<String>,
    pub expected_delivery_date: Option<String>,
    pub source: Option<ApprovalPurchaseSourceLine>,
}

/// 已冻结允许版本中的销售行；金额与数量保持十进制字符串。
#[derive(Debug, Serialize)]
pub struct ApprovalPurchaseSourceLine {
    pub revision_id: String,
    pub title: String,
    pub specification: Option<String>,
    pub quantity: String,
    pub unit: String,
    pub unit_price_gross: String,
    pub fulfillment_due_at: i64,
}

/// 已授权资料的精确采购提交对照；历史未冻结来源时不回填。
/// # 参数
/// 数据库及已经通过审批实例读取授权的资料。
/// # 返回
/// 全部采购提交行；没有冻结销售来源的历史资料返回空值。
/// # 错误
/// 提交、总行数、销售版本或稳定销售行不匹配时拒绝读取。
pub(crate) async fn purchase_lines(
    db: &Database,
    materials: &ApprovalMaterialsView,
) -> Result<Option<Vec<ApprovalPurchaseLine>>> {
    if materials.display.source_sales.is_empty() {
        return Ok(None);
    }
    let submission = db
        .purchase_order_submissions()
        .find_by_order_and_submission_no(
            &PurchaseOrderId::new(&materials.document_id),
            &format!("SUB-{:06}", materials.subject_version),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::ConflictError("本次审批对应的采购提交不存在".into()))?;
    let lines = db
        .purchase_order()
        .list_submission_lines(&PurchaseOrderSubmissionId::new(&submission.base.id), &mut NoTransaction)
        .await?;
    validate_submission(
        materials,
        submission.purchase_order_id.as_ref(),
        submission.formal_sequence(),
        lines.len(),
    )?;
    let ids = lines
        .iter()
        .filter_map(|line| line.sales_order_revision_line_id.as_ref().map(ToString::to_string))
        .collect::<Vec<_>>();
    let sources = revision_lines(db, &ids, &mut NoTransaction).await?;
    validate_revisions(db, &materials.display.source_sales).await?;
    let goods = db
        .sales_order_goods_service_line_revisions()
        .list_by_revision_line_ids(
            &ids.iter().map(SalesOrderRevisionLineId::new).collect::<Vec<_>>(),
            &mut NoTransaction,
        )
        .await?;
    lines
        .iter()
        .map(|line| comparison_line(line, &materials.display.source_sales, &sources, &goods))
        .collect::<Result<Vec<_>>>()
        .map(Some)
}

fn validate_submission(
    materials: &ApprovalMaterialsView,
    order_id: &str,
    version: Option<u32>,
    count: usize,
) -> Result<()> {
    let source = &materials.display.source;
    let expected = u32::try_from(source.lines.len()).ok().and_then(|n| n.checked_add(source.more_count));
    if materials.document_type != "purchase_order"
        || materials.document_id != order_id
        || materials.display.root_document_id != order_id
        || version != Some(materials.subject_version)
        || expected != u32::try_from(count).ok()
    {
        return Err(Error::ConflictError("采购提交与本次审批资料不一致".into()));
    }
    Ok(())
}

async fn validate_revisions(db: &Database, allowed: &[ApprovalRelatedSalesSnapshot]) -> Result<()> {
    let ids = allowed.iter().map(|source| source.revision_id.clone()).collect::<Vec<_>>();
    let revisions = db.sales_order_revisions().find_revisions_by_ids(&ids, &mut NoTransaction).await?;
    if revisions.len() != allowed.len()
        || allowed.iter().any(|source| {
            !revisions.iter().any(|revision| {
                revision.base.id == source.revision_id
                    && revision.sales_order_id.as_ref() == source.document_id
                    && revision.revision.revision_no == source.revision_no
            })
        })
    {
        return Err(Error::ConflictError("采购来源销售版本与冻结资料不一致".into()));
    }
    Ok(())
}

fn comparison_line(
    line: &PurchaseOrderSubmissionLine,
    allowed: &[ApprovalRelatedSalesSnapshot],
    sources: &[SalesOrderRevisionLine],
    goods: &[SalesOrderGoodsServiceLineRevision],
) -> Result<ApprovalPurchaseLine> {
    Ok(ApprovalPurchaseLine {
        id: line.base.id.clone(),
        line_no: line.line_no,
        title: line.product_name_snapshot.clone().unwrap_or_else(|| {
            if line.line_type == PurchaseLineType::LogisticsFee {
                "物流费用"
            } else {
                "未记录商品名称"
            }
            .into()
        }),
        specification: line.specification_snapshot.clone(),
        quantity: line.quantity.as_ref().map(|value| value.to_decimal().to_string()),
        unit: line.base_unit_code.clone(),
        unit_cost_gross: line.unit_cost_gross.as_ref().map(|value| value.to_decimal().to_string()),
        expected_delivery_date: line.expected_delivery_date.map(|date| date.to_string()),
        source: source_line(line, allowed, sources, goods)?,
    })
}

fn source_line(
    line: &PurchaseOrderSubmissionLine,
    allowed: &[ApprovalRelatedSalesSnapshot],
    sources: &[SalesOrderRevisionLine],
    goods: &[SalesOrderGoodsServiceLineRevision],
) -> Result<Option<ApprovalPurchaseSourceLine>> {
    if line.line_type == PurchaseLineType::LogisticsFee {
        return Ok(None);
    }
    let source = line
        .sales_order_revision_line_id
        .as_ref()
        .and_then(|id| sources.iter().find(|source| source.base.id == id.as_ref()))
        .filter(|source| {
            source.line_type == LineType::GoodsService
                && Some(&source.sales_order_line_id) == line.sales_order_line_id.as_ref()
                && allowed
                    .iter()
                    .any(|allowed| allowed.revision_id == source.sales_order_revision_id.as_ref())
        })
        .ok_or_else(|| Error::ConflictError("采购提交与冻结销售来源行不匹配".into()))?;
    let goods = goods
        .iter()
        .find(|goods| goods.revision_line_id.as_ref() == source.base.id)
        .ok_or_else(|| Error::ConflictError("来源销售版本明细缺少成交价".into()))?;
    Ok(Some(ApprovalPurchaseSourceLine {
        revision_id: source.sales_order_revision_id.to_string(),
        title: source.item_name_snapshot.clone(),
        specification: source.spec_snapshot.clone(),
        quantity: goods.quantity.to_decimal().to_string(),
        unit: goods.base_unit_code.clone(),
        unit_price_gross: goods.unit_price_gross.to_decimal().to_string(),
        fulfillment_due_at: goods.fulfillment_due_at.unix_secs(),
    }))
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::{
        ProcurementConfirmationLineId, PurchaseOrderSubmissionLineId, SalesOrderGoodsServiceLineRevisionId,
        SalesOrderLineId, SalesOrderRevisionId, SkuId, SkuRevisionId,
    };
    use erp_procurement::entity::purchase_order::PurchaseOrderSubmissionLineData;
    use erp_sales::entity::sales_order::{
        SalesOrderGoodsServiceLineRevisionData, SalesOrderRevisionLineData,
    };
    use erp_workflow::entity::approval_integration::display_snapshot::{
        ApprovalBriefLine, ApprovalBriefSource, ApprovalDisplaySnapshot,
    };

    use super::*;
    fn sales_line() -> SalesOrderRevisionLine {
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
                supplier_offering_source: None,
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

    fn goods() -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision::new(
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
        .unwrap()
    }

    fn allowed() -> Vec<ApprovalRelatedSalesSnapshot> {
        vec![ApprovalRelatedSalesSnapshot {
            document_id: "sales-1".into(),
            document_no: "XS-1".into(),
            revision_id: "revision-1".into(),
            revision_no: 1,
            source: ApprovalBriefSource::default(),
        }]
    }

    #[test]
    fn comparison_uses_exact_line_prices_and_preserves_partial_quantity() {
        let mut sales = goods();
        sales.quantity = "10".parse().unwrap();
        let view = comparison_line(&purchase_line(), &allowed(), &[sales_line()], &[sales]).unwrap();
        assert_eq!(view.quantity.as_deref(), Some("2"));
        assert_eq!(view.unit_cost_gross.as_deref(), Some("15"));
        let source = view.source.unwrap();
        assert_eq!(source.quantity, "10");
        assert_eq!(source.unit_price_gross, "20");
        assert_eq!(source.fulfillment_due_at, 1_800_000_000);
    }

    #[test]
    fn missing_or_unfrozen_revision_and_wrong_stable_line_fail_closed() {
        let purchase = purchase_line();
        assert!(comparison_line(&purchase, &[], &[sales_line()], &[goods()]).is_err());
        assert!(comparison_line(&purchase, &allowed(), &[], &[goods()]).is_err());
        assert!(comparison_line(&purchase, &allowed(), &[sales_line()], &[]).is_err());
        let mut source = sales_line();
        source.sales_order_line_id = SalesOrderLineId::new("other-line");
        assert!(comparison_line(&purchase, &allowed(), &[source], &[goods()]).is_err());
        let mut source = sales_line();
        source.sales_order_revision_id = SalesOrderRevisionId::new("later-version");
        assert!(comparison_line(&purchase, &allowed(), &[source], &[goods()]).is_err());
    }

    #[test]
    fn logistics_fee_does_not_invent_sales_price() {
        let mut purchase = purchase_line();
        purchase.line_type = PurchaseLineType::LogisticsFee;
        purchase.quantity = None;
        purchase.unit_cost_gross = None;
        purchase.product_name_snapshot = None;
        let view = comparison_line(&purchase, &allowed(), &[], &[]).unwrap();
        assert!(view.source.is_none());
        assert!(view.unit_cost_gross.is_none());
        assert_eq!(view.title, "物流费用");
    }

    #[test]
    fn submission_requires_exact_order_version_and_complete_line_count() {
        let mut display = ApprovalDisplaySnapshot::new("purchase-order".into());
        display.source.lines =
            vec![ApprovalBriefLine { title: "礼盒".into(), quantity: None, due_label: None }];
        display.source.more_count = 2;
        let mut materials = ApprovalMaterialsView {
            document_no: "PO-1".into(),
            document_type: "purchase_order".into(),
            document_id: "purchase-order".into(),
            subject_version: 1,
            display,
            attachments: vec![],
        };
        assert!(validate_submission(&materials, "purchase-order", Some(1), 3).is_ok());
        assert!(validate_submission(&materials, "other-order", Some(1), 3).is_err());
        assert!(validate_submission(&materials, "purchase-order", Some(2), 3).is_err());
        assert!(validate_submission(&materials, "purchase-order", None, 3).is_err());
        assert!(validate_submission(&materials, "purchase-order", Some(1), 1).is_err());
        materials.display.root_document_id = "other-order".into();
        assert!(validate_submission(&materials, "purchase-order", Some(1), 3).is_err());
    }
}
