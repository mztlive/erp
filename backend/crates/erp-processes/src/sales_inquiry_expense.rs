//! 询价费用来源的原事务步骤；采购与财务消费同一跨版本额度。

use std::collections::HashSet;

use application_core::{AuditActor, CommandIdentity};
use erp_core::common::time::Instant;
use erp_core::ids::PurchaseOrderId;
use erp_core::money::{Amount, Rate};
use erp_finance::dto::inquiry_fee::InquiryFeeReference;
use erp_finance::entity::cost::CostEntry;
use erp_finance::repository::PayableExt;
use erp_finance::repository::prelude::*;
use erp_procurement::entity::purchase_order::{
    InquiryExpenseSource, PurchaseExpenseDetail, PurchaseOrder, PurchaseOrderSubmissionLine,
};
use erp_sales::entity::sales_inquiry::{
    InquiryFee, InquiryFeeCategory, InquiryFeeReservation, InquiryReservationState, SalesInquiry,
};
use erp_sales::service::sales_inquiry::SalesInquiryService;
use erp_supply::inquiry::{InquiryFeeBasis, InquiryFeeBasisData, InquirySupplyService};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 核验收款依据后登记采购费用来源占用。
///
/// # 参数
/// `db`、`executor` 为原建单事务；其余参数为已验证采购与费用行。
/// # 返回
/// 来源为空时不写入；存在来源时保存跨轮次占用。
/// # 错误
/// 来源失效、销售归属、费用条件、收款依据或额度不符时拒绝。
pub(crate) async fn reserve_purchase(
    db: &Database,
    order: &PurchaseOrder,
    lines: &[PurchaseOrderSubmissionLine],
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = SalesInquiryService::new(db.clone());
    for line in lines {
        let Some(detail) = line.expense.as_ref() else { continue };
        let Some(source) = detail.inquiry_source.as_ref() else { continue };
        let source = source.normalized()?;
        let mut inquiry = service.require(&source.inquiry_id, executor).await?;
        let fee = require_fee(&inquiry, &source, line.gross_amount, line.input_tax_rate)?;
        if !fee.requires_erp_payment {
            return Err(Error::BusinessLogicError("本费用应按财务凭证登记，不生成费用采购".into()));
        }
        if category_code(fee) != detail.category.as_str() {
            return Err(Error::ValidationError("费用类别与已接受询价结果不一致".into()));
        }
        confirm_conversion_payee(db, &inquiry, &source, order, actor.id(), executor).await?;
        let reservation = reservation(
            &source,
            order.sales_order_id.as_ref(),
            &order.base.id,
            &detail.expense_line_id,
            line.gross_amount,
        );
        inquiry.reserve_fee(reservation, Instant::now())?;
        service.save(&mut inquiry, executor).await?;
    }
    Ok(())
}

fn reservation(
    source: &InquiryExpenseSource,
    sales_id: &str,
    document: &str,
    line: &str,
    amount: Amount,
) -> InquiryFeeReservation {
    InquiryFeeReservation {
        reservation_id: format!("{document}:{line}"),
        allocation_id: source.allocation_id.clone(),
        fee_id: source.fee_id.clone(),
        sales_order_id: sales_id.into(),
        feedback_round: source.feedback_round,
        amount,
        state: InquiryReservationState::Reserved,
        source_document_id: document.into(),
        source_line_id: line.into(),
    }
}

fn require_fee<'a>(
    inquiry: &'a SalesInquiry,
    source: &InquiryExpenseSource,
    amount: Amount,
    rate: Option<Rate>,
) -> Result<&'a InquiryFee> {
    let feedback = inquiry
        .feedbacks
        .iter()
        .find(|feedback| feedback.round == source.feedback_round)
        .ok_or_else(|| Error::BusinessLogicError("询价费用反馈不存在".into()))?;
    let fee = feedback
        .data
        .fees
        .iter()
        .find(|fee| fee.fee_id == source.fee_id)
        .ok_or_else(|| Error::BusinessLogicError("询价费用不存在".into()))?;
    if rate != Some(fee.tax_rate) || amount > fee.amount_gross {
        return Err(Error::ValidationError("费用税率或金额不符合已接受结果，请先重新询价".into()));
    }
    Ok(fee)
}

