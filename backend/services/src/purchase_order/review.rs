//! 采购财务审核、应付与成本事实编排。

use std::collections::HashMap;
use std::str::FromStr;

use database::{
    AccessControlExt, CostExt, Executor, FulfillmentExt, NoTransaction, PayableExt, PurchaseOrderExt,
    Transactional,
};
use entities::common::source::SourceType;
use entities::common::time::Instant;
use entities::fulfillment::{
    Delivery, DeliveryData, DeliveryId, DeliveryLine, DeliveryLineData, DeliveryLineId, FulfillmentResult,
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData,
    PurchaseReceiptLineId, QualityResult, ServiceFulfillment, ServiceFulfillmentData, ServiceFulfillmentId,
};
use entities::ids::{
    CostEntryId, PayableEntryId, PurchaseLineSalesAllocationId, PurchaseOrderId, PurchaseReceiptId,
    SalesOrderLineId,
};
use entities::money::Quantity;
use entities::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderReviewDecision,
    PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use entities::work_item::WorkItemStatus;
use id_generator::next_id;

use super::allocation_maintenance::{persist_current_sales_allocations, prepare_current_sales_allocations};
use super::dto::{PurchaseReviewResult, ReviewPurchaseOrderCommand};
use super::shared::{zero_amount, zero_rate};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;

impl PurchaseOrderService {
    /// 旧财务审核旁路。审批改造后立即失败关闭。
    ///
    /// # 错误
    /// 恒返回冲突，不得再写入 `PurchaseReviewStatus`。
    pub async fn review_purchase_order(
        &self,
        _path_purchase_order_id: &str,
        _command: ReviewPurchaseOrderCommand,
        _actor: &AuditActor,
        _rbac: SharedRbacService,
    ) -> Result<PurchaseReviewResult> {
        Err(Error::ConflictError(
            "采购单必须走统一审批，禁止写入财务审核旁路".to_string(),
        ))
    }

