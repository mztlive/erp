//! 采购草稿校验、读取、替换计划与单域事务内写入。
use super::line_input::{build_submission_lines, compute_request_totals, to_line_inputs};
use crate::dto::purchase_order::SavePurchaseOrderLine;
use crate::entity::purchase_order::{
    DraftLineEditViolation, PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission,
    PurchaseOrderSubmissionData, PurchaseOrderSubmissionLine, SubmissionStatus,
};
use crate::repository::PurchaseOrderExt;
use crate::{Error, Result};
use erp_core::ids::PurchaseOrderSubmissionId;
use erp_core::money::Amount;
use id_generator::next_id;
use persistence_core::Executor;
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
/// # 参数
/// * `db` - MongoDB 数据库
/// * `purchase_order_id` - 采购单 ID
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回存在的采购单。
///
/// # 错误
/// 采购单不存在或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 本函数不执行版本或状态校验，避免打乱调用方的安全校验顺序。
pub async fn load_purchase_order(
    db: &mongodb::Database,
    purchase_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<PurchaseOrder> {
    db.purchase_orders()
        .find_by_id(purchase_order_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))
}
/// 校验采购草稿保存目标的创建人、版本和状态。
///
/// # 参数
/// * `created_by` - 采购单创建人 ID
/// * `current_version` - 采购单当前乐观锁版本
/// * `status` - 采购单当前状态
/// * `expected_lock_version` - 客户端期望版本
/// * `actor_id` - 当前操作人 ID
///
/// # 返回
/// 创建人、版本和草稿状态全部匹配时返回 `Ok(())`。
///
/// # 错误
/// 非创建人统一返回不存在；其后才允许返回版本或状态错误。
///
/// # 关键业务约束
/// 创建人校验必须先于版本和状态，禁止向其他账号泄露资源版本或生命周期状态。
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
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    if status != PurchaseOrderStatus::Draft {
        return Err(Error::BusinessLogicError(
            "只有草稿状态的采购单可以编辑".to_string(),
        ));
    }
    Ok(())
}
/// 加载当前可编辑草稿提交及其完整行。
///
/// # 参数
/// * `db` - MongoDB 数据库
/// * `order` - 已校验可编辑的采购单
/// * `executor` - 数据访问执行器
///
/// # 返回
/// 返回草稿提交头和该提交的完整行。
///
/// # 错误
/// 当前草稿引用缺失、提交不存在、提交已冻结或仓储读取失败时返回错误。
///
/// # 关键业务约束
/// 只允许替换状态仍为 `Draft` 的当前提交。
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
/// # 参数
/// * `order` - 当前采购单
/// * `old_draft` - 当前草稿提交
/// * `lines` - 已通过草稿编辑校验的完整请求行
///
/// # 返回
/// 返回已通过实体校验的新提交、完整行和服务端金额。
///
/// # 错误
/// 金额计算、提交构造或草稿行构造失败时返回错误。
///
/// # 关键业务约束
/// 供应商、采购类型、履约责任和供应商快照均继承原草稿，不接受客户端改写。
pub fn build_draft_replacement(
    order: &PurchaseOrder,
    old_draft: &PurchaseOrderSubmission,
    lines: &[SavePurchaseOrderLine],
) -> Result<DraftReplacement> {
    let inputs = to_line_inputs(lines)?;
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
            payment_term_snapshot: old_draft.payment_term_snapshot.clone(),
            gross_amount: gross,
            net_amount: net,
            tax_amount: tax,
        },
    )?;
    let lines = build_submission_lines(&submission.base.id.clone().into(), &inputs)?;
    Ok(DraftReplacement {
        submission,
        lines,
        gross,
        net,
        tax,
    })
}
/// 把领域草稿编辑校验失败映射为稳定 HTTP 语义。
///
/// # 参数
/// * `violation` - 领域校验失败原因
///
/// # 返回
/// 返回校验、业务或冲突错误，文案与既有实现一致。
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
        DraftLineEditViolation::SourceLineRemoved | DraftLineEditViolation::ExceedsAvailableQuantity => {
            Error::ConflictError(violation.to_string())
        }
        DraftLineEditViolation::MissingSalesStableLine
        | DraftLineEditViolation::MissingOriginalAllocatedQuantity => {
            Error::BusinessLogicError(violation.to_string())
        }
    }
}
/// 在调用方事务内依次替代旧草稿、创建新提交与行、更新采购单；失败停止并保留原错误。
pub async fn persist_replacement(
    db: &mongodb::Database,
    order: &mut PurchaseOrder,
    old_draft: &mut PurchaseOrderSubmission,
    replacement: &DraftReplacement,
    session: &mut dyn Executor,
) -> Result<()> {
    db.purchase_order_submissions().update(old_draft, session).await?;
    db.purchase_order_submissions()
        .create(&replacement.submission, session)
        .await?;
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
        SavePurchaseOrderDraftRequest {
            expected_lock_version: 3,
            payment_term_code: Some(" NET-30 ".to_string()),
            lines: vec![SavePurchaseOrderLine {
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
            }],
            line_patches: vec![],
            idempotency_key: "save-key-1".to_string(),
        }
    }
    /// 验证非创建人无法观察目标采购单的版本或状态错误。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 创建人校验未优先返回统一不存在错误时测试失败。
    #[test]
    fn save_target_checks_creator_before_version_and_status() {
        let mut request = save_request("1");
        request.expected_lock_version = 1;
        let status = PurchaseOrderStatus::Voided;
        let result = ensure_save_target("creator-1", 99, status, request.expected_lock_version, "actor-2");

        assert!(matches!(result, Err(Error::NotFound(_))));
    }
}
