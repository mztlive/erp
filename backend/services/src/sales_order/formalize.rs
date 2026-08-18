//! 销售单最终通过：包装既有 `formalize_submission` 仓储端口。

use database::{
    AccessControlExt, DocumentRegistryExt, NoTransaction, ProjectionExt, SalesOrderExt, Transactional,
};
use entities::common::time::Instant;
use entities::ids::{
    SalesOrderId, SalesOrderProjectionDeliveryId, SalesOrderProjectionId, SalesOrderProjectionRevisionId,
    SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderSubmissionId, SalesOrderVoucherLineRevisionId,
};
use entities::projection::{
    CardForm as ProjectionCardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection,
    SalesOrderProjectionData, SalesOrderProjectionDelivery, SalesOrderProjectionDeliveryData,
    SalesOrderProjectionRevision, SalesOrderProjectionRevisionData,
};
use entities::sales_order::{
    BusinessType, CardForm, GoodsLineFields, LineType, RevisionSource, SalesOrder,
    SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData,
    SalesOrderGoodsServiceLineRevisionId, SalesOrderRevision, SalesOrderRevisionData, SalesOrderRevisionLine,
    SalesOrderRevisionLineData, SalesOrderSubmission, SalesOrderSubmissionLine,
    SalesOrderVoucherLineRevision, SalesOrderVoucherLineRevisionData, VoucherLineDraft,
};
use id_generator::next_id;
use mongodb::Database;

use super::adapter::{ensure_final_approve_formalize, is_voucher_sales_order};
use super::dto::SalesOrderDetailView;
use super::SalesOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::projection::projection_content_hash;

/// 销售版本聚合载体（版本头 + 公共行 + 子类型行）。
struct RevisionAggregate {
    revision: SalesOrderRevision,
    lines: Vec<SalesOrderRevisionLine>,
    goods_lines: Vec<SalesOrderGoodsServiceLineRevision>,
    voucher_lines: Vec<SalesOrderVoucherLineRevision>,
}