/// 返回询价费用与正式采购、财务费用共用的类别代码。
/// # 参数
/// `fee` 为领域已校验的费用快照。
/// # 返回
/// 返回正式费用合同中的稳定类别代码。
/// # 错误
/// 不执行 I/O，不返回错误。
pub(crate) fn category_code(fee: &InquiryFee) -> &'static str {
    match fee.category {
        InquiryFeeCategory::Logistics => "logistics",
        InquiryFeeCategory::Printing => "printing",
        InquiryFeeCategory::Warehousing => "storage",
        InquiryFeeCategory::Delivery => "delivery",
        InquiryFeeCategory::PlatformTech => "platform_tech",
        InquiryFeeCategory::OfflineService => "offline_service",
        InquiryFeeCategory::Other => "other",
    }
}

async fn validate_payee(
    db: &Database,
    inquiry: &SalesInquiry,
    source: &InquiryExpenseSource,
    order: &PurchaseOrder,
    executor: &mut dyn Executor,
) -> Result<()> {
    let feedback = inquiry
        .feedbacks
        .iter()
        .find(|feedback| feedback.round == source.feedback_round)
        .ok_or_else(|| Error::BusinessLogicError("询价费用反馈不存在".into()))?;
    let bases = InquirySupplyService::new(db.clone())
        .fee_bases(&inquiry.base.id, feedback.requirement_version, feedback.round, executor)
        .await?;
    if !payee_basis_matches(&bases, &source.fee_id, order.supplier_id.as_ref(), &order.payment_term_code)? {
        return Err(Error::BusinessLogicError("本轮询价费用缺少已确认的收款依据".into()));
    }
    Ok(())
}

fn payee_basis_matches(
    bases: &[InquiryFeeBasis],
    fee_id: &str,
    supplier_id: &str,
    payment_term_code: &str,
) -> Result<bool> {
    let mut matching = bases.iter().filter(|basis| basis.data.fee_id == fee_id);
    let Some(basis) = matching.next() else { return Ok(false) };
    if matching.next().is_some()
        || basis.data.payee_supplier_id.as_ref() != supplier_id
        || basis.data.payment_term_code != payment_term_code
    {
        return Err(Error::BusinessLogicError(
            "收款供应商或付款条件与本轮询价费用依据不一致，请重新确认".into(),
        ));
    }
    Ok(true)
}

async fn confirm_conversion_payee(
    db: &Database,
    inquiry: &SalesInquiry,
    source: &InquiryExpenseSource,
    order: &PurchaseOrder,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let feedback = inquiry.accepted_feedback(source.feedback_round, Instant::now())?;
    let service = InquirySupplyService::new(db.clone());
    let bases =
        service.fee_bases(&inquiry.base.id, feedback.requirement_version, feedback.round, executor).await?;
    if !payee_basis_matches(&bases, &source.fee_id, order.supplier_id.as_ref(), &order.payment_term_code)? {
        let identity = CommandIdentity::new(
            "inquiry-fee-basis:",
            [
                inquiry.base.id.clone(),
                feedback.requirement_version.to_string(),
                feedback.round.to_string(),
                source.fee_id.clone(),
            ],
        )?;
        let basis = InquiryFeeBasis::new(
            identity.current_id().to_string(),
            InquiryFeeBasisData {
                inquiry_id: inquiry.base.id.clone(),
                requirement_version: feedback.requirement_version,
                feedback_round: feedback.round,
                fee_id: source.fee_id.clone(),
                payee_supplier_id: order.supplier_id.clone(),
                payment_term_code: order.payment_term_code.clone(),
                internal_note: "销售生效后的费用采购转换确认".into(),
                attachment_ids: vec![],
                recorded_by: actor_id.to_string(),
            },
        )?;
        service.append_fee_basis(&basis, executor).await?;
    }
    validate_payee(db, inquiry, source, order, executor).await
}

/// 目标采购费用的完整采用事实，保留对来源金额和税率的核验依据。
pub(crate) struct PurchaseExpenseTarget<'a> {
    pub detail: &'a PurchaseExpenseDetail,
    pub amount: Amount,
    pub rate: Option<Rate>,
}

