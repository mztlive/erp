//! 采购确认审批自动生成采购单草稿。
//!
//! 本模块是 D14 销售复核与 D15 采购单之间的应用层协作接缝：按供应商与履约方式
//! 拆单，构造采购主表、草稿提交和明细，但不自行开启事务或写库。调用方必须把返回
//! 的全部 bundle 与采购确认决策放进同一个事务。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use database::{NoTransaction, PartyExt, SalesOrderExt, SupplierExt, SupplierOfferingExt};
use entities::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
    SupplierAccountId,
};
use entities::money::{line_amounts, round_to_cent, Amount, Rate};
use entities::purchase_order::{
    PaymentTermSnapshot, PurchaseLineType, PurchaseOrder, PurchaseOrderData, PurchaseOrderSubmission,
    PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, PurchaseType,
    SupplierSnapshot,
};
use entities::sales_order::SalesOrderSubmissionLine;
use entities::sales_review::{FulfillmentMode, ProcurementConfirmationLine};
use entities::supplier_offering::SupplierOfferingRevision;
use id_generator::next_id;
use mongodb::Database;

use super::{fulfillment_from_mode, today_stamp};
use crate::errors::{Error, Result};

/// 审批事务中待写入的一张采购单草稿聚合。
#[derive(Debug)]
pub(crate) struct AutomaticPurchaseDraft {
    /// 采购单主表。
    pub order: PurchaseOrder,
    /// 初始草稿提交。
    pub submission: PurchaseOrderSubmission,
    /// 草稿商品与费用行。
    pub lines: Vec<PurchaseOrderSubmissionLine>,
}

/// 按供应商与履约方式构造采购单草稿。
///
/// 商品成本来自采购确认行；供给修订中的运费和服务费按供给版本去重后形成独立物流
/// 费用行，避免把一次性费用重复摊入商品单价。函数只读取并构造对象，不写数据库。
///
/// # 参数
/// * `db` - 数据库连接
/// * `sales_order_id` - 来源销售单
/// * `confirmation_lines` - 已通过完整来源复验的采购确认行
/// * `actor_id` - 审批执行人
///
/// # 返回
/// 返回按 `供应商 × 履约方式` 稳定排序的采购单草稿聚合。
///
/// # 错误
/// 确认行为空、供应商或商务版本缺失、销售提交行/供给修订缺失时返回错误。
pub(crate) async fn build_automatic_purchase_drafts(
    db: &Database,
    sales_order_id: &SalesOrderId,
    confirmation_lines: &[ProcurementConfirmationLine],
    actor_id: &str,
) -> Result<Vec<AutomaticPurchaseDraft>> {
    if confirmation_lines.is_empty() {
        return Err(Error::ValidationError("采购确认没有可建单分行".to_string()));
    }
    let sales_lines = sales_submission_lines(db, confirmation_lines).await?;
    let offering_revisions = offering_revisions(db, confirmation_lines).await?;
    let grouped = group_confirmation_lines(confirmation_lines);
    let mut drafts = Vec::with_capacity(grouped.len());
    for (_, lines) in grouped {
        drafts.push(
            build_group_draft(
                db,
                sales_order_id,
                &lines,
                &sales_lines,
                &offering_revisions,
                actor_id,
            )
            .await?,
        );
    }
    Ok(drafts)
}

/// 按拆单维度分组并保持稳定顺序。
fn group_confirmation_lines(
    lines: &[ProcurementConfirmationLine],
) -> BTreeMap<(String, String), Vec<ProcurementConfirmationLine>> {
    let mut grouped = BTreeMap::new();
    for line in lines {
        grouped
            .entry((
                line.supplier_id.to_string(),
                line.fulfillment_mode.as_str().to_string(),
            ))
            .or_insert_with(Vec::new)
            .push(line.clone());
    }
    grouped
}