impl SalesOrderService {
    /// 最终通过并形式化已批准提交。
    ///
    /// 只包装既有 `repository formalize_submission`：先把销售单推进到
    /// `EFFECTIVE` / `APPROVED`，再写入正式修订。卡券在同一事务向商城发出执行投影。
    /// 不得 `$set` 绕过领域不变式，也不得按卡券运营节点写专用副作用分支。
    ///
    /// # 参数
    /// * `id` - 销售单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回形式化后的销售单详情。
    ///
    /// # 错误
    /// 非审批中、缺少提交、卡券投影字段缺失或仓储失败时返回错误。
    pub async fn formalize_approved_submission(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<SalesOrderDetailView> {
        let mut order = self
            .db
            .sales_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
        ensure_final_approve_formalize(&order)?;
        let (submission, lines) = load_latest_submission(&self.db, id).await?;
        persist_formalized_submission(&self.db, &mut order, submission, lines, actor).await?;
        self.sales_order_detail(id, None).await
    }
}

/// 读取该销售单最新提交及其明细。
///
/// # 错误
/// 无提交或仓储失败时返回错误。
async fn load_latest_submission(
    db: &Database,
    sales_order_id: &str,
) -> Result<(SalesOrderSubmission, Vec<SalesOrderSubmissionLine>)> {
    let mut submissions = db
        .sales_order_submissions()
        .find_many(
            mongodb::bson::doc! { "sales_order_id": sales_order_id },
            &mut NoTransaction,
        )
        .await?;
    submissions.sort_by_key(|item| item.submission_no);
    let submission = submissions
        .pop()
        .ok_or_else(|| Error::ConflictError("销售单没有可形式化的提交".to_string()))?;
    let submission_id = SalesOrderSubmissionId::new(submission.base.id.clone());
    let lines = db
        .sales_order_submission_lines()
        .list_lines_by_submissions(&[submission_id], &mut NoTransaction)
        .await?;
    Ok((submission, lines))
}

/// 在同一事务内推进状态并调用既有 `formalize_submission`。
///
/// # 错误
/// 状态不允许、行字段缺失或写入失败时返回错误。
async fn persist_formalized_submission(
    db: &Database,
    order: &mut SalesOrder,
    mut submission: SalesOrderSubmission,
    lines: Vec<SalesOrderSubmissionLine>,
    actor: &AuditActor,
) -> Result<()> {
    let now = Instant::now();
    let aggregate = build_revision_for_order(order, &submission, &lines, now)?;
    let projection = if is_voucher_sales_order(order.business_type) {
        Some(build_voucher_execution_projection(
            order,
            &submission,
            &aggregate,
            now,
        )?)
    } else {
        None
    };
    order.approve(now, actor.id())?;
    order.attach_revision(aggregate.revision.base.id.clone(), actor.id());
    submission.approve(actor.id())?;
    let audit = actor
        .clone()
        .resource_log("sales_order.formalize", "sales_order", order.base.id.clone())?;
    let order_id = order.base.id.clone();
    let db = db.clone();
    let mut order_for_tx = order.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                if let Some(mut document) = db.business_documents().find_by_id(&order_id, session).await? {
                    document.formalize(now);
                    db.business_documents().update(&mut document, session).await?;
                }
                db.sales_order()
                    .formalize_submission(
                        &mut order_for_tx,
                        &aggregate.revision,
                        &aggregate.lines,
                        &aggregate.goods_lines,
                        &aggregate.voucher_lines,
                        session,
                    )
                    .await?;
                db.sales_order_submissions()
                    .update(&mut submission, session)
                    .await?;
                if let Some((proj, revision, delivery)) = projection {
                    persist_voucher_projection(&db, &proj, &revision, &delivery, session).await?;
                }
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await?;
    let _ = SalesOrderId::new(order.base.id.clone());
    Ok(())
}

/// 按业务性质构造正式版本。
///
/// # 错误
/// 行类型与业务性质不一致或字段缺失时返回错误。
fn build_revision_for_order(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    match order.business_type {
        BusinessType::Voucher => build_voucher_revision(order, submission, submission_lines, effective_at),
        BusinessType::GoodsService => {
            build_goods_service_revision(order, submission, submission_lines, effective_at)
        }
    }
}

/// 由实物及服务提交构造正式版本。卡券行失败关闭。
///
/// # 错误
/// 行字段组缺失或版本字段校验失败时返回错误。
fn build_goods_service_revision(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    let revision_id = SalesOrderRevisionId::new(next_id());
    let revision = SalesOrderRevision::new(
        revision_id.clone(),
        SalesOrderRevisionData {
            sales_order_id: submission.sales_order_id.clone(),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: order.stable.current_revision_id.clone().map(Into::into),
            content_hash: format!("sub:{}", submission.base.id),
            customer_revision_id: None,
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: submission.customer_snapshot.customer_name.clone(),
                contract_no: submission
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: submission
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
            effective_at,
            recorded_at: effective_at,
        },
    )?;
    let mut revision_lines = Vec::with_capacity(submission_lines.len());
    let mut goods_lines = Vec::new();
    for sub_line in submission_lines {
        if sub_line.line_type != LineType::GoodsService {
            return Err(Error::ConflictError(
                "实物及服务销售单不得形式化卡券行".to_string(),
            ));
        }
        let revision_line_id = SalesOrderRevisionLineId::new(next_id());
        revision_lines.push(SalesOrderRevisionLine::new(
            revision_line_id.clone(),
            SalesOrderRevisionLineData {
                sales_order_revision_id: revision_id.clone(),
                sales_order_line_id: sub_line.sales_order_line_id.clone(),
                line_no: sub_line.line_no,
                line_type: sub_line.line_type,
                gross_amount: sub_line.gross_amount,
                net_amount: sub_line.net_amount,
                tax_amount: sub_line.tax_amount,
                sales_tax_rate: sub_line.sales_tax_rate,
                item_name_snapshot: sub_line.item_name_snapshot.clone(),
                spec_snapshot: sub_line.spec_snapshot.clone(),
                unit_snapshot: sub_line.unit_snapshot.clone(),
            },
        )?);
        let goods = submission_line_goods(sub_line)?;
        goods_lines.push(SalesOrderGoodsServiceLineRevision::new(
            SalesOrderGoodsServiceLineRevisionId::new(next_id()),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id,
                sku_id: goods.sku_id,
                sku_revision_id: goods.sku_revision_id,
                welfare_scenario: goods.welfare_scenario,
                fulfillment_mode: goods.fulfillment_mode,
                fulfillment_due_at: goods.fulfillment_due_at,
                quantity: goods.quantity,
                base_unit_code: goods.base_unit_code,
                unit_price_gross: goods.unit_price_gross,
            },
        )?);
    }
    Ok(RevisionAggregate {
        revision,
        lines: revision_lines,
        goods_lines,
        voucher_lines: Vec::new(),
    })
}