/// 保存或正式变更费用时同步明确采用的反馈轮次和完整目标条件。
/// # 参数
/// `order` 为当前采购与收款条件，`targets` 为完整目标费用，`actor_id` 为确认人，
/// `executor` 为原事务。
/// # 返回
/// 同事务保存跨轮次余额；原轮次减少和不变内容不依赖报价继续有效。
/// # 错误
/// 来源新增、遗漏、替换、税率不符、已办理修改或新采用条件失效时拒绝。
pub(crate) async fn resize_purchase(
    db: &Database,
    order: &PurchaseOrder,
    targets: &[PurchaseExpenseTarget<'_>],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = SalesInquiryService::new(db.clone());
    let mut inquiries = service.fee_sources(&order.base.id, executor).await?;
    validate_target_bindings(&inquiries, &order.base.id, order.sales_order_id.as_ref(), targets)?;
    if !inquiries.is_empty() {
        // 付款按采购单核销，没有逐费用付款归属；不能把整单每一费用都标成已发生。
        // 同事务重验整单财务下限，来源仅随通过正式业务守卫的减额释放。
        if let Some(account) = db
            .payable_accounts()
            .find_by_purchase_order(&PurchaseOrderId::new(&order.base.id), executor)
            .await?
        {
            let total = targets.iter().fold(Amount::zero(), |sum, row| sum.checked_add(row.amount));
            account.ensure_purchase_change_amount(total)?;
        }
    }
    for inquiry in &mut inquiries {
        resize_inquiry(db, inquiry, order, targets, actor_id, executor).await?;
        service.save(inquiry, executor).await?;
    }
    Ok(())
}

fn validate_target_bindings(
    inquiries: &[SalesInquiry],
    document_id: &str,
    sales_id: &str,
    targets: &[PurchaseExpenseTarget<'_>],
) -> Result<()> {
    let mut ids = HashSet::new();
    for target in targets {
        if !ids.insert(&target.detail.expense_line_id) {
            return Err(Error::ValidationError("采购费用行身份重复".into()));
        }
        let Some(source) = &target.detail.inquiry_source else { continue };
        let source = source.normalized()?;
        let matches = inquiries
            .iter()
            .filter(|inquiry| inquiry.base.id == source.inquiry_id)
            .flat_map(|inquiry| &inquiry.fee_ledger)
            .filter(|row| {
                row.source_document_id == document_id
                    && row.source_line_id == target.detail.expense_line_id
                    && row.sales_order_id == sales_id
                    && row.fee_id == source.fee_id
                    && row.allocation_id == source.allocation_id
            })
            .count();
        if matches != 1 {
            return Err(Error::ValidationError(
                "新增或伪造的询价费用来源不能通过采购修改采用，请另建费用采购".into(),
            ));
        }
    }
    Ok(())
}

async fn resize_inquiry(
    db: &Database,
    inquiry: &mut SalesInquiry,
    order: &PurchaseOrder,
    targets: &[PurchaseExpenseTarget<'_>],
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let reservations = inquiry
        .fee_ledger
        .iter()
        .filter(|row| row.source_document_id == order.base.id)
        .cloned()
        .collect::<Vec<_>>();
    for row in reservations {
        let target = targets
            .iter()
            .find(|target| target.detail.expense_line_id == row.source_line_id)
            .ok_or_else(|| Error::ValidationError("询价费用行不能删除，请通过金额调整保留来源".into()))?;
        let source = target
            .detail
            .inquiry_source
            .as_ref()
            .ok_or_else(|| Error::ValidationError("询价费用来源不能移除".into()))?
            .normalized()?;
        validate_target_identity(inquiry, order, &row, &source)?;
        let fee = require_fee(inquiry, &source, target.amount, target.rate)?;
        if !fee.requires_erp_payment || category_code(fee) != target.detail.category.as_str() {
            return Err(Error::ValidationError("目标费用类别或办理方式与采用的询价反馈不一致".into()));
        }
        if source.feedback_round != row.feedback_round || target.amount > row.amount {
            confirm_conversion_payee(db, inquiry, &source, order, actor_id, executor).await?;
        } else {
            validate_payee(db, inquiry, &source, order, executor).await?;
        }
        inquiry.adjust_fee_for_round(
            &row.reservation_id,
            target.amount,
            source.feedback_round,
            Instant::now(),
        )?;
    }
    Ok(())
}

fn validate_target_identity(
    inquiry: &SalesInquiry,
    order: &PurchaseOrder,
    row: &InquiryFeeReservation,
    source: &InquiryExpenseSource,
) -> Result<()> {
    if source.inquiry_id != inquiry.base.id
        || source.fee_id != row.fee_id
        || source.allocation_id != row.allocation_id
        || row.sales_order_id != order.sales_order_id.as_ref()
    {
        return Err(Error::ValidationError("询价费用的稳定来源及销售归属不能更换".into()));
    }
    Ok(())
}

/// 作废费用采购仅释放尚未办理来源；驳回及撤回不得调用。
///
/// # 参数
/// `document_id` 为已通过业务作废守卫的采购单。
/// # 返回
/// 对应未办理来源已释放，历史事实保留。
/// # 错误
/// 存在已办理金额或保存冲突时拒绝。
pub(crate) async fn release_purchase(
    db: &Database,
    document_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = SalesInquiryService::new(db.clone());
    for mut inquiry in service.fee_sources(document_id, executor).await? {
        let ids = inquiry
            .fee_ledger
            .iter()
            .filter(|row| {
                row.source_document_id == document_id && row.state != InquiryReservationState::Released
            })
            .map(|row| row.reservation_id.clone())
            .collect::<Vec<_>>();
        for id in ids {
            inquiry.release_fee(&id)?;
        }
        service.save(&mut inquiry, executor).await?;
    }
    Ok(())
}

/// 确认发生时将已占用费用标记为已办理；不再扣减第二次。
///
/// # 参数
/// `document_id`、`line_id` 为本次实际发生的稳定费用身份。
/// # 返回
/// 来源为空时保持原流程；有关联时禁止后续释放。
/// # 错误
/// 台账损坏、已释放或 CAS 失败时拒绝。
pub(crate) async fn mark_occurred(
    db: &Database,
    document_id: &str,
    line_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let service = SalesInquiryService::new(db.clone());
    for mut inquiry in service.fee_sources(document_id, executor).await? {
        let ids = inquiry
            .fee_ledger
            .iter()
            .filter(|row| row.source_document_id == document_id && row.source_line_id == line_id)
            .map(|row| row.reservation_id.clone())
            .collect::<Vec<_>>();
        if ids.is_empty() {
            continue;
        }
        for id in ids {
            inquiry.register_fee(&id)?;
        }
        service.save(&mut inquiry, executor).await?;
    }
    Ok(())
}

/// 财务直接登记与费用采购消费同一来源，不产生应付。
///
/// # 参数
/// `reference` 为可选稳定费用来源；`entry` 为已校验的实际登记事实。
/// # 返回
/// 来源占用并登记为已办理，和成本事实共用事务。
/// # 错误
/// 费用仍要求 ERP 付款、来源金额或类别非法、重复办理时拒绝。
pub(crate) async fn register_finance(
    db: &Database,
    reference: Option<&InquiryFeeReference>,
    entry: &CostEntry,
    executor: &mut dyn Executor,
) -> Result<()> {
    let Some(reference) = reference else { return Ok(()) };
    let source = InquiryExpenseSource {
        inquiry_id: reference.inquiry_id.clone(),
        feedback_round: reference.feedback_round,
        fee_id: reference.fee_id.clone(),
        allocation_id: reference.allocation_id.clone(),
    }
    .normalized()?;
    let service = SalesInquiryService::new(db.clone());
    let mut inquiry = service.require(&source.inquiry_id, executor).await?;
    let fee = require_fee(&inquiry, &source, entry.gross_amount, Some(entry.input_tax_rate))?;
    if fee.requires_erp_payment || category_code(fee) != entry.cost_type.as_str() {
        return Err(Error::BusinessLogicError("费用办理方式或类别不符合已接受询价，请先重新确认费用".into()));
    }
    let reserved =
        reservation(&source, &entry.source_document_id, &entry.base.id, &entry.base.id, entry.gross_amount);
    let reservation_id = reserved.reservation_id.clone();
    inquiry.reserve_fee(reserved, Instant::now())?;
    inquiry.register_fee(&reservation_id)?;
    service.save(&mut inquiry, executor).await?;
    Ok(())
}

/// 核对财务原结果的询价来源，阻止同键切换来源或去掉来源后重放。
///
/// # 参数
/// `entry_id` 为原成本事实，`reference` 为本次声明的可选来源。
/// # 返回
/// 只有原来源与本次完全相同时允许返回原结果。
/// # 错误
/// 来源被替换、移除或损坏时拒绝。
pub(crate) async fn check_finance_source(
    db: &Database,
    entry_id: &str,
    reference: Option<&InquiryFeeReference>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let sources = SalesInquiryService::new(db.clone()).fee_sources(entry_id, executor).await?;
    let matches = match reference {
        None => sources.is_empty(),
        Some(reference) => {
            sources.len() == 1
                && sources[0].base.id == reference.inquiry_id
                && sources[0].fee_ledger.iter().any(|row| {
                    row.source_document_id == entry_id
                        && row.feedback_round == reference.feedback_round
                        && row.fee_id == reference.fee_id
                        && row.allocation_id == reference.allocation_id
                        && row.state == InquiryReservationState::Handled
                })
        },
    };
    if !matches {
        return Err(Error::ConflictError("本操作号已用于其他询价费用来源".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SupplierAccountId;
    use erp_procurement::entity::purchase_order::PurchaseExpenseCategory;
    use erp_sales::entity::sales_inquiry::{InquiryCreateData, InquiryRequirementData};

    use super::*;

    fn inquiry() -> SalesInquiry {
        SalesInquiry::create(
            "iq".into(),
            "IQ01".into(),
            InquiryCreateData {
                customer_id: "customer".into(),
                customer_name: "客户".into(),
                sales_owner_user_id: "sales".into(),
                business_org_unit_id: "org".into(),
                deadline_at: Instant::from_unix_secs(100),
                copied_from_inquiry_id: None,
                draft: InquiryRequirementData::default(),
            },
        )
        .unwrap()
    }
    fn source() -> InquiryExpenseSource {
        InquiryExpenseSource {
            inquiry_id: "iq".into(),
            fee_id: "fee".into(),
            allocation_id: "allocation".into(),
            feedback_round: 1,
        }
    }
    fn detail() -> PurchaseExpenseDetail {
        PurchaseExpenseDetail {
            inquiry_source: Some(source()),
            ..PurchaseExpenseDetail::new(
                "expense".into(),
                PurchaseExpenseCategory::Logistics,
                "运费".into(),
                None,
            )
            .unwrap()
        }
    }

    #[test]
    fn accepted_fee_conversion_can_fill_missing_basis_but_cannot_replace_confirmed_payee() {
        assert!(!payee_basis_matches(&[], "fee", "supplier", "net30").unwrap());
        let basis = InquiryFeeBasis::new(
            "basis".into(),
            InquiryFeeBasisData {
                inquiry_id: "iq".into(),
                requirement_version: 1,
                feedback_round: 1,
                fee_id: "fee".into(),
                payee_supplier_id: SupplierAccountId::new("supplier"),
                payment_term_code: "net30".into(),
                internal_note: String::new(),
                attachment_ids: vec![],
                recorded_by: "procurement".into(),
            },
        )
        .unwrap();
        assert!(payee_basis_matches(std::slice::from_ref(&basis), "fee", "supplier", "net30").unwrap());
        assert!(payee_basis_matches(std::slice::from_ref(&basis), "fee", "other", "net30").is_err());
        assert!(payee_basis_matches(std::slice::from_ref(&basis), "fee", "supplier", "advance").is_err());
        assert!(payee_basis_matches(&[basis.clone(), basis], "fee", "supplier", "net30").is_err());
    }

    #[test]
    fn purchase_edits_cannot_introduce_forged_fee_sources_or_switch_sales() {
        let detail = detail();
        let targets = [PurchaseExpenseTarget {
            detail: &detail,
            amount: "10.00".parse().unwrap(),
            rate: Some("0.06".parse().unwrap()),
        }];
        assert!(validate_target_bindings(&[], "purchase", "sales", &targets).is_err());
        let mut inquiry = inquiry();
        inquiry.fee_ledger.push(reservation(
            &source(),
            "sales",
            "purchase",
            "expense",
            "10.00".parse().unwrap(),
        ));
        assert!(validate_target_bindings(&[inquiry.clone()], "purchase", "sales", &targets).is_ok());
        assert!(validate_target_bindings(&[inquiry.clone()], "purchase", "other-sales", &targets).is_err());
        assert!(validate_target_bindings(&[inquiry], "another-purchase", "sales", &targets).is_err());
    }

    #[test]
    fn feedback_round_can_change_only_with_identical_stable_fee_binding() {
        let detail = detail();
        let mut inquiry = inquiry();
        inquiry.fee_ledger.push(reservation(
            &source(),
            "sales",
            "purchase",
            "expense",
            "10.00".parse().unwrap(),
        ));
        let mut next = detail.clone();
        next.inquiry_source.as_mut().unwrap().feedback_round = 2;
        let targets = [PurchaseExpenseTarget {
            detail: &next,
            amount: "12.00".parse().unwrap(),
            rate: Some("0.06".parse().unwrap()),
        }];
        assert!(validate_target_bindings(&[inquiry.clone()], "purchase", "sales", &targets).is_ok());
        next.inquiry_source.as_mut().unwrap().fee_id = "other-fee".into();
        let forged = [PurchaseExpenseTarget {
            detail: &next,
            amount: "12.00".parse().unwrap(),
            rate: Some("0.06".parse().unwrap()),
        }];
        assert!(validate_target_bindings(&[inquiry], "purchase", "sales", &forged).is_err());
    }
}