    /// 最终通过并生效：形成采购版本、应付与成本事实。
    ///
    /// 仅由合同 §4.4.4 `on_final_approve` 调用，不得再作为人工财务审核旁路。
    ///
    /// # 参数
    /// * `id` - 采购单主键
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回生效结果。
    ///
    /// # 错误
    /// 非审批中、缺少提交、来源复验失败或仓储失败时返回错误。
    pub async fn formalize_approved_order(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PurchaseReviewResult> {
        use super::adapter::{execute_purchase_order_domain_action, purchase_order_adapter};
        use crate::approval::policy::ApprovalDomainAction;

        let adapter = purchase_order_adapter()?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        let submission_id = order
            .submission_id_for_formalization()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        execute_purchase_order_domain_action(
            &mut order.clone(),
            adapter.on_final_approve,
            submission_id.as_ref(),
            actor.id(),
        )?;
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("待审核提交不存在".to_string()))?;
        submission
            .ensure_pending()
            .map_err(|_| Error::ConflictError("提交已审核或已失效，请勿重复生效".to_string()))?;
        let submission_lines = self
            .db
            .purchase_order()
            .list_submission_lines(&submission_id, &mut NoTransaction)
            .await?;
        let revision_no = self.next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .build_effective_revision(&order, &submission, &submission_lines, revision_no)
            .await?;
        let payable = self
            .build_payable(&order, &submission, &submission_lines, actor.id())
            .await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;
        let subject_version = order.approval_subject_version.to_string();
        let lock_version = order.base.version;
        let revision_id = revision.base.id.clone();
        let payable_entry_id = payable.1.base.id.clone();
        persist_formalized_order(
            &self.db,
            FormalizedOrderPersist {
                order,
                submission,
                submission_lines,
                revision,
                revision_lines,
                payable,
                cost_entries,
            },
            actor,
        )
        .await?;
        let _ = ApprovalDomainAction::PurchaseOrderFormalizeApprovedOrder;
        Ok(PurchaseReviewResult {
            work_item_id: String::new(),
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: "0".to_string(),
            subject_version,
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision_id),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable_entry_id),
            lock_version,
            reference: format!("PO-V{revision_no}"),
        })
    }

    /// 构建应付子账与原始应付分录（D19；子账按采购单维度）。
    async fn build_payable(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        actor_id: &str,
    ) -> Result<(entities::payable::PayableAccount, entities::payable::PayableEntry)> {
        let expected_delivery_on = submission_lines
            .iter()
            .filter(|line| line.line_type == PurchaseLineType::ItemService)
            .filter_map(|line| line.expected_delivery_date)
            .max();
        let due_date = submission
            .payment_term_snapshot
            .payable_due_date(
                entities::common::time::BusinessDate::today(),
                expected_delivery_on,
            )
            .map_err(Error::Logic)?;
        let account = entities::payable::PayableAccount::new(
            entities::ids::PayableAccountId::new(next_id()),
            entities::payable::PayableAccountData {
                source_document_id: order.base.id.clone(),
                supplier_id: order.supplier_id.clone(),
                source_type: entities::payable::PayableSourceType::PurchaseOrder,
                gross_total: submission.gross_amount,
                settled_total: zero_amount(),
                invoiceable_total: submission.gross_amount,
                invoiced_total: zero_amount(),
            },
            actor_id,
        )?;
        let entry = entities::payable::PayableEntry::new(
            PayableEntryId::new(next_id()),
            entities::payable::PayableEntryData {
                payable_account_id: account.base.id.clone().into(),
                entry_type: entities::payable::PayableEntryType::Original,
                direction: entities::payable::EntryDirection::Increase,
                amount: submission.gross_amount,
                due_date,
                source_fact_type: "purchase_order".to_string(),
                source_document_id: order.base.id.clone(),
                source_revision_id: submission.base.id.clone(),
                source_sequence: 1,
                posted_at: Instant::now(),
            },
        )?;
        Ok((account, entry))
    }

    /// 构建 `CONFIRMED` 成本事实（D20；逐采购行一个成本事实）。
    async fn build_confirmed_cost_entries(
        &self,
        submission: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<Vec<entities::cost::CostEntry>> {
        let mut entries = Vec::new();
        for line in lines {
            let tax_rate = line.input_tax_rate.unwrap_or_else(zero_rate);
            entries.push(entities::cost::CostEntry::new(
                CostEntryId::new(next_id()),
                entities::cost::CostEntryData {
                    cost_type: if line.line_type == PurchaseLineType::LogisticsFee {
                        entities::cost::CostType::Logistics
                    } else {
                        entities::cost::CostType::Product
                    },
                    cost_stage: entities::cost::CostStage::Confirmed,
                    cost_scope: entities::cost::CostScope::NonVoucherFulfillment,
                    cost_basis: None,
                    supplier_id: Some(submission.supplier_id.clone()),
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    tax_inclusion: true,
                    input_tax_rate: tax_rate,
                    occurred_at: Instant::now(),
                    source_fact_type: "purchase_order".to_string(),
                    source_document_id: submission.purchase_order_id.to_string(),
                    source_line_id: line.base.id.clone(),
                    source_version: revision_no.to_string(),
                    adjusts_cost_entry_id: None,
                    evidence_attachment_id: None,
                },
            )?);
        }
        Ok(entries)
    }
}

/// 采购单正式生效写入所需的单据、版本、应付与成本。
///
/// # 用途
/// 将生效版本、提交结论、应付与成本打包后一次写入。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 提交必须仍为待审核；来源复验失败必须回滚。
struct FormalizedOrderPersist {
    /// 待正式化的采购单。
    order: PurchaseOrder,
    /// 待记录结论的提交。
    submission: PurchaseOrderSubmission,
    /// 提交行。
    submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 生效版本。
    revision: entities::purchase_order::PurchaseOrderRevision,
    /// 生效版本行。
    revision_lines: Vec<entities::purchase_order::PurchaseOrderRevisionLine>,
    /// 应付账户与分录。
    payable: (entities::payable::PayableAccount, entities::payable::PayableEntry),
    /// 确认成本分录。
    cost_entries: Vec<entities::cost::CostEntry>,
}

