//! 采购财务审核、应付与成本事实编排。

use std::collections::HashMap;
use std::str::FromStr;

use erp_core::common::source::SourceType;
use erp_core::common::time::Instant;
use erp_core::ids::{PurchaseLineSalesAllocationId, PurchaseOrderId, PurchaseReceiptId, SalesOrderLineId};
use erp_core::money::Quantity;
use erp_fulfillment::entity::fulfillment::{
    Delivery, DeliveryData, DeliveryId, DeliveryLine, DeliveryLineData, DeliveryLineId, FulfillmentResult,
    PurchaseReceipt, PurchaseReceiptData, PurchaseReceiptLine, PurchaseReceiptLineData,
    PurchaseReceiptLineId, QualityResult, ServiceFulfillment, ServiceFulfillmentData, ServiceFulfillmentId,
};
use erp_fulfillment::repository::FulfillmentExt;
use erp_procurement::entity::purchase_order::{
    FulfillmentResponsibility, PurchaseLineType, PurchaseOrder, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine,
};
use erp_procurement::repository::PurchaseOrderExt;
use erp_workflow::entity::work_item::WorkItemStatus;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};

use super::allocation_maintenance::prepare_current_sales_allocations;
use super::PurchaseOrderProcess;
use crate::{Error, Result};
use application_core::AuditActor;
use erp_procurement::dto::purchase_order::PurchaseReviewResult;

