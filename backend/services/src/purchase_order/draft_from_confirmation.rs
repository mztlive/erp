//! 从已通过的采购确认分行拆单并写入采购草稿。
//!
//! 供 W07 确认通过事务与 W08 依据建单复用；写入必须走调用方传入的执行器，
//! 以便确认通过与建草稿落在同一 Mongo 事务内。

use std::collections::{BTreeMap, HashMap};

use database::{
    AccessControlExt, Executor, PartyExt, PurchaseOrderExt, SalesOrderExt, SupplierExt, WorkItemExt,
};
use entities::common::time::Instant;
use entities::ids::{
    PurchaseOrderId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId,
    SupplierAccountId,
};
use entities::money::line_amounts;
use entities::purchase_order::{
    FulfillmentResponsibility, PaymentTermSnapshot, PurchaseLineType, PurchaseOrder, PurchaseOrderData,
    PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine,
    PurchaseOrderSubmissionLineData, PurchaseType, SupplierSnapshot,
};
use entities::sales_review::{FulfillmentMode, ProcurementConfirmationLine};
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;

use super::shared::{fulfillment_from_mode, today_stamp, zero_amount};
use super::submission::finance_review_work_item;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

/// 确认拆单后落库（或幂等复用）的采购草稿身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct CreatedPurchaseDraft {
    /// 采购单主键。
    pub purchase_order_id: String,
    /// 采购单号。
    pub purchase_no: String,
    /// 乐观锁版本。
    pub lock_version: u64,
    /// 是否复用已有非终态采购单。
    pub replayed: bool,
}

