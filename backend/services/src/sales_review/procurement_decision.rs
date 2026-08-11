use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, NoTransaction, PurchaseOrderExt, ReceivableExt, SalesOrderExt, SalesReviewExt,
    SupplierExt, SupplierOfferingExt, Transactional, WorkItemExt,
};
use entities::common::time::{BusinessDate, Instant};
use entities::money::Quantity;
use entities::sales_order::{LineType, RevisionSource, SalesOrder, SalesOrderSubmissionLine};
use entities::sales_review::{
    ProcurementConfirmation, ProcurementConfirmationLine, ProcurementConfirmationStatus,
};
use entities::supplier::{CapabilityCode, CapabilityStatus};
use entities::supplier_offering::{
    OfferingStatus, SupplierOffering, SupplierOfferingAvailability, SupplierOfferingRevision,
};
use validator::Validate;

use super::formalization::{build_receivable_account, build_receivable_entry, build_revision};
use super::{
    ApproveProcurementConfirmationRequest, GeneratedPurchaseOrderView, ProcurementConfirmationDecisionView,
    RejectProcurementConfirmationRequest, SalesReviewService,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl SalesReviewService {
    /// 采购确认通过（§8.1.1 单事务不变量）。
    ///
    /// 校验采购确认覆盖全部需外采明细（§6.5 跨行断言）后，在**单个事务**内：
    /// 锁定提交并形成不可变销售版本与版本行、更新销售单当前版本与状态
    /// （`EFFECTIVE` + 审核轨 `APPROVED`）、形成应收往来子账与原始应收分录、
    /// 完成采购确认待办、按供应商与履约方式生成采购单草稿、写审计。重复通过幂等返回既有结果。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `req` - 通过请求（幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回决策结果视图（含生效版本与应收子账）。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次/提交/销售单不存在
    /// * `ValidationError` - 覆盖不足或行字段组缺失
    /// * `ConflictError` - 状态机/乐观锁/唯一索引冲突
    pub async fn approve_procurement_confirmation(
        &self,
        id: &str,
        req: ApproveProcurementConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementConfirmationDecisionView> {
        req.validate()?;
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        if confirmation.stable.status == ProcurementConfirmationStatus::Approved {
            return self
                .procurement_decision_idempotent(&confirmation, ProcurementConfirmationStatus::Approved)
                .await;
        }
        if confirmation.stable.status != ProcurementConfirmationStatus::Pending {
            return Err(Error::ConflictError("采购确认已处理，不允许重复决策".to_string()));
        }

        let submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;
        let submission_lines = self
            .db
            .sales_order_submission_lines()
            .list_lines_by_submissions(
                std::slice::from_ref(&confirmation.submission_id),
                &mut NoTransaction,
            )
            .await?;
        let confirmation_lines = self
            .db
            .procurement_confirmation_lines()
            .list_lines_by_confirmation(&confirmation.base.id.clone().into(), &mut NoTransaction)
            .await?;
        self.ensure_confirmation_sources(&confirmation_lines, &submission_lines)
            .await?;
        ensure_confirmation_coverage(&submission_lines, &confirmation_lines)?;
        let order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        ensure_order_awaiting_confirmation(&order)?;
        let purchase_drafts = crate::purchase_order::draft::build_automatic_purchase_drafts(
            &self.db,
            &confirmation.sales_order_id,
            &confirmation_lines,
            actor.id(),
        )
        .await?;
        let purchase_order_views = purchase_drafts
            .iter()
            .map(|draft| GeneratedPurchaseOrderView {
                purchase_order_id: draft.order.base.id.clone(),
                purchase_no: draft.order.purchase_no.clone(),
            })
            .collect::<Vec<_>>();
        let purchase_audits = purchase_drafts
            .iter()
            .map(|draft| {
                actor.clone().resource_log(
                    "purchase_order.auto_create",
                    "purchase_order",
                    draft.order.base.id.clone(),
                )
            })
            .collect::<Result<Vec<_>>>()?;

        let now = Instant::now();
        let revision = build_revision(
            &order,
            &submission,
            &submission_lines,
            RevisionSource::ErpApproval,
            now,
            actor,
        )?;
        let mut order_for_tx = order.clone();
        order_for_tx.approve(now, actor.id())?;
        order_for_tx.attach_revision(&revision.revision.base.id, actor.id());

        let account = build_receivable_account(&order_for_tx, &revision);
        let entry = build_receivable_entry(&account, &revision, now)?;
        let mut confirmation_for_tx = confirmation.clone();
        confirmation_for_tx.approve(actor.id(), now)?;
        let mut work_item = self
            .complete_work_item("procurement_confirmation", id, actor, now)
            .await?;

        let audit = actor.clone().resource_log(
            "procurement_confirmation.approve",
            "sales_order",
            confirmation.sales_order_id.to_string(),
        )?;
        let revision_id = revision.revision.base.id.clone();
        let account_id = account.base.id.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sales_order()
                        .formalize_submission(
                            &mut order_for_tx,
                            &revision.revision,
                            &revision.lines,
                            &revision.goods_lines,
                            &revision.voucher_lines,
                            session,
                        )
                        .await?;
                    db.receivable()
                        .create_receivable_with_entry(&account, &entry, session)
                        .await?;
                    db.procurement_confirmations()
                        .update(&mut confirmation_for_tx, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    for draft in &purchase_drafts {
                        db.purchase_orders().create(&draft.order, session).await?;
                        db.purchase_order_submissions()
                            .create(&draft.submission, session)
                            .await?;
                        for line in &draft.lines {
                            db.purchase_order_submission_lines().create(line, session).await?;
                        }
                    }
                    for purchase_audit in &purchase_audits {
                        db.audit_logs().create(purchase_audit, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id,
            sales_order_id: confirmation.sales_order_id.to_string(),
            status: ProcurementConfirmationStatus::Approved,
            revision_id: Some(revision_id),
            receivable_account_id: Some(account_id),
            purchase_orders: purchase_order_views,
            handled_at: now.unix_secs() as u64,
            reference: format!("PC-OK-{}", order.order_no),
        })
    }

    /// 采购确认驳回（销售单回到销售可处理草稿，不把「驳回」混入已生效状态）。
    ///
    /// # 参数
    /// * `id` - 确认批次 ID
    /// * `req` - 驳回请求（原因代码必填）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回决策结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 确认批次/销售单不存在
    /// * `ConflictError` - 确认批次已处理
    pub async fn reject_procurement_confirmation(
        &self,
        id: &str,
        req: RejectProcurementConfirmationRequest,
        actor: &AuditActor,
    ) -> Result<ProcurementConfirmationDecisionView> {
        req.validate()?;
        let confirmation = self
            .db
            .procurement_confirmations()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购确认不存在".to_string()))?;
        if confirmation.stable.status == ProcurementConfirmationStatus::Rejected {
            return self
                .procurement_decision_idempotent(&confirmation, ProcurementConfirmationStatus::Rejected)
                .await;
        }
        if confirmation.stable.status != ProcurementConfirmationStatus::Pending {
            return Err(Error::ConflictError("采购确认已处理，不允许重复决策".to_string()));
        }
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        let order_no = order.order_no.clone();
        let mut submission = self
            .db
            .sales_order_submissions()
            .find_by_id(&confirmation.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售提交不存在".to_string()))?;

        let now = Instant::now();
        let mut confirmation_for_tx = confirmation.clone();
        confirmation_for_tx.reject(actor.id(), now, req.reject_reason_code)?;
        order.return_to_draft(actor.id())?;
        submission.reject(actor.id())?;
        let mut work_item = self
            .complete_work_item("procurement_confirmation", id, actor, now)
            .await?;
        let audit = actor.clone().resource_log(
            "procurement_confirmation.reject",
            "sales_order",
            confirmation.sales_order_id.to_string(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.procurement_confirmations()
                        .update(&mut confirmation_for_tx, session)
                        .await?;
                    db.sales_orders().update(&mut order, session).await?;
                    db.sales_order_submissions()
                        .update(&mut submission, session)
                        .await?;
                    db.work_items().update(&mut work_item, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id,
            sales_order_id: confirmation.sales_order_id.to_string(),
            status: ProcurementConfirmationStatus::Rejected,
            revision_id: None,
            receivable_account_id: None,
            purchase_orders: Vec::new(),
            handled_at: now.unix_secs() as u64,
            reference: format!("PC-REJ-{order_no}"),
        })
    }

    /// 采购确认决策幂等回查（已处理批次重复决策返回既有结果）。
    ///
    /// # 参数
    /// * `confirmation` - 已处理的确认批次
    /// * `status` - 既有结论
    ///
    /// # 返回
    /// 返回决策结果视图。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn procurement_decision_idempotent(
        &self,
        confirmation: &ProcurementConfirmation,
        status: ProcurementConfirmationStatus,
    ) -> Result<ProcurementConfirmationDecisionView> {
        let order = self
            .db
            .sales_orders()
            .find_by_id(&confirmation.sales_order_id, &mut NoTransaction)
            .await?;
        let revision_id = order
            .as_ref()
            .and_then(|order| order.stable.current_revision_id.clone());
        let account = self
            .db
            .receivable_accounts()
            .find_one_by_field(
                "sales_order_id",
                confirmation.sales_order_id.to_string(),
                &mut NoTransaction,
            )
            .await?;
        let purchase_orders = self
            .db
            .purchase_orders()
            .find_many(
                mongodb::bson::doc! {
                    "sales_order_id": confirmation.sales_order_id.to_string(),
                },
                &mut NoTransaction,
            )
            .await?
            .into_iter()
            .map(|order| GeneratedPurchaseOrderView {
                purchase_order_id: order.base.id,
                purchase_no: order.purchase_no,
            })
            .collect();
        Ok(ProcurementConfirmationDecisionView {
            confirmation_id: confirmation.base.id.clone(),
            sales_order_id: confirmation.sales_order_id.to_string(),
            status,
            revision_id,
            receivable_account_id: account.map(|account| account.base.id),
            purchase_orders,
            handled_at: confirmation
                .handled_at
                .map(|instant| instant.unix_secs() as u64)
                .unwrap_or(0),
            reference: "PC-DONE".to_string(),
        })
    }

    /// 重新校验确认行引用的供给修订与能力修订仍是当前有效版本。
    ///
    /// # 参数
    /// * `lines` - 待保存或待审批的采购确认分行
    ///
    /// # 返回
    /// 全部分行来源与供应商匹配且仍有效时返回 `Ok(())`。
    ///
    /// # 错误
    /// 来源缺失、供应商不匹配或版本已更新时返回 `ValidationError`。
    pub(super) async fn ensure_confirmation_sources(
        &self,
        lines: &[ProcurementConfirmationLine],
        submission_lines: &[SalesOrderSubmissionLine],
    ) -> Result<()> {
        let today = BusinessDate::today();
        let zero = Quantity::from_str("0").expect("静态零值必须合法");
        let mut quantities_by_revision: HashMap<String, (Option<Quantity>, Quantity)> = HashMap::new();
        let mut resolved = Vec::with_capacity(lines.len());
        for line in lines {
            let (offering, revision, availability) = self.current_confirmation_offering(line).await?;
            ensure_confirmation_line_sku(line, &offering, submission_lines)?;
            let total = quantities_by_revision
                .entry(revision.base.id.clone())
                .or_insert((availability.available_quantity, zero));
            total.1 = Quantity::try_from(total.1.to_decimal() + line.confirmed_quantity.to_decimal())?;
            resolved.push((line, revision, availability));
        }
        for (line, revision, availability) in resolved {
            let procurement_quantity = quantities_by_revision
                .get(&revision.base.id)
                .map(|(_, quantity)| *quantity)
                .ok_or_else(|| Error::BusinessLogicError("采购数量汇总结果缺失".to_string()))?;
            ensure_confirmation_line_terms(
                line,
                &revision,
                &availability,
                procurement_quantity,
                submission_lines,
                today,
            )?;
            self.ensure_confirmation_capability(line, today).await?;
        }
        ensure_confirmation_capacity(&quantities_by_revision)?;
        Ok(())
    }

    /// 加载并校验确认行引用的当前供给。
    async fn current_confirmation_offering(
        &self,
        line: &ProcurementConfirmationLine,
    ) -> Result<(
        SupplierOffering,
        SupplierOfferingRevision,
        SupplierOfferingAvailability,
    )> {
        let revision_id = line
            .supplier_offering_revision_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行缺少供给版本", line.line_no)))?;
        let revision = self
            .db
            .supplier_offering_revisions()
            .find_by_id(revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给版本不存在", line.line_no)))?;
        let offering = self
            .db
            .supplier_offerings()
            .find_by_id(&revision.supplier_offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行供给不存在", line.line_no)))?;
        let is_current = offering.stable.current_revision_id.as_deref() == Some(revision_id.as_ref());
        if offering.supplier_id != line.supplier_id
            || offering.stable.status != OfferingStatus::Active
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行供给已变化，请重新选择",
                line.line_no
            )));
        }
        let availability = self
            .db
            .supplier_offering_availabilities()
            .find_by_offering_id(&revision.supplier_offering_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行可供状态不存在", line.line_no)))?;
        Ok((offering, revision, availability))
    }

    /// 校验确认行引用的供应商能力仍为当前启用版本。
    async fn ensure_confirmation_capability(
        &self,
        line: &ProcurementConfirmationLine,
        today: BusinessDate,
    ) -> Result<()> {
        let revision = self
            .db
            .supplier_capability_revisions()
            .find_by_id(&line.supplier_capability_revision_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行能力版本不存在", line.line_no)))?;
        let capability = self
            .db
            .supplier_capabilities()
            .find_by_supplier_and_code(&line.supplier_id, revision.capability_code, &mut NoTransaction)
            .await?
            .ok_or_else(|| {
                Error::ValidationError(format!("采购确认第 {} 行供应商能力不存在", line.line_no))
            })?;
        let is_current = capability.stable.current_revision_id.as_deref()
            == Some(line.supplier_capability_revision_id.as_ref());
        let in_window = revision.valid_from <= today && revision.valid_to.is_none_or(|date| today <= date);
        let capability_matches_mode =
            revision.capability_code == capability_code_for_fulfillment(line.fulfillment_mode);
        if revision.supplier_id != line.supplier_id
            || revision.status != CapabilityStatus::Active
            || capability.stable.status != CapabilityStatus::Active
            || !in_window
            || !capability_matches_mode
            || !is_current
        {
            return Err(Error::ValidationError(format!(
                "采购确认第 {} 行能力版本已变化，请重新选择",
                line.line_no
            )));
        }
        crate::supplier::eligibility::ensure_capability_qualified(
            &self.db,
            &line.supplier_id,
            &line.supplier_capability_revision_id,
            today,
        )
        .await
        .map_err(|error| {
            Error::BusinessLogicError(format!("采购确认第 {} 行资质校验失败：{error}", line.line_no))
        })?;
        Ok(())
    }
}

/// 校验确认分行选择的供给属于对应销售提交商品。
fn ensure_confirmation_line_sku(
    line: &ProcurementConfirmationLine,
    offering: &SupplierOffering,
    submission_lines: &[SalesOrderSubmissionLine],
) -> Result<()> {
    let submission_line = submission_lines
        .iter()
        .find(|row| row.base.id == line.sales_order_submission_line_id.as_ref())
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售明细不存在", line.line_no)))?;
    if submission_line.sku_id.as_ref() == Some(&offering.sku_id) {
        return Ok(());
    }
    Err(Error::ValidationError(format!(
        "采购确认第 {} 行供给不属于该销售商品，请重新选择",
        line.line_no
    )))
}

/// 校验确认行价格、税率、可供状态、有效期与销售承诺交期。
fn ensure_confirmation_line_terms(
    line: &ProcurementConfirmationLine,
    revision: &SupplierOfferingRevision,
    availability: &SupplierOfferingAvailability,
    procurement_quantity: Quantity,
    submission_lines: &[SalesOrderSubmissionLine],
    today: BusinessDate,
) -> Result<()> {
    if !availability.is_available()
        || revision.valid_from > today
        || revision.valid_to.is_some_and(|valid_to| valid_to < today)
    {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行供给当前不可用，请重新计算方案",
            line.line_no
        )));
    }
    let expected_price = if procurement_quantity >= revision.bulk_minimum_order_quantity {
        revision.bulk_supply_price_gross
    } else {
        revision.dropship_supply_price_gross
    };
    if line.latest_cost_gross != expected_price || line.input_tax_rate != revision.input_tax_rate {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行价格或税率已变化，请重新计算方案",
            line.line_no
        )));
    }
    let submission_line = submission_lines
        .iter()
        .find(|submission| submission.base.id == line.sales_order_submission_line_id.as_ref())
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售明细不存在", line.line_no)))?;
    let due_at = submission_line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("采购确认第 {} 行销售承诺交期缺失", line.line_no)))?;
    let due_date = BusinessDate::from_str(&due_at.as_utc().date_naive().to_string())?;
    if line.expected_delivery_date > due_date {
        return Err(Error::ValidationError(format!(
            "采购确认第 {} 行预计交期晚于销售承诺日期 {}",
            line.line_no, due_date
        )));
    }
    Ok(())
}