/// 批量加载确认行引用的销售提交行。
async fn sales_submission_lines(
    db: &Database,
    lines: &[ProcurementConfirmationLine],
) -> Result<HashMap<String, SalesOrderSubmissionLine>> {
    let ids = lines
        .iter()
        .map(|line| line.sales_order_submission_line_id.to_string())
        .collect::<Vec<_>>();
    let rows = db
        .sales_order_submission_lines()
        .find_many(mongodb::bson::doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
    Ok(rows
        .into_iter()
        .map(|line| (line.base.id.clone(), line))
        .collect())
}

/// 批量加载确认行引用的供给修订。
async fn offering_revisions(
    db: &Database,
    lines: &[ProcurementConfirmationLine],
) -> Result<HashMap<String, SupplierOfferingRevision>> {
    let ids = lines
        .iter()
        .filter_map(|line| line.supplier_offering_revision_id.clone())
        .map(|id| id.to_string())
        .collect::<Vec<_>>();
    let rows = db
        .supplier_offering_revisions()
        .find_many(mongodb::bson::doc! { "id": { "$in": ids } }, &mut NoTransaction)
        .await?;
    Ok(rows
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect())
}

/// 构造一个拆单分组的采购主表、草稿提交和明细。
async fn build_group_draft(
    db: &Database,
    sales_order_id: &SalesOrderId,
    confirmation_lines: &[ProcurementConfirmationLine],
    sales_lines: &HashMap<String, SalesOrderSubmissionLine>,
    revisions: &HashMap<String, SupplierOfferingRevision>,
    actor_id: &str,
) -> Result<AutomaticPurchaseDraft> {
    let first = confirmation_lines
        .first()
        .ok_or_else(|| Error::ValidationError("采购拆单分组为空".to_string()))?;
    let supplier_context = supplier_context(db, &first.supplier_id).await?;
    let purchase_type = purchase_type_from_mode(first.fulfillment_mode);
    let fulfillment = fulfillment_from_mode(first.fulfillment_mode);
    let order_id = PurchaseOrderId::new(next_id());
    let purchase_no = format!("PO-{}-{}", today_stamp(), &order_id.to_string()[..6]);
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no,
            sales_order_id: sales_order_id.clone(),
            supplier_id: first.supplier_id.clone(),
            purchase_type,
            payment_term_code: supplier_context.payment_term_code.clone(),
            fulfillment_responsibility: fulfillment,
        },
        actor_id,
    )?;
    let submission_id = PurchaseOrderSubmissionId::new(next_id());
    let lines = build_draft_lines(
        &submission_id,
        confirmation_lines,
        sales_lines,
        revisions,
        first.fulfillment_mode,
    )?;
    let (gross_amount, net_amount, tax_amount) = line_totals(&lines)?;
    let payment_term_snapshot = payment_term_snapshot(&supplier_context.payment_term_code)?;
    let submission = PurchaseOrderSubmission::new(
        submission_id,
        PurchaseOrderSubmissionData {
            purchase_order_id: order_id,
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: first.supplier_id.clone(),
            purchase_type,
            fulfillment_responsibility: fulfillment,
            supplier_revision_id: supplier_context.commercial_revision_id,
            supplier_snapshot: SupplierSnapshot::new(supplier_context.name)?,
            payment_term_snapshot,
            gross_amount,
            net_amount,
            tax_amount,
        },
    )?;
    order.current_submission_id = Some(submission.base.id.clone());
    Ok(AutomaticPurchaseDraft {
        order,
        submission,
        lines,
    })
}

/// 供应商建单所需的当前商务上下文。
struct SupplierContext {
    name: String,
    commercial_revision_id: entities::ids::SupplierCommercialProfileRevisionId,
    payment_term_code: String,
}

/// 读取供应商主体名称与当前商务结算版本。
async fn supplier_context(db: &Database, supplier_id: &SupplierAccountId) -> Result<SupplierContext> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(supplier_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let commercial_revision_id = supplier
        .current_commercial_profile_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
    let commercial = db
        .supplier_commercial_profile_revisions()
        .find_by_id(&commercial_revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商商务结算版本不存在".to_string()))?;
    let party = db
        .parties()
        .find_by_id(&supplier.party_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商主体不存在".to_string()))?;
    let party_revision_id = party
        .stable
        .current_revision_id
        .ok_or_else(|| Error::BusinessLogicError("供应商主体缺少当前版本".to_string()))?;
    let party_revision = db
        .party_revisions()
        .find_by_id(&party_revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("供应商主体版本不存在".to_string()))?;
    Ok(SupplierContext {
        name: party_revision.legal_name,
        commercial_revision_id,
        payment_term_code: commercial.payment_term_snapshot,
    })
}

/// 构造商品行及供给费用行。
fn build_draft_lines(
    submission_id: &PurchaseOrderSubmissionId,
    confirmation_lines: &[ProcurementConfirmationLine],
    sales_lines: &HashMap<String, SalesOrderSubmissionLine>,
    revisions: &HashMap<String, SupplierOfferingRevision>,
    fulfillment_mode: FulfillmentMode,
) -> Result<Vec<PurchaseOrderSubmissionLine>> {
    let mut result = Vec::new();
    for confirmation_line in confirmation_lines {
        result.push(item_line(
            submission_id,
            (result.len() + 1) as u32,
            confirmation_line,
            sales_lines,
        )?);
    }
    let fee_rows = fee_rows(confirmation_lines, revisions, fulfillment_mode)?;
    for (gross, rate) in fee_rows {
        result.push(fee_line(submission_id, (result.len() + 1) as u32, gross, rate)?);
    }
    Ok(result)
}

/// 构造一条商品采购草稿行。
fn item_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
    confirmation_line: &ProcurementConfirmationLine,
    sales_lines: &HashMap<String, SalesOrderSubmissionLine>,
) -> Result<PurchaseOrderSubmissionLine> {
    let sales_line = sales_lines
        .get(&confirmation_line.sales_order_submission_line_id.to_string())
        .ok_or_else(|| Error::NotFound("采购确认引用的销售提交行不存在".to_string()))?;
    let (gross_amount, net_amount, tax_amount) = line_amounts(
        confirmation_line.latest_cost_gross,
        confirmation_line.confirmed_quantity,
        confirmation_line.input_tax_rate,
    );
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(confirmation_line.base.id.clone().into()),
            sku_id: sales_line.sku_id.clone(),
            sku_revision_id: sales_line.sku_revision_id.clone(),
            product_name_snapshot: Some(sales_line.item_name_snapshot.clone()),
            specification_snapshot: Some(
                sales_line
                    .spec_snapshot
                    .clone()
                    .unwrap_or_else(|| "默认规格".to_string()),
            ),
            quantity: Some(confirmation_line.confirmed_quantity),
            base_unit_code: sales_line
                .base_unit_code
                .clone()
                .or_else(|| sales_line.unit_snapshot.clone()),
            unit_cost_gross: Some(confirmation_line.latest_cost_gross),
            gross_amount,
            net_amount,
            tax_amount,
            input_tax_rate: Some(confirmation_line.input_tax_rate),
            expected_delivery_date: Some(confirmation_line.expected_delivery_date),
            sales_order_submission_line_id: Some(confirmation_line.sales_order_submission_line_id.clone()),
            allocated_quantity: Some(confirmation_line.confirmed_quantity),
        },
    )
    .map_err(Into::into)
}