/// 按供应商 + 履约方式拆单，并在调用方事务内写入采购草稿。
///
/// # 参数
/// * `db` - 数据库
/// * `sales_order_id` - 来源销售单
/// * `lines` - 已通过的采购确认分行
/// * `actor` - 审计操作人
/// * `executor` - 由调用方传入的事务或非事务执行器
///
/// # 返回
/// 返回每个拆单维度对应的采购草稿身份，顺序稳定。
///
/// # 错误
/// 供应商/商务资料缺失、行快照无法构造或仓储写入失败时返回错误。
///
/// # 约束
/// 同一销售单 + 供应商 + 履约责任已存在草稿或待审采购单时复用，不重复创建。
pub(crate) async fn create_drafts_from_confirmation_lines(
    db: &Database,
    sales_order_id: &SalesOrderId,
    lines: &[ProcurementConfirmationLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<Vec<CreatedPurchaseDraft>> {
    if lines.is_empty() {
        return Err(Error::BusinessLogicError("创建依据没有可拆入的分行".to_string()));
    }

    let mut drafts = Vec::new();
    for indexes in group_line_indexes(lines) {
        let group: Vec<&ProcurementConfirmationLine> = indexes.iter().map(|index| &lines[*index]).collect();
        drafts.push(persist_split_group(db, sales_order_id, &group, actor, executor).await?);
    }
    Ok(drafts)
}

/// 按供应商与履约方式对确认行分组，返回稳定顺序的行下标组。
///
/// # 参数
/// * `lines` - 采购确认分行
///
/// # 返回
/// 返回分组后的行下标；组顺序按供应商 ID、履约方式代码排序。
///
/// # 错误
/// 无。
fn group_line_indexes(lines: &[ProcurementConfirmationLine]) -> Vec<Vec<usize>> {
    let mut groups: BTreeMap<(String, String), Vec<usize>> = BTreeMap::new();
    for (index, line) in lines.iter().enumerate() {
        groups
            .entry((
                line.supplier_id.to_string(),
                line.fulfillment_mode.as_str().to_string(),
            ))
            .or_default()
            .push(index);
    }
    groups.into_values().collect()
}

/// 把一个拆单分组写成采购草稿，或复用已有非终态单据。
///
/// # 参数
/// * `db` - 数据库
/// * `sales_order_id` - 来源销售单
/// * `lines` - 同一供应商与履约方式下的确认分行
/// * `actor` - 审计操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回该分组对应的采购草稿身份。
///
/// # 错误
/// 供应商资料缺失或写入失败时返回错误。
async fn persist_split_group(
    db: &Database,
    sales_order_id: &SalesOrderId,
    lines: &[&ProcurementConfirmationLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<CreatedPurchaseDraft> {
    let first = lines
        .first()
        .copied()
        .ok_or_else(|| Error::BusinessLogicError("拆单分组不能为空".to_string()))?;
    let fulfillment = fulfillment_from_mode(first.fulfillment_mode);
    if let Some(existing) =
        find_existing_draft(db, sales_order_id, &first.supplier_id, fulfillment, executor).await?
    {
        return Ok(existing);
    }

    let mut prepared = prepare_draft(db, sales_order_id, lines, fulfillment, actor, executor).await?;
    formalize_prepared_order(&mut prepared, actor)?;
    write_prepared_draft(db, &prepared, actor, executor).await?;
    Ok(prepared.as_created())
}

/// 查找同一拆单维度上已存在的草稿或待审采购单。
///
/// # 参数
/// * `db` - 数据库
/// * `sales_order_id` - 来源销售单
/// * `supplier_id` - 供应商
/// * `fulfillment` - 履约责任
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 找到则返回可复用草稿身份，否则返回 `None`。
///
/// # 错误
/// 查询失败时返回仓储错误。
async fn find_existing_draft(
    db: &Database,
    sales_order_id: &SalesOrderId,
    supplier_id: &SupplierAccountId,
    fulfillment: FulfillmentResponsibility,
    executor: &mut dyn Executor,
) -> Result<Option<CreatedPurchaseDraft>> {
    let existing = db
        .purchase_orders()
        .find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "supplier_id": supplier_id.to_string(),
                "fulfillment_responsibility": fulfillment.as_str(),
                "status": { "$in": [
                    PurchaseOrderStatus::Draft.as_str(),
                    PurchaseOrderStatus::PendingFinanceReview.as_str(),
                ]},
            },
            executor,
        )
        .await?;
    Ok(existing.map(|order| CreatedPurchaseDraft {
        purchase_order_id: order.base.id.clone(),
        purchase_no: order.purchase_no.clone(),
        lock_version: order.base.version,
        replayed: true,
    }))
}

/// 待写入的采购草稿聚合。
struct PreparedDraft {
    order: PurchaseOrder,
    submission: PurchaseOrderSubmission,
    lines: Vec<PurchaseOrderSubmissionLine>,
}

impl PreparedDraft {
    /// 转成对外采购单身份（新建，非复用）。
    ///
    /// # 返回
    /// 返回新建草稿身份。
    ///
    /// # 错误
    /// 无。
    fn as_created(&self) -> CreatedPurchaseDraft {
        CreatedPurchaseDraft {
            purchase_order_id: self.order.base.id.clone(),
            purchase_no: self.order.purchase_no.clone(),
            lock_version: self.order.base.version,
            replayed: false,
        }
    }
}

/// 读取供应商资料并构造尚未落库的采购草稿聚合。
///
/// # 参数
/// * `db` - 数据库
/// * `sales_order_id` - 来源销售单
/// * `lines` - 同一拆单维度的确认分行
/// * `fulfillment` - 履约责任
/// * `actor` - 审计操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回可写入的草稿聚合。
///
/// # 错误
/// 供应商、商务版本或销售行快照缺失时返回错误。
async fn prepare_draft(
    db: &Database,
    sales_order_id: &SalesOrderId,
    lines: &[&ProcurementConfirmationLine],
    fulfillment: FulfillmentResponsibility,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<PreparedDraft> {
    let first = lines[0];
    let supplier_name = resolve_supplier_name(db, &first.supplier_id, executor)
        .await?
        .unwrap_or_else(|| first.supplier_id.to_string());
    let payment_term_code = resolve_payment_term_code(db, &first.supplier_id, executor).await?;
    let purchase_type = purchase_type_from_mode(first.fulfillment_mode);
    let order_id = PurchaseOrderId::new(next_id());
    let mut order = PurchaseOrder::new(
        order_id.clone(),
        PurchaseOrderData {
            purchase_no: format!("PO-{}-{}", today_stamp(), &order_id.to_string()[..6]),
            sales_order_id: sales_order_id.clone(),
            supplier_id: first.supplier_id.clone(),
            purchase_type,
            payment_term_code: payment_term_code.clone(),
            fulfillment_responsibility: fulfillment,
        },
        actor.id(),
    )?;
    let submission = build_draft_submission(
        db,
        &order_id,
        first,
        purchase_type,
        fulfillment,
        &supplier_name,
        &payment_term_code,
        lines,
        executor,
    )
    .await?;
    order.current_submission_id = Some(submission.base.id.clone());
    let submission_lines = build_draft_lines(db, &submission.base.id, lines, executor).await?;
    Ok(PreparedDraft {
        order,
        submission,
        lines: submission_lines,
    })
}

/// 把已构造的采购草稿、提交和明细写入当前执行器。
///
/// # 参数
/// * `db` - 数据库
/// * `prepared` - 待写入聚合
/// * `actor` - 审计操作人
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 写入成功返回 `Ok(())`。
///
/// # 错误
/// 仓储写入或审计失败时返回错误。
async fn write_prepared_draft(
    db: &Database,
    prepared: &PreparedDraft,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let audit = actor.clone().resource_log(
        "purchase_order.create",
        "purchase_order",
        prepared.order.base.id.clone(),
    )?;
    db.purchase_orders().create(&prepared.order, executor).await?;
    db.purchase_order_submissions()
        .create(&prepared.submission, executor)
        .await?;
    for line in &prepared.lines {
        db.purchase_order_submission_lines()
            .create(line, executor)
            .await?;
    }
    let work_item = finance_review_work_item(&prepared.order, &prepared.submission)?;
    db.work_items().create(&work_item, executor).await?;
    db.audit_logs().create(&audit, executor).await?;
    Ok(())
}

/// 把待写入的采购草稿推进为正式待财务审核采购单。
///
/// # 参数
/// * `prepared` - 待写入聚合
/// * `actor` - 提交人
///
/// # 返回
/// 推进成功返回 `Ok(())`。
///
/// # 错误
/// 状态不是草稿或提交字段非法时返回错误。
fn formalize_prepared_order(prepared: &mut PreparedDraft, actor: &AuditActor) -> Result<()> {
    prepared.submission.submission_no = "SUB-000001".to_string();
    prepared.submission.submit(Instant::now(), actor.id())?;
    prepared
        .order
        .submit_for_review(prepared.submission.base.id.clone(), actor.id())?;
    Ok(())
}

/// 由履约方式推断采购类型。
///
/// # 参数
/// * `mode` - 确认行履约方式
///
/// # 返回
/// 入仓/直发为实物，电子交付为虚拟，线下服务为服务。
///
/// # 错误
/// 无。
fn purchase_type_from_mode(mode: FulfillmentMode) -> PurchaseType {
    match mode {
        FulfillmentMode::CompanyWarehouse | FulfillmentMode::SupplierDirect => PurchaseType::Physical,
        FulfillmentMode::ElectronicDelivery => PurchaseType::Virtual,
        FulfillmentMode::OfflineService => PurchaseType::Service,
    }
}

/// 读取供应商主体法定名称。
///
/// # 参数
/// * `db` - 数据库
/// * `supplier_id` - 供应商账号
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 找到主体修订时返回法定名称，否则 `None`。
///
/// # 错误
/// 查询失败时返回仓储错误。
async fn resolve_supplier_name(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<Option<String>> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok(None);
    };
    let Some(party) = db.parties().find_by_id(&supplier.party_id, executor).await? else {
        return Ok(None);
    };
    let Some(revision_id) = party.stable.current_revision_id else {
        return Ok(None);
    };
    let revision = db.party_revisions().find_by_id(&revision_id, executor).await?;
    Ok(revision.map(|revision| revision.legal_name))
}

/// 读取供应商当前商务结算版本上的付款条件，缺省 `NET-30`。
///
/// # 参数
/// * `db` - 数据库
/// * `supplier_id` - 供应商账号
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回付款条件代码。
///
/// # 错误
/// 查询失败时返回仓储错误。
async fn resolve_payment_term_code(
    db: &Database,
    supplier_id: &SupplierAccountId,
    executor: &mut dyn Executor,
) -> Result<String> {
    let Some(supplier) = db.supplier_accounts().find_by_id(supplier_id, executor).await? else {
        return Ok("NET-30".to_string());
    };
    let Some(revision_id) = supplier.current_commercial_profile_revision_id else {
        return Ok("NET-30".to_string());
    };
    let revision = db
        .supplier_commercial_profile_revisions()
        .find_by_id(&revision_id, executor)
        .await?;
    Ok(revision
        .map(|revision| revision.payment_term_snapshot)
        .unwrap_or_else(|| "NET-30".to_string()))
}

/// 构造采购草稿提交头。
///
/// # 参数
/// * `db` - 数据库
/// * `order_id` - 采购单 ID
/// * `first` - 分组首行，用于供应商与履约
/// * `purchase_type` - 采购类型
/// * `fulfillment` - 履约责任
/// * `supplier_name` - 供应商名称快照
/// * `payment_term_code` - 付款条件代码
/// * `lines` - 确认分行
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回未落库的草稿提交。
///
/// # 错误
/// 供应商缺少商务结算版本或快照非法时返回错误。
#[allow(clippy::too_many_arguments)]
async fn build_draft_submission(
    db: &Database,
    order_id: &PurchaseOrderId,
    first: &ProcurementConfirmationLine,
    purchase_type: PurchaseType,
    fulfillment: FulfillmentResponsibility,
    supplier_name: &str,
    payment_term_code: &str,
    lines: &[&ProcurementConfirmationLine],
    executor: &mut dyn Executor,
) -> Result<PurchaseOrderSubmission> {
    let supplier = db
        .supplier_accounts()
        .find_by_id(&first.supplier_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;
    let revision_id = supplier
        .current_commercial_profile_revision_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("供应商缺少商务结算版本".to_string()))?;
    let (gross, net, tax) = sum_confirmation_amounts(lines);
    PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order_id.clone(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: first.supplier_id.clone(),
            purchase_type,
            fulfillment_responsibility: fulfillment,
            supplier_revision_id: revision_id,
            supplier_snapshot: SupplierSnapshot::new(supplier_name.to_string())?,
            payment_term_snapshot: payment_term_snapshot(payment_term_code)?,
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
        },
    )
    .map_err(Into::into)
}

/// 汇总确认分行的含税/不含税/税额。
///
/// # 参数
/// * `lines` - 确认分行
///
/// # 返回
/// 返回 `(gross, net, tax)`。
///
/// # 错误
/// 无。
fn sum_confirmation_amounts(
    lines: &[&ProcurementConfirmationLine],
) -> (
    entities::money::Amount,
    entities::money::Amount,
    entities::money::Amount,
) {
    lines.iter().fold(
        (zero_amount(), zero_amount(), zero_amount()),
        |(gross, net, tax), line| {
            let (line_gross, line_net, line_tax) = line_amounts(
                line.latest_cost_gross,
                line.confirmed_quantity,
                line.input_tax_rate,
            );
            (
                gross.checked_add(line_gross),
                net.checked_add(line_net),
                tax.checked_add(line_tax),
            )
        },
    )
}

/// 构造付款条件快照。
///
/// # 参数
/// * `payment_term_code` - 付款条件代码
///
/// # 返回
/// 返回快照；`PREPAY` 前缀视为先款后货。
///
/// # 错误
/// 代码非法时返回校验错误。
fn payment_term_snapshot(payment_term_code: &str) -> Result<PaymentTermSnapshot> {
    let prepay_gate = payment_term_code.trim().to_uppercase().starts_with("PREPAY");
    PaymentTermSnapshot::new(payment_term_code.to_string(), prepay_gate, None, None).map_err(Into::into)
}

/// 从确认分行构造采购草稿明细，商品快照取自销售提交行。
///
/// # 参数
/// * `db` - 数据库
/// * `submission_id` - 草稿提交 ID
/// * `lines` - 确认分行
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回未落库的草稿明细。
///
/// # 错误
/// 明细构造失败或销售行查询失败时返回错误。
async fn build_draft_lines(
    db: &Database,
    submission_id: &str,
    lines: &[&ProcurementConfirmationLine],
    executor: &mut dyn Executor,
) -> Result<Vec<PurchaseOrderSubmissionLine>> {
    let sales_line_ids: Vec<String> = lines
        .iter()
        .map(|line| line.sales_order_submission_line_id.to_string())
        .collect();
    let sales_lines = db
        .sales_order_submission_lines()
        .find_many(doc! { "id": { "$in": sales_line_ids } }, executor)
        .await?;
    let sales_by_id: HashMap<String, entities::sales_order::SalesOrderSubmissionLine> = sales_lines
        .into_iter()
        .map(|line| (line.base.id.clone(), line))
        .collect();
    let mut result = Vec::with_capacity(lines.len());
    for (index, line) in lines.iter().enumerate() {
        result.push(build_one_draft_line(submission_id, index, line, &sales_by_id)?);
    }
    Ok(result)
}

/// 构造单条采购草稿明细。
///
/// # 参数
/// * `submission_id` - 草稿提交 ID
/// * `index` - 分组内行序（0 起）
/// * `line` - 确认分行
/// * `sales_by_id` - 销售提交行快照
///
/// # 返回
/// 返回未落库的草稿明细。
///
/// # 错误
/// 明细字段非法时返回校验错误。
fn build_one_draft_line(
    submission_id: &str,
    index: usize,
    line: &ProcurementConfirmationLine,
    sales_by_id: &HashMap<String, entities::sales_order::SalesOrderSubmissionLine>,
) -> Result<PurchaseOrderSubmissionLine> {
    let (gross, net, tax) = line_amounts(
        line.latest_cost_gross,
        line.confirmed_quantity,
        line.input_tax_rate,
    );
    let sales_line = sales_by_id.get(&line.sales_order_submission_line_id.to_string());
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: PurchaseOrderSubmissionId::new(submission_id.to_string()),
            line_no: (index + 1) as u32,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(line.base.id.clone().into()),
            sku_id: sales_line.and_then(|sales| sales.sku_id.clone()),
            sku_revision_id: sales_line.and_then(|sales| sales.sku_revision_id.clone()),
            product_name_snapshot: sales_line.map(|sales| sales.item_name_snapshot.clone()),
            specification_snapshot: sales_line.and_then(|sales| sales.spec_snapshot.clone()),
            quantity: Some(line.confirmed_quantity),
            base_unit_code: sales_line.and_then(|sales| sales.unit_snapshot.clone()),
            unit_cost_gross: Some(line.latest_cost_gross),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
            input_tax_rate: Some(line.input_tax_rate),
            expected_delivery_date: Some(line.expected_delivery_date),
            sales_order_submission_line_id: Some(line.sales_order_submission_line_id.clone()),
            allocated_quantity: Some(line.confirmed_quantity),
        },
    )
    .map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use super::purchase_type_from_mode;
    use entities::purchase_order::PurchaseType;
    use entities::sales_review::FulfillmentMode;

    #[test]
    fn purchase_type_follows_fulfillment_mode() {
        assert_eq!(
            purchase_type_from_mode(FulfillmentMode::CompanyWarehouse),
            PurchaseType::Physical
        );
        assert_eq!(
            purchase_type_from_mode(FulfillmentMode::ElectronicDelivery),
            PurchaseType::Virtual
        );
        assert_eq!(
            purchase_type_from_mode(FulfillmentMode::OfflineService),
            PurchaseType::Service
        );
    }
}