impl PurchaseOrderProcess {
    /// 读取并完成采购形式化的事务外领域计算。
    ///
    /// # 错误
    /// 单据、提交或来源事实不完整时返回错误。
    pub async fn prepare_formalized_order(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<PreparedFormalizedOrder> {
        use super::adapter::{execute_purchase_order_domain_action, purchase_order_adapter};

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
        let revision_no = self.domain().next_revision_no(&order).await?;
        let (revision, revision_lines) = self
            .domain()
            .build_effective_revision(&order, &submission, &submission_lines, revision_no)
            .await?;
        let payable = self
            .build_payable(&order, &submission, &submission_lines, actor.id())
            .await?;
        let cost_entries = self
            .build_confirmed_cost_entries(&submission, &submission_lines, revision_no)
            .await?;
        let result = PurchaseReviewResult {
            work_item_id: String::new(),
            work_item_status: WorkItemStatus::Completed.as_str().to_string(),
            task_version: "0".to_string(),
            subject_version: order.approval_subject_version.to_string(),
            review_result: "APPROVED".to_string(),
            revision_id: Some(revision.base.id.clone()),
            revision_no: Some(revision_no),
            payable_entry_id: Some(payable.1.base.id.clone()),
            lock_version: order.base.version,
            reference: format!("PO-V{revision_no}"),
        };
        Ok(PreparedFormalizedOrder {
            persist: FormalizedOrderPersist {
                order,
                submission,
                submission_lines,
                revision,
                revision_lines,
                payable,
                cost_entries,
            },
            result,
        })
    }

    /// 构建应付子账与原始应付分录（D19；子账按采购单维度）。
    async fn build_payable(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        submission_lines: &[PurchaseOrderSubmissionLine],
        actor_id: &str,
    ) -> Result<(
        erp_finance::entity::payable::PayableAccount,
        erp_finance::entity::payable::PayableEntry,
    )> {
        let expected_delivery_on = submission_lines
            .iter()
            .filter(|line| line.line_type == PurchaseLineType::ItemService)
            .filter_map(|line| line.expected_delivery_date)
            .max();
        let due_date = submission
            .payment_term_snapshot
            .payable_due_date(
                erp_core::common::time::BusinessDate::today(),
                expected_delivery_on,
                super::adapters::payment_term::parse_snapshot,
            )
            .map_err(Error::Logic)?;
        Ok(erp_finance::service::payable::purchase_initial::prepare(
            erp_finance::service::payable::purchase_initial::InitialPurchasePayable {
                order_id: &order.base.id,
                submission_id: &submission.base.id,
                supplier_id: order.supplier_id.clone(),
                gross_amount: submission.gross_amount,
                due_date,
            },
            actor_id,
        )?)
    }

    /// 构建 `CONFIRMED` 成本事实（D20；逐采购行一个成本事实）。
    async fn build_confirmed_cost_entries(
        &self,
        submission: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
        revision_no: u32,
    ) -> Result<Vec<erp_finance::entity::cost::CostEntry>> {
        let facts = lines
            .iter()
            .map(
                |line| erp_finance::service::cost::purchase_initial::PurchaseCostLine {
                    id: line.base.id.clone(),
                    is_logistics: line.line_type == PurchaseLineType::LogisticsFee,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                },
            )
            .collect::<Vec<_>>();
        Ok(erp_finance::service::cost::purchase_initial::prepare(
            submission.purchase_order_id.as_ref(),
            &submission.supplier_id,
            &facts,
            revision_no,
        )?)
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
pub struct PreparedFormalizedOrder {
    /// 事务内待写事实。
    persist: FormalizedOrderPersist,
    /// 兼容现有服务调用方的结果视图。
    result: PurchaseReviewResult,
}

/// 采购正式版本与财务事实的冻结计划；只能由组合流程消费。
pub struct FormalizedOrderPersist {
    /// 待正式化的采购单。
    order: PurchaseOrder,
    /// 待记录结论的提交。
    submission: PurchaseOrderSubmission,
    /// 提交行。
    submission_lines: Vec<PurchaseOrderSubmissionLine>,
    /// 生效版本。
    revision: erp_procurement::entity::purchase_order::PurchaseOrderRevision,
    /// 生效版本行。
    revision_lines: Vec<erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine>,
    /// 应付账户与分录。
    payable: (
        erp_finance::entity::payable::PayableAccount,
        erp_finance::entity::payable::PayableEntry,
    ),
    /// 确认成本分录。
    cost_entries: Vec<erp_finance::entity::cost::CostEntry>,
}

/// 为生效采购单按创建时冻结的目标仓库创建采购入库草稿。
async fn create_receipt_draft_for_order(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
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
            receipt_no: erp_fulfillment::service::document_number::next_purchase_receipt_no(db).await?,
            purchase_order_id: erp_core::ids::PurchaseOrderId::new(order.base.id.clone()),
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
                    purchase_order_revision_line_id: erp_core::ids::PurchaseOrderRevisionLineId::new(
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
    crate::fulfillment_execution::task::ensure_fulfillment_task(
        db,
        crate::fulfillment_execution::task::FulfillmentTaskObject::PurchaseReceipt(&receipt),
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
    revision_lines: &[erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
    allocations: &HashMap<String, PurchaseLineSalesAllocationId>,
    executor: &mut dyn Executor,
) -> Result<()> {
    let delivery_id = DeliveryId::new(next_id());
    let delivery = Delivery::new(
        delivery_id.clone(),
        DeliveryData {
            delivery_no: erp_fulfillment::service::document_number::next_delivery_no(db).await?,
            delivery_type: erp_fulfillment::entity::fulfillment::DeliveryType::SupplierDirect,
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
                erp_fulfillment::entity::fulfillment::DeliveryType::SupplierDirect,
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
    crate::fulfillment_execution::task::ensure_fulfillment_task(
        db,
        crate::fulfillment_execution::task::FulfillmentTaskObject::Delivery(&delivery),
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
/// 服务类型。采购登记服务地点、时间、结果和现场图片凭证后确认完成。
/// 交付对象为占位快照（UI 不采集）；服务地点占位值必须在确认时替换。
async fn create_service_fulfillment_draft_for_order(
    db: &mongodb::Database,
    order: &PurchaseOrder,
    revision_lines: &[erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine],
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
        crate::fulfillment_execution::task::ensure_fulfillment_task(
            db,
            crate::fulfillment_execution::task::FulfillmentTaskObject::ServiceFulfillment(&record),
            executor,
        )
        .await?;
    }
    Ok(())
}

impl PreparedFormalizedOrder {
    /// 将计算结果与一次性写入计划交给采购形式化流程。
    pub fn into_parts(self) -> (FormalizedOrderPersist, PurchaseReviewResult) {
        (self.persist, self.result)
    }
}

/// 已写入采购版本后，等待财务和履约消费的冻结事实。
pub struct FormalizedPurchaseEffects {
    order: PurchaseOrder,
    revision_lines: Vec<erp_procurement::entity::purchase_order::PurchaseOrderRevisionLine>,
    allocations_by_line: HashMap<String, PurchaseLineSalesAllocationId>,
    payable: (
        erp_finance::entity::payable::PayableAccount,
        erp_finance::entity::payable::PayableEntry,
    ),
    cost_entries: Vec<erp_finance::entity::cost::CostEntry>,
}

impl FormalizedOrderPersist {
    /// 本次正式化的采购单标识，供组合层创建原成功审计。
    pub fn order_id(&self) -> &str {
        &self.order.base.id
    }

    /// 重验采购来源、持久化正式版本和销售分配并推进采购状态。
    ///
    /// # 错误
    /// 来源、版本或 CAS 校验失败时，在调用方唯一事务内传播错误。
    pub async fn persist_order(
        self,
        db: &mongodb::Database,
        actor: &AuditActor,
        session: &mut dyn Executor,
    ) -> Result<FormalizedPurchaseEffects> {
        let persist = self;
        let FormalizedOrderPersist {
            order,
            submission,
            submission_lines,
            revision,
            mut revision_lines,
            payable,
            cost_entries,
        } = persist;
        let actor_id = actor.id().to_string();
        erp_procurement::service::purchase_order::formalization::ensure_review_sources(
            &submission,
            &submission_lines,
        )?;
        let allocations = prepare_current_sales_allocations(db, &order, &mut revision_lines, session).await?;
        let order_mut = erp_procurement::service::purchase_order::formalization::persist_formalized_order(
            db,
            erp_procurement::service::purchase_order::formalization::FormalizedOrderWrite {
                order,
                submission,
                revision: &revision,
                revision_lines: &revision_lines,
                allocations: &allocations,
            },
            &actor_id,
            session,
        )
        .await?;

        Ok(FormalizedPurchaseEffects {
            order: order_mut,
            revision_lines,
            allocations_by_line: allocations.by_purchase_line,
            payable,
            cost_entries,
        })
    }
}

impl FormalizedPurchaseEffects {
    /// 本次原始应付账户与分录；须先写入后创建付款工作项。
    pub fn payable(
        &self,
    ) -> &(
        erp_finance::entity::payable::PayableAccount,
        erp_finance::entity::payable::PayableEntry,
    ) {
        &self.payable
    }

    /// 按原采购提交顺序产生的确认成本分录。
    pub fn cost_entries(&self) -> &[erp_finance::entity::cost::CostEntry] {
        &self.cost_entries
    }

    /// 财务成功后按冻结履约责任创建原履约草稿及履约任务。
    ///
    /// # 错误
    /// 履约事实不完整或持久化失败时传播给调用方事务。
    pub async fn persist_fulfillment(
        self,
        db: &mongodb::Database,
        actor_id: &str,
        session: &mut dyn Executor,
    ) -> Result<()> {
        if self.order.fulfillment_responsibility == FulfillmentResponsibility::Warehouse {
            create_receipt_draft_for_order(db, &self.order, &self.revision_lines, session).await?;
        } else if self.order.fulfillment_responsibility == FulfillmentResponsibility::SupplierDirect {
            create_delivery_draft_for_order(
                db,
                &self.order,
                &self.revision_lines,
                &self.allocations_by_line,
                session,
            )
            .await?;
        } else if self.order.fulfillment_responsibility == FulfillmentResponsibility::Electronic {
            super::electronic_drafts::create_electronic_drafts(
                db,
                &self.order,
                &self.revision_lines,
                &self.allocations_by_line,
                actor_id,
                session,
            )
            .await?;
        } else if self.order.fulfillment_responsibility == FulfillmentResponsibility::Service {
            create_service_fulfillment_draft_for_order(
                db,
                &self.order,
                &self.revision_lines,
                &self.allocations_by_line,
                actor_id,
                session,
            )
            .await?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {}