/// 按供给修订去重汇总一次性运费与服务费。
fn fee_rows(
    confirmation_lines: &[ProcurementConfirmationLine],
    revisions: &HashMap<String, SupplierOfferingRevision>,
    fulfillment_mode: FulfillmentMode,
) -> Result<Vec<(Amount, Rate)>> {
    let mut used = HashSet::new();
    let mut result = Vec::new();
    for line in confirmation_lines {
        let revision_id = line
            .supplier_offering_revision_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("采购确认行缺少供给版本".to_string()))?;
        if !used.insert(revision_id.to_string()) {
            continue;
        }
        let revision = revisions
            .get(revision_id.as_ref())
            .ok_or_else(|| Error::NotFound("采购确认引用的供给版本不存在".to_string()))?;
        let mut fee = zero_amount()?;
        if fulfillment_mode == FulfillmentMode::CompanyWarehouse {
            if let Some(freight) = revision.freight_amount {
                fee = fee.checked_add(freight);
            }
        }
        if let Some(service_fee) = revision.service_fee_amount {
            fee = fee.checked_add(service_fee);
        }
        if fee != zero_amount()? {
            result.push((fee, revision.input_tax_rate));
        }
    }
    Ok(result)
}

/// 构造一条物流及服务费用行。
fn fee_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
    gross_amount: Amount,
    input_tax_rate: Rate,
) -> Result<PurchaseOrderSubmissionLine> {
    let tax = Amount::try_from(round_to_cent(
        gross_amount.to_decimal() * input_tax_rate.to_decimal(),
    ))?;
    let net = gross_amount.checked_sub(tax);
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::LogisticsFee,
            procurement_confirmation_line_id: None,
            sku_id: None,
            sku_revision_id: None,
            product_name_snapshot: None,
            specification_snapshot: None,
            quantity: None,
            base_unit_code: None,
            unit_cost_gross: None,
            gross_amount,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(input_tax_rate),
            expected_delivery_date: None,
            sales_order_submission_line_id: None,
            allocated_quantity: None,
        },
    )
    .map_err(Into::into)
}

/// 汇总草稿行金额三元组。
fn line_totals(lines: &[PurchaseOrderSubmissionLine]) -> Result<(Amount, Amount, Amount)> {
    let zero = zero_amount()?;
    Ok(lines.iter().fold((zero, zero, zero), |totals, line| {
        (
            totals.0.checked_add(line.gross_amount),
            totals.1.checked_add(line.net_amount),
            totals.2.checked_add(line.tax_amount),
        )
    }))
}

/// 采购履约方式对应采购类型。
fn purchase_type_from_mode(mode: FulfillmentMode) -> PurchaseType {
    match mode {
        FulfillmentMode::CompanyWarehouse | FulfillmentMode::SupplierDirect => PurchaseType::Physical,
        FulfillmentMode::ElectronicDelivery => PurchaseType::Virtual,
        FulfillmentMode::OfflineService => PurchaseType::Service,
    }
}

/// 构造付款条件门禁快照。
fn payment_term_snapshot(payment_term_code: &str) -> Result<PaymentTermSnapshot> {
    let prepay_gate = payment_term_code.trim().to_uppercase().starts_with("PREPAY");
    PaymentTermSnapshot::new(payment_term_code.to_string(), prepay_gate, None, None).map_err(Into::into)
}

/// 构造金额零值。
fn zero_amount() -> Result<Amount> {
    Amount::from_str("0.00").map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::{group_confirmation_lines, purchase_type_from_mode};
    use entities::purchase_order::PurchaseType;
    use entities::sales_review::FulfillmentMode;

    #[test]
    fn purchase_type_follows_fulfillment_mode() {
        assert_eq!(
            purchase_type_from_mode(FulfillmentMode::SupplierDirect),
            PurchaseType::Physical
        );
        assert_eq!(
            purchase_type_from_mode(FulfillmentMode::ElectronicDelivery),
            PurchaseType::Virtual
        );
    }

    #[test]
    fn empty_confirmation_has_no_groups() {
        assert!(group_confirmation_lines(&[]).is_empty());
    }
}