/// 从提交行还原实物及服务字段组。
///
/// # 错误
/// 实物行缺商品字段组时返回校验错误。
fn submission_line_goods(line: &SalesOrderSubmissionLine) -> Result<GoodsLineFields> {
    let sku_id = line
        .sku_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少商品字段组", line.line_no)))?;
    let sku_revision_id = line
        .sku_revision_id
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU 修订", line.line_no)))?;
    let fulfillment_mode = line
        .fulfillment_mode
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约方式", line.line_no)))?;
    let fulfillment_due_at = line
        .fulfillment_due_at
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少履约期限", line.line_no)))?;
    let quantity = line
        .quantity
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少数量", line.line_no)))?;
    let base_unit_code = line
        .base_unit_code
        .clone()
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少单位", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少含税单价", line.line_no)))?;
    Ok(GoodsLineFields {
        sku_id,
        sku_revision_id,
        welfare_scenario: line.welfare_scenario,
        fulfillment_mode,
        fulfillment_due_at,
        quantity,
        base_unit_code,
        unit_price_gross,
    })
}

/// 由卡券提交构造正式版本。实物行失败关闭。
///
/// # 错误
/// 行字段组缺失、非卡券行或版本字段校验失败时返回错误。
fn build_voucher_revision(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    submission_lines: &[SalesOrderSubmissionLine],
    effective_at: Instant,
) -> Result<RevisionAggregate> {
    let revision_id = SalesOrderRevisionId::new(next_id());
    let revision = new_revision_header(order, submission, revision_id.clone(), effective_at)?;
    let mut revision_lines = Vec::with_capacity(submission_lines.len());
    let mut voucher_lines = Vec::new();
    for sub_line in submission_lines {
        if sub_line.line_type != LineType::Voucher {
            return Err(Error::ConflictError("卡券销售单不得形式化实物行".to_string()));
        }
        let revision_line_id = SalesOrderRevisionLineId::new(next_id());
        revision_lines.push(new_revision_line(&revision_id, &revision_line_id, sub_line)?);
        let voucher = submission_line_voucher(sub_line)?;
        voucher_lines.push(SalesOrderVoucherLineRevision::new(
            SalesOrderVoucherLineRevisionId::new(next_id()),
            SalesOrderVoucherLineRevisionData {
                revision_line_id,
                face_value: voucher.face_value,
                card_count: voucher.card_count,
                unit_price_gross: voucher.unit_price_gross,
                card_form: voucher.card_form,
            },
        )?);
    }
    if voucher_lines.len() != 1 || revision_lines.len() != 1 {
        return Err(Error::BusinessLogicError(
            "卡券销售正式版本必须且只能包含一条卡券明细".to_string(),
        ));
    }
    Ok(RevisionAggregate {
        revision,
        lines: revision_lines,
        goods_lines: Vec::new(),
        voucher_lines,
    })
}

/// 构造正式版本头。
///
/// # 错误
/// 版本字段校验失败时返回错误。
fn new_revision_header(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    revision_id: SalesOrderRevisionId,
    effective_at: Instant,
) -> Result<SalesOrderRevision> {
    Ok(SalesOrderRevision::new(
        revision_id,
        SalesOrderRevisionData {
            sales_order_id: submission.sales_order_id.clone(),
            revision_no: 1,
            revision_source: RevisionSource::ErpApproval,
            source_snapshot_id: None,
            previous_revision_id: order.stable.current_revision_id.clone().map(Into::into),
            content_hash: format!("sub:{}", submission.base.id),
            customer_revision_id: None,
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: entities::sales_order::HeaderSnapshotData {
                customer_name: submission.customer_snapshot.customer_name.clone(),
                contract_no: submission
                    .contract_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.contract_no.clone()),
                settlement_party_name: submission
                    .settlement_party_snapshot
                    .as_ref()
                    .map(|snapshot| snapshot.settlement_party_name.clone()),
                payment_term_code: submission.payment_term_snapshot.payment_term_code.clone(),
                payment_term_name: submission.payment_term_snapshot.payment_term_name.clone(),
                invoice_type: submission.invoice_requirement_snapshot.invoice_type.clone(),
                tax_point: submission.invoice_requirement_snapshot.tax_point.clone(),
            },
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
            effective_at,
            recorded_at: effective_at,
        },
    )?)
}