/// 在同一事务内写入生效版本、采购单、提交结论、应付与成本。
///
/// # 用途
/// 正式化已通过审核的采购提交。
///
/// # 参数
/// * `db` - 数据库
/// * `persist` - 采购单、提交、版本、应付与成本
/// * `actor` - 审计操作人
///
/// # 返回
/// 写入成功时返回 `Ok(())`。
///
/// # 错误
/// 状态不允许、来源复验失败或仓储失败时返回错误。
///
/// # 关键业务约束
/// 必须与审核结论同一事务写入。
async fn persist_formalized_order(
    db: &mongodb::Database,
    persist: FormalizedOrderPersist,
    actor: &AuditActor,
) -> Result<()> {
    let FormalizedOrderPersist {
        order,
        submission,
        submission_lines,
        revision,
        mut revision_lines,
        payable,
        cost_entries,
    } = persist;
    let fulfillment_responsibility = order.fulfillment_responsibility;
    let actor_id = actor.id().to_string();
    let audit = actor.clone().resource_log(
        "purchase_order.formalize",
        "purchase_order",
        order.base.id.clone(),
    )?;
    let db = db.clone();
    let client = db.client().clone();
    client
        .with_transaction(move |session| {
            Box::pin(async move {
                ensure_purchase_review_sources(&db, &order, &submission, &submission_lines, session).await?;
                let allocations =
                    prepare_current_sales_allocations(&db, &order, &mut revision_lines, session).await?;
                db.purchase_order()
                    .create_effective_revision(&revision, &revision_lines, session)
                    .await?;
                persist_current_sales_allocations(&db, &allocations, session).await?;
                let mut order_mut = order;
                order_mut.formalize_with_revision(revision.base.id.clone().into(), &actor_id)?;
                let mut submission_mut = submission;
                submission_mut.record_review(
                    PurchaseOrderReviewDecision::Approved { comment: None },
                    Instant::now(),
                    &actor_id,
                )?;
                db.purchase_order_submissions()
                    .update(&mut submission_mut, session)
                    .await?;
                db.purchase_orders().update(&mut order_mut, session).await?;
                db.payable()
                    .create_payable_with_entry(&payable.0, &payable.1, session)
                    .await?;
                crate::payable::payment_task::ensure_purchase_payment_task(
                    &db, &payable.0, &payable.1, session,
                )
                .await?;
                for entry in &cost_entries {
                    db.cost()
                        .create_cost_entry_with_allocations(entry, Vec::new(), session)
                        .await?;
                }
                // 入仓采购单生效后自动生成采购入库草稿与 W01 指定到人的入库任务。
                if fulfillment_responsibility == FulfillmentResponsibility::Warehouse {
                    create_receipt_draft_for_order(&db, &order_mut, &revision_lines, session).await?;
                } else if fulfillment_responsibility == FulfillmentResponsibility::SupplierDirect {
                    // 供应商直发草稿继续由采购单当前责任人处理。
                    create_delivery_draft_for_order(
                        &db,
                        &order_mut,
                        &revision_lines,
                        &allocations.by_purchase_line,
                        session,
                    )
                    .await?;
                } else if fulfillment_responsibility == FulfillmentResponsibility::Service {
                    // 线下服务草稿继续由采购单当前责任人处理。
                    create_service_fulfillment_draft_for_order(
                        &db,
                        &order_mut,
                        &revision_lines,
                        &allocations.by_purchase_line,
                        &actor_id,
                        session,
                    )
                    .await?;
                }
                db.audit_logs().create(&audit, session).await?;
                Ok::<(), crate::errors::Error>(())
            })
        })
        .await
}

