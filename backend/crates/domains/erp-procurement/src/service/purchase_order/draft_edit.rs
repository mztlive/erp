//! 采购草稿校验、读取、替换计划与单域事务内写入。
use erp_core::ids::PurchaseOrderSubmissionId;
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;

use super::line_input::{build_submission_lines, compute_request_totals};
use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{
    DraftLineEditViolation, PaymentSubjectSnapshot, PurchaseOrder, PurchaseOrderStatus,
    PurchaseOrderSubmission, PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, SubmissionStatus,
    inherit_submission_sources,
};
use crate::repository::PurchaseOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};
/// 待写入的新采购草稿提交及金额。
pub struct DraftReplacement {
    /// 新草稿提交头。
    pub submission: PurchaseOrderSubmission,
    /// 新草稿提交行。
    pub lines: Vec<PurchaseOrderSubmissionLine>,
    /// 含税金额。
    pub gross: Amount,
    /// 不含税金额。
    pub net: Amount,
    /// 税额。
    pub tax: Amount,
}
/// 按 ID 加载采购单。
///
/// 本函数不执行版本或状态校验，避免打乱调用方的安全校验顺序。
///
/// 返回存在的采购单。采购单不存在或仓储读取失败时返回错误。
///
/// 沿用调用方的执行器，不另开事务。
pub async fn load_purchase_order(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    super::shared::load_order_by_id(db, purchase_order_id, executor).await
}
/// 校验采购草稿保存目标的创建人、版本和状态。
///
/// 创建人校验必须先于版本和状态，禁止向其他账号泄露资源版本或生命周期状态。
///
/// 创建人、版本和草稿状态全部匹配时返回 `Ok(())`。非创建人统一返回不存在；其后才允许返回版本或状态错误。
///
/// - `current_version`：采购单当前乐观锁版本。
/// - `status`：采购单当前状态。
pub fn ensure_save_target(
    created_by: &str,
    current_version: u64,
    status: PurchaseOrderStatus,
    expected_lock_version: u64,
    actor_id: &str,
) -> Result<()> {
    if created_by != actor_id {
        return Err(Error::NotFound("采购单不存在或不可编辑".to_string()));
    }
    if current_version != expected_lock_version {
        return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
    }
    if !matches!(status, PurchaseOrderStatus::Draft | PurchaseOrderStatus::Rejected) {
        return Err(Error::BusinessLogicError("只有草稿状态的采购单可以编辑".to_string()));
    }
    Ok(())
}
/// 保存采购草稿前，沿原执行器按当前提交指针取得草稿头与完整明细。
/// 指针或草稿缺失、提交已冻结或查询失败时拒绝，不能以历史提交替代。
pub async fn load_current_draft(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
    let draft_id = order
        .current_submission_id
        .as_ref()
        .map(ToString::to_string)
        .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
    let draft_id = PurchaseOrderSubmissionId::new(draft_id);
    let draft = db
        .purchase_order_submissions()
        .find_by_id(draft_id.as_ref(), executor)
        .await?
        .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
    if draft.status != SubmissionStatus::Draft {
        return Err(Error::BusinessLogicError("草稿提交已冻结，不能保存".to_string()));
    }

    let lines = db
        .purchase_order_submission_lines()
        .find_lines_by_submission_ids(std::slice::from_ref(&draft_id), executor)
        .await?;
    Ok((draft, lines))
}
/// 构造完整替换后的新草稿提交、行和金额。
///
/// 供应商、采购类型、履约责任和供应商快照均继承原草稿，不接受客户端改写。
///
/// 返回已通过实体校验的新提交、完整行和服务端金额。金额计算、提交构造或草稿行构造失败时返回错误。
///
/// - `order`：当前采购单。
/// - `old_draft`：当前草稿提交。
/// - `old_lines`：当前服务端完整来源行。
/// - `payment_subject`：新草稿的付款主体；按新销售版本刷新时替换销售来源。
pub fn build_draft_replacement(
    order: &PurchaseOrder,
    old_draft: &PurchaseOrderSubmission,
    old_lines: &[PurchaseOrderSubmissionLine],
    lines: &[SavePurchaseOrderLine],
    payment_subject: PaymentSubjectSnapshot,
) -> Result<DraftReplacement> {
    let mut inputs = SavePurchaseOrderLine::to_line_inputs(lines)?;
    inherit_submission_sources(&mut inputs, old_lines)?;
    let (gross, net, tax) = compute_request_totals(&inputs)?;

    let submission = PurchaseOrderSubmission::new(
        PurchaseOrderSubmissionId::new(next_id()),
        PurchaseOrderSubmissionData {
            purchase_order_id: order.base.id.clone().into(),
            submission_no: format!("DRAFT-{}", &next_id()[..8]),
            supplier_id: old_draft.supplier_id.clone(),
            purchase_type: old_draft.purchase_type,
            fulfillment_responsibility: old_draft.fulfillment_responsibility,
            supplier_revision_id: old_draft.supplier_revision_id.clone(),
            supplier_snapshot: old_draft.supplier_snapshot.clone(),
            payment_subject,
            payment_term_snapshot: old_draft.payment_term_snapshot.clone(),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
        },
    )?;

    let lines = build_submission_lines(&submission.base.id.clone().into(), &inputs)?;
    Ok(DraftReplacement { submission, lines, gross, net, tax })
}
/// 把领域草稿编辑校验失败映射为稳定 HTTP 语义。
///
/// 返回校验、业务或冲突错误，文案与既有实现一致。本函数构造错误值，不产生 `Result`。
///
/// `violation` 为领域校验失败原因。
pub fn map_draft_edit_violation(violation: DraftLineEditViolation) -> Error {
    match violation {
        DraftLineEditViolation::SourceLineCountChanged
        | DraftLineEditViolation::DuplicateSalesLine
        | DraftLineEditViolation::RewrittenSalesLine
        | DraftLineEditViolation::RewrittenSourceReference
        | DraftLineEditViolation::MissingSalesLineId
        | DraftLineEditViolation::MissingQuantity
        | DraftLineEditViolation::MissingAllocatedQuantity
        | DraftLineEditViolation::InvalidQuantity(_)
        | DraftLineEditViolation::QuantityAllocationMismatch
        | DraftLineEditViolation::PaymentTermChanged => Error::ValidationError(violation.to_string()),
        DraftLineEditViolation::SourceLineRemoved
        | DraftLineEditViolation::ExceedsAvailableQuantity
        | DraftLineEditViolation::RemovedLineStillPurchasable
        | DraftLineEditViolation::NoPurchasableLine => Error::ConflictError(violation.to_string()),
        DraftLineEditViolation::MissingSalesStableLine
        | DraftLineEditViolation::MissingOriginalAllocatedQuantity => {
            Error::BusinessLogicError(violation.to_string())
        },
    }
}
/// 在调用方事务内依次替代旧草稿、创建新提交与行、更新采购单；失败停止并保留原错误。任一步仓储写入或 CAS 失败时返回对应错误，不再继续后续写入。
///
/// `replacement` 为新提交、新行和金额。
pub async fn persist_replacement(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    old_draft: &mut PurchaseOrderSubmission,
    replacement: &DraftReplacement,
    session: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order_submissions().update(old_draft, session).await?;
    db.purchase_order_submissions().create(&replacement.submission, session).await?;
    for line in &replacement.lines {
        db.purchase_order_submission_lines().create(line, session).await?;
    }

    db.purchase_orders().update(order, session).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::dto::purchase_order::SavePurchaseOrderDraftRequest;
    use crate::entity::purchase_order::PurchaseLineType;
    /// 构造最小保存草稿请求。
    ///
    /// # 参数
    /// * `quantity` - 商品行采购与分配数量
    ///
    /// # 返回
    /// 返回用于请求指纹测试的完整 DTO。
    ///
    /// # 错误
    /// 无。
    fn save_request(quantity: &str) -> SavePurchaseOrderDraftRequest {
        SavePurchaseOrderDraftRequest::new("save-key-1")
            .with_expected_lock_version(3)
            .with_payment_term_code(" NET-30 ")
            .with_lines(vec![SavePurchaseOrderLine {
                delivery_allocations: None,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: None,
                sku_id: Some("sku-1".to_string()),
                sku_revision_id: Some("sku-rev-1".to_string()),
                product_name: Some("产品".to_string()),
                specification: None,
                quantity: Some(quantity.to_string()),
                base_unit_code: Some("EA".to_string()),
                unit_cost_gross: Some("10".to_string()),
                input_tax_rate: Some("0.13".to_string()),
                expected_delivery_date: Some("2026-08-25".to_string()),
                sales_order_line_id: Some("sales-line-1".to_string()),
                sales_order_revision_line_id: Some("sales-revision-line-1".to_string()),
                sales_order_submission_line_id: Some("sales-submission-line-1".to_string()),
                allocated_quantity: Some(quantity.to_string()),
                gross_amount: None,
                expense: None,
            }])
    }
    // 验证非创建人无法观察目标采购单的版本或状态错误。
    #[test]
    fn save_target_checks_creator_before_version_and_status() {
        let mut request = save_request("1");
        request.expected_lock_version = 1;
        let status = PurchaseOrderStatus::Voided;
        let result = ensure_save_target("creator-1", 99, status, request.expected_lock_version, "actor-2");
        assert!(matches!(result, Err(Error::NotFound(_))));
    }
}