/// 校验同一供给修订的多条拆分行合计不超过当前可供数量。
fn ensure_confirmation_capacity(
    quantities_by_revision: &HashMap<String, (Option<Quantity>, Quantity)>,
) -> Result<()> {
    for (revision_id, (available, confirmed)) in quantities_by_revision {
        if available.is_some_and(|quantity| *confirmed > quantity) {
            return Err(Error::ValidationError(format!(
                "供给版本 {} 的确认数量合计超过当前可供数量，请重新计算方案",
                revision_id
            )));
        }
    }
    Ok(())
}

/// 将确认履约方式映射到供应商能力类别。
fn capability_code_for_fulfillment(mode: entities::sales_review::types::FulfillmentMode) -> CapabilityCode {
    match mode {
        entities::sales_review::types::FulfillmentMode::CompanyWarehouse
        | entities::sales_review::types::FulfillmentMode::SupplierDirect => CapabilityCode::Physical,
        entities::sales_review::types::FulfillmentMode::ElectronicDelivery => CapabilityCode::Virtual,
        entities::sales_review::types::FulfillmentMode::OfflineService => CapabilityCode::OfflineService,
    }
}

/// 校验采购确认覆盖全部需外采明细（§6.5 跨行断言）。
///
/// # 参数
/// * `submission_lines` - 销售提交明细
/// * `confirmation_lines` - 采购确认分行
///
/// # 返回
/// 全部行确认数量覆盖承诺数量时返回 `Ok(())`。
///
/// # 错误
/// 任一提交明细确认数量不足时返回 `ValidationError`。
fn ensure_confirmation_coverage(
    submission_lines: &[SalesOrderSubmissionLine],
    confirmation_lines: &[ProcurementConfirmationLine],
) -> Result<()> {
    for sub_line in submission_lines {
        if sub_line.line_type == LineType::Voucher {
            continue;
        }
        let required = sub_line
            .quantity
            .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少承诺数量", sub_line.line_no)))?;
        let mut confirmed = Quantity::from_str("0").expect("静态零值必须合法");
        for line in confirmation_lines {
            if line.sales_order_submission_line_id.to_string() == sub_line.base.id {
                if line.supplier_offering_revision_id.is_none() {
                    return Err(Error::ValidationError(format!(
                        "第 {} 行缺少有效供给版本，请重新选择供应商供给",
                        sub_line.line_no
                    )));
                }
                confirmed = Quantity::try_from(confirmed.to_decimal() + line.confirmed_quantity.to_decimal())
                    .map_err(|error| Error::ValidationError(error.to_string()))?;
            }
        }
        if confirmed.to_decimal() < required.to_decimal() {
            return Err(Error::ValidationError(format!(
                "第 {} 行确认数量不足：需 {required}，已确认 {confirmed}",
                sub_line.line_no
            )));
        }
    }
    Ok(())
}

/// 校验销售单处于待采购确认状态。
///
/// # 参数
/// * `order` - 销售单
///
/// # 返回
/// 状态合法时返回 `Ok(())`。
///
/// # 错误
/// 状态非法时返回 `ValidationError`。
fn ensure_order_awaiting_confirmation(order: &SalesOrder) -> Result<()> {
    if order.stable.status != entities::sales_order::CommercialStatus::PendingReview
        || order.review_status != entities::sales_order::ReviewStatus::PendingProcurementConfirmation
    {
        return Err(Error::ValidationError(
            "销售单不在待采购确认状态，无法通过采购确认".to_string(),
        ));
    }
    Ok(())
}