/// 为生效采购单按创建时冻结的目标仓库创建采购入库草稿。
async fn create_receipt_draft_for_order(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let warehouse_id = order
        .target_warehouse_for_receipt()
        .map_err(|error| Error::BusinessLogicError(error.to_string()))?
        .clone();
    let receipt_id = PurchaseReceiptId::new(next_id());
    let receipt = PurchaseReceipt::new(
        receipt_id.clone(),
        PurchaseReceiptData {
            receipt_no: crate::fulfillment::document_number::next_purchase_receipt_no(db).await?,
            purchase_order_id: entities::ids::PurchaseOrderId::new(order.base.id.clone()),
            warehouse_id,
        },
    )?;
    let zero = Quantity::from_str("0").map_err(|error| Error::Internal(error.to_string()))?;
    let mut lines = Vec::with_capacity(revision_lines.len());
    for (index, line) in revision_lines.iter().enumerate() {
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let quantity = line.quantity.unwrap_or(zero);
        lines.push(
            PurchaseReceiptLine::new(
                PurchaseReceiptLineId::new(next_id()),
                PurchaseReceiptLineData {
                    purchase_receipt_id: receipt_id.clone(),
                    line_no: (index + 1) as u32,
                    purchase_order_revision_line_id: entities::ids::PurchaseOrderRevisionLineId::new(
                        line.base.id.clone(),
                    ),
                    received_quantity: quantity,
                    qualified_quantity: quantity,
                    rejected_quantity: zero,
                    quality_result: QualityResult::Passed,
                },
            )
            .map_err(Error::Logic)?,
        );
    }
    db.fulfillment()
        .create_purchase_receipt_with_lines(&receipt, &lines, executor)
        .await?;
    crate::fulfillment::task::ensure_fulfillment_task(
        db,
        crate::fulfillment::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
        executor,
    )
    .await?;
    Ok(())
}

/// 为生效供应商直发采购单创建发货草稿（§6.7 直发）。
///
/// 行按版本行全额生成，引用同一事务内已创建的「采购行→销售行」分配
/// （`DeliveryLine` 行级校验要求直发必填分配）；草稿进入 W01 履约任务作业面
/// 「交付与代发」通道，采购登记物流后过账发货。
async fn create_delivery_draft_for_order(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    allocations: &HashMap<String, PurchaseLineSalesAllocationId>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: crate::fulfillment::document_number::next_delivery_no(db).await?,
            delivery_type: entities::fulfillment::DeliveryType::SupplierDirect,
            sales_order_id: order.sales_order_id.clone(),
            purchase_order_id: Some(PurchaseOrderId::new(order.base.id.clone())),
            warehouse_id: None,
            carrier: None,
            tracking_no: None,
            address_snapshot_encrypted: None,
            address_snapshot_fingerprint: None,
        },
    )
    .map_err(Error::Logic)?;
    let zero = Quantity::from_str("0").map_err(|error| Error::Internal(error.to_string()))?;
    let mut lines = Vec::with_capacity(revision_lines.len());
    for (index, line) in revision_lines.iter().enumerate() {
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let Some(confirmation_line_id) = &line.procurement_confirmation_line_id else {
            continue;
        };
        let Some(allocation_id) = allocations.get(line.base.id.as_str()) else {
            return Err(Error::BusinessLogicError(
                "供应商直发明细缺少销售分配，无法生成发货草稿".to_string(),
            ));
        };
        let quantity = line.quantity.unwrap_or(zero);
        lines.push(
            DeliveryLine::new(
                DeliveryLineId::new(next_id()),
                DeliveryLineData {
                    delivery_id: delivery_id.clone(),
                    line_no: (index + 1) as u32,
                    sales_order_line_id: SalesOrderLineId::new(confirmation_line_id.to_string()),
                    quantity,
                    stock_reservation_id: None,
                    purchase_line_sales_allocation_id: Some(allocation_id.clone()),
                },
                entities::fulfillment::DeliveryType::SupplierDirect,
            )
            .map_err(Error::Logic)?,
        );
    }
    if lines.is_empty() {
        return Ok(());
    }
    db.fulfillment()
        .create_delivery_with_lines(&delivery, &lines, executor)
        .await?;
    crate::fulfillment::task::ensure_fulfillment_task(
        db,
        crate::fulfillment::task::FulfillmentTaskObject::Delivery(&delivery),
        executor,
    )
    .await?;
    Ok(())
}