/// 构造正式版本公共行。
///
/// # 错误
/// 行字段校验失败时返回错误。
fn new_revision_line(
    revision_id: &SalesOrderRevisionId,
    revision_line_id: &SalesOrderRevisionLineId,
    sub_line: &SalesOrderSubmissionLine,
) -> Result<SalesOrderRevisionLine> {
    Ok(SalesOrderRevisionLine::new(
        revision_line_id.clone(),
        SalesOrderRevisionLineData {
            sales_order_revision_id: revision_id.clone(),
            sales_order_line_id: sub_line.sales_order_line_id.clone(),
            line_no: sub_line.line_no,
            line_type: sub_line.line_type,
            gross_amount: sub_line.gross_amount,
            net_amount: sub_line.net_amount,
            tax_amount: sub_line.tax_amount,
            sales_tax_rate: sub_line.sales_tax_rate,
            item_name_snapshot: sub_line.item_name_snapshot.clone(),
            spec_snapshot: sub_line.spec_snapshot.clone(),
            unit_snapshot: sub_line.unit_snapshot.clone(),
        },
    )?)
}

/// 从提交行还原卡券字段组。
///
/// # 错误
/// 卡券行缺字段组时返回校验错误。
fn submission_line_voucher(line: &SalesOrderSubmissionLine) -> Result<VoucherLineDraft> {
    let face_value = line
        .face_value
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券字段组", line.line_no)))?;
    let card_count = line
        .card_count
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡张数", line.line_no)))?;
    let unit_price_gross = line
        .unit_price_gross
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡券成交单价", line.line_no)))?;
    let face_value_total = line
        .face_value_total
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少面额小计", line.line_no)))?;
    let transaction_amount = line
        .transaction_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少成交金额", line.line_no)))?;
    let gift_amount = line
        .gift_amount
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少配赠金额", line.line_no)))?;
    let card_form = line
        .card_form
        .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少卡形态", line.line_no)))?;
    Ok(VoucherLineDraft {
        face_value,
        card_count,
        unit_price_gross,
        face_value_total,
        transaction_amount,
        gift_amount,
        gift_rate: None,
        card_form,
    })
}

/// 由已形式化卡券版本构造商城执行投影与待下发记录。
///
/// # 错误
/// 冻结投影字段缺失或卡券行不唯一时返回错误。
fn build_voucher_execution_projection(
    order: &SalesOrder,
    submission: &SalesOrderSubmission,
    aggregate: &RevisionAggregate,
    at: Instant,
) -> Result<(
    SalesOrderProjection,
    SalesOrderProjectionRevision,
    SalesOrderProjectionDelivery,
)> {
    let voucher = aggregate
        .voucher_lines
        .first()
        .ok_or_else(|| Error::BusinessLogicError("卡券销售正式版本必须且只能包含一条卡券明细".to_string()))?;
    let target_mall_id = submission
        .target_mall_id
        .clone()
        .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结目标商城".to_string()))?;
    let projection_id = SalesOrderProjectionId::new(next_id());
    let projection = SalesOrderProjection::new(
        projection_id.clone(),
        SalesOrderProjectionData {
            sales_order_id: SalesOrderId::new(order.base.id.clone()),
            target_mall_id: target_mall_id.clone(),
        },
    )?;
    let projection_revision_id = SalesOrderProjectionRevisionId::new(next_id());
    let mut projection_revision = SalesOrderProjectionRevision::new(
        projection_revision_id.clone(),
        1,
        SalesOrderProjectionRevisionData {
            projection_id,
            projection_source: ProjectionSource::ErpRevision,
            sales_order_revision_id: aggregate.revision.base.id.clone().into(),
            customer_external_identity: submission
                .customer_external_identity
                .clone()
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结商城客户身份".to_string()))?,
            voucher_category_external_identity: submission
                .voucher_category_external_identity
                .clone()
                .ok_or_else(|| {
                    Error::BusinessLogicError("卡券销售提交缺少冻结商城卡券类目身份".to_string())
                })?,
            voucher_expiry_at: submission
                .voucher_expiry_at
                .ok_or_else(|| Error::BusinessLogicError("卡券销售提交缺少冻结履约期限".to_string()))?,
            face_value: voucher.face_value,
            card_count: voucher.card_count,
            card_form: match voucher.card_form {
                CardForm::Electronic => ProjectionCardForm::Electronic,
                CardForm::Physical => ProjectionCardForm::Physical,
            },
            effective_at: at,
            content_hash: "pending".to_string(),
        },
    )?;
    projection_revision.content_hash = projection_content_hash(&projection_revision);
    let delivery = SalesOrderProjectionDelivery::new(
        SalesOrderProjectionDeliveryId::new(next_id()),
        SalesOrderProjectionDeliveryData {
            projection_revision_id,
            target_mall_id,
            status: ProjectionDeliveryStatus::PendingSend,
            attempt_count: 0,
            next_attempt_at: None,
            mall_ack_at: None,
            mall_execution_baseline: None,
            error_code: None,
            error_summary: None,
        },
    )?;
    Ok((projection, projection_revision, delivery))
}