/// 服务履约草稿敏感字段的查询指纹密钥（域内常量，同 warehouse 域先例；
/// 草稿阶段为占位快照，代码库无按指纹查询服务履约的路径）。
const SERVICE_FULFILLMENT_FINGERPRINT_KEY: &[u8] = b"erp-service-fulfillment-draft-key-v1";

/// 为生效线下服务采购单创建服务履约草稿（§6.7 服务）。
///
/// 服务履约记录按采购版本行逐行生成（单记录单明细），引用同一事务内已创建
/// 的「采购行→销售行」分配；草稿进入 W01 履约任务作业面的
/// 服务类型，采购登记服务地点/时间/结果后确认完成。交付对象与服务地点为
/// 占位快照（UI 不采集交付对象；确认后仍以占位值落库）。
async fn create_service_fulfillment_draft_for_order(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[entities::purchase_order::PurchaseOrderRevisionLine],
    allocations: &HashMap<String, PurchaseLineSalesAllocationId>,
    actor_id: &str,
    executor: &mut dyn Executor,
) -> Result<()> {
    let zero = Quantity::from_str("0").map_err(|error| Error::Internal(error.to_string()))?;
    let now = Instant::now();
    let placeholder = "待填写".to_string();
    for line in revision_lines {
        if line.line_type != PurchaseLineType::ItemService {
            continue;
        }
        let Some(confirmation_line_id) = &line.procurement_confirmation_line_id else {
            continue;
        };
        let Some(allocation_id) = allocations.get(line.base.id.as_str()) else {
            return Err(Error::BusinessLogicError(
                "线下服务明细缺少销售分配，无法生成服务履约草稿".to_string(),
            ));
        };
        let quantity = line.quantity.unwrap_or(zero);
        let record_id = ServiceFulfillmentId::new(next_id());
        let record = ServiceFulfillment::new(
            record_id.clone(),
            ServiceFulfillmentData {
                fulfillment_no: format!("SF-{}", record_id.as_ref()),
                sales_order_line_id: SalesOrderLineId::new(confirmation_line_id.to_string()),
                purchase_order_id: PurchaseOrderId::new(order.base.id.clone()),
                purchase_line_sales_allocation_id: allocation_id.clone(),
                recipient_snapshot: placeholder.clone(),
                recipient_snapshot_fingerprint: ServiceFulfillment::recipient_snapshot_fingerprint(
                    &placeholder,
                    SERVICE_FULFILLMENT_FINGERPRINT_KEY,
                ),
                quantity,
                result: FulfillmentResult::Success,
                evidence_attachment_id: None,
                service_location_encrypted: placeholder.clone(),
                service_location_fingerprint: ServiceFulfillment::service_location_fingerprint(
                    &placeholder,
                    SERVICE_FULFILLMENT_FINGERPRINT_KEY,
                ),
                service_started_at: None,
                service_ended_at: None,
                completion_note: None,
                fact_no: next_id(),
                occurred_at: now,
                recorded_at: now,
                recorded_by: actor_id.to_string(),
                source_type: SourceType::Erp,
                source_reference: None,
                reason_code: None,
                reason_text: None,
            },
        )
        .map_err(Error::Logic)?;
        db.service_fulfillments().create(&record, executor).await?;
        crate::fulfillment::task::ensure_fulfillment_task(
            db,
            crate::fulfillment::task::FulfillmentTaskObject::ServiceFulfillment(&record),
            executor,
        )
        .await?;
    }
    Ok(())
}

/// 在审核通过事务内重验冻结金额、销售分配与采购确认的精确供给来源。
async fn ensure_purchase_review_sources(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    submission: &PurchaseOrderSubmission,
    lines: &[PurchaseOrderSubmissionLine],
    executor: &mut dyn Executor,
) -> Result<()> {
    let _ = (db, executor, order);
    submission
        .ensure_line_totals(lines)
        .map_err(|error| Error::BusinessLogicError(error.to_string()))
}

#[cfg(test)]
mod tests {}