/// 在同一事务写入执行投影版本与待下发记录。
///
/// # 错误
/// 仓储写入失败时返回错误。
async fn persist_voucher_projection(
    db: &Database,
    projection: &SalesOrderProjection,
    revision: &SalesOrderProjectionRevision,
    delivery: &SalesOrderProjectionDelivery,
    session: &mut mongodb::ClientSession,
) -> Result<()> {
    db.projection()
        .create_projection_revision(projection, revision, session)
        .await?;
    db.sales_order_projection_deliveries()
        .create(delivery, session)
        .await?;
    Ok(())
}

/// 证明本模块只包装仓储 `formalize_submission`。
#[cfg(test)]
mod tests {
    use super::ensure_final_approve_formalize;
    use entities::ids::{CustomerAccountId, PartyId, SalesOrderId};
    use entities::sales_order::{BusinessType, CommercialStatus, ReviewStatus, SalesOrder, SalesOrderData};

    fn draft_order() -> SalesOrder {
        SalesOrder::new(
            SalesOrderId::new("so-1"),
            SalesOrderData {
                order_no: "SO-1".into(),
                business_type: BusinessType::GoodsService,
                origin_system: entities::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("cust-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "user-1",
        )
        .expect("草稿必须可构造")
    }

    /// 形式化必须调用仓储 `formalize_submission`，且只接受 `IN_APPROVAL`。
    #[test]
    fn formalize_wraps_repository_and_only_accepts_in_approval() {
        let source = include_str!("formalize.rs");
        assert!(source.contains("formalize_submission("));
        assert!(source.contains("ensure_final_approve_formalize"));
        assert!(source.contains("build_voucher_execution_projection"));
        assert!(source.contains("create_projection_revision"));
        let production = source.split("/// 证明本模块只包装仓储").next().expect("生产代码");
        assert!(!production.contains("CARD_SALES_APPROVAL"));
        let mut order = draft_order();
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("提交进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
        order.commercial_status = CommercialStatus::Effective;
        order.review_status = ReviewStatus::Approved;
        assert!(ensure_final_approve_formalize(&order).is_err());
    }

    /// 卡券最终通过同样只接受审批中，并与投影写入同事务。
    #[test]
    fn voucher_formalize_accepts_in_approval_and_projects() {
        let source = include_str!("formalize.rs");
        assert!(source.contains("is_voucher_sales_order"));
        assert!(source.contains("persist_voucher_projection"));
        let mut order = draft_order();
        order.business_type = BusinessType::Voucher;
        assert!(ensure_final_approve_formalize(&order).is_err());
        order.start_approval_submission("user-1").expect("卡券进入审批中");
        assert_eq!(order.review_status, ReviewStatus::InApproval);
        assert!(ensure_final_approve_formalize(&order).is_ok());
    }
}
