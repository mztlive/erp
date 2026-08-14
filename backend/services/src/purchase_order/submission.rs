//! 采购草稿冻结、提交审核与待办构造。

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, Transactional, WorkItemExt};
use entities::common::time::Instant;
use entities::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, WorkItemId};
use entities::purchase_order::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, SubmissionStatus,
};
use entities::work_item::{
    AssignmentMode, AssignmentSource, WorkItem, WorkItemData, WorkItemPriority, WorkItemType,
};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::dto::{SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

const PURCHASE_SUBMIT_RECEIPT_PREFIX: &str = "purchase-submit-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

impl PurchaseOrderService {
    /// 提交财务审核（§6.6：头行冻结，形成不可变提交与审核待办）。
    ///
    /// 单事务写入提交、提交明细、采购主表指针、审核待办与幂等收据；同一键
    /// 重试返回原结果，不同键重复提交失败，提交序号唯一索引兜底。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交结果（提交 ID、序号与审核待办）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 期望版本不一致或重复提交
    /// * `BusinessLogicError` - 状态非草稿或草稿内容缺失
    pub async fn submit(
        &self,
        id: &str,
        req: SubmitPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmitPurchaseOrderResult> {
        req.validate()?;
        let action = "purchase_order.submit";
        let fingerprint = command_fingerprint(&[id, &req.expected_lock_version.to_string()]);
        let audit_id = purchase_submit_audit_id(actor.id(), id, &req.idempotency_key);
        if let Some(result) = self.replay_purchase_submit(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        let mut order = self
            .db
            .purchase_orders()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
        self.ensure_version(&order, req.expected_lock_version)?;
        if order.stable.status != PurchaseOrderStatus::Draft {
            return Err(Error::ConflictError(
                "采购单已提交或已生效，请勿重复提交".to_string(),
            ));
        }
        let draft_id = order
            .current_submission_id
            .as_ref()
            .map(ToString::to_string)
            .ok_or_else(|| Error::BusinessLogicError("采购单缺少草稿提交".to_string()))?;
        let mut draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        if draft.status != SubmissionStatus::Draft {
            return Err(Error::ConflictError("草稿提交已冻结".to_string()));
        }
        let mut draft_lines = self
            .db
            .purchase_order_submission_lines()
            .find_many(
                mongodb::bson::doc! { "purchase_order_submission_id": draft_id },
                &mut NoTransaction,
            )
            .await?;

        // 形成新的正式提交（复制草稿内容，冻结）。
        let mut superseded_draft = draft.clone();
        superseded_draft.mark_superseded()?;
        let submission = self
            .freeze_submission(&mut order, &mut draft, &mut draft_lines, actor)
            .await?;
        let work_item = self.build_review_work_item(&order, &submission)?;

        let audit_actor = actor.clone();
        let audit_id_for_tx = audit_id.clone();
        let fingerprint_for_tx = fingerprint.clone();
        let db = self.db.clone();
        let client = db.client().clone();
        let mut order_for_tx = order.clone();
        let work_item_for_tx = work_item.clone();
        let submission_for_tx = submission.clone();
        let superseded_draft_for_tx = superseded_draft.clone();
        let transaction_result = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.purchase_order()
                        .create_purchase_submission(
                            &mut order_for_tx,
                            &submission_for_tx,
                            &draft_lines,
                            session,
                        )
                        .await?;
                    db.purchase_order_submissions()
                        .update(&mut superseded_draft_for_tx.clone(), session)
                        .await?;
                    db.work_items().create(&work_item_for_tx, session).await?;
                    let receipt = PurchaseSubmitReceipt {
                        submission_id: submission_for_tx.base.id.clone(),
                        submission_no: submission_for_tx.submission_no.clone(),
                        work_item_id: work_item_for_tx.base.id.clone(),
                        task_version: work_item_for_tx.base.version,
                        subject_version: work_item_for_tx.subject_version.clone(),
                        lock_version: order_for_tx.base.version,
                    };
                    let audit = audit_actor.resource_log_with_id(
                        audit_id_for_tx,
                        action,
                        "purchase_order",
                        order_for_tx.base.id.clone(),
                        Some(purchase_submit_receipt_message(&fingerprint_for_tx, &receipt)),
                    )?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<PurchaseSubmitReceipt, crate::errors::Error>(receipt)
                })
            })
            .await;
        let receipt = match transaction_result {
            Ok(receipt) => receipt,
            Err(error) => {
                if let Some(result) = self.replay_purchase_submit(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            }
        };

        Ok(SubmitPurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            submission_id: receipt.submission_id,
            submission_no: receipt.submission_no.clone(),
            work_item_id: receipt.work_item_id,
            task_version: receipt.task_version,
            subject_version: receipt.subject_version,
            lock_version: receipt.lock_version,
            reference: receipt.submission_no,
        })
    }

    /// 重放已提交的采购冻结命令，并拒绝同一键混用不同对象版本。
    async fn replay_purchase_submit(
        &self,
        audit_id: &str,
        expected_fingerprint: &str,
        purchase_order_id: &str,
    ) -> Result<Option<SubmitPurchaseOrderResult>> {
        let Some(audit) = self
            .db
            .audit_logs()
            .find_by_id(audit_id, &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        if audit.resource_id.as_deref() != Some(purchase_order_id) {
            return Err(Error::Internal("采购提交幂等收据与业务对象不一致".to_string()));
        }
        let receipt = parse_purchase_submit_receipt(
            audit
                .message
                .as_deref()
                .ok_or_else(|| Error::Internal("采购提交幂等收据缺少结果".to_string()))?,
            expected_fingerprint,
        )?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("采购提交幂等收据引用的采购单不存在".to_string()))?;
        Ok(Some(SubmitPurchaseOrderResult {
            purchase_order_id: purchase_order_id.to_string(),
            purchase_no: order.purchase_no,
            submission_id: receipt.submission_id,
            submission_no: receipt.submission_no.clone(),
            work_item_id: receipt.work_item_id,
            task_version: receipt.task_version,
            subject_version: receipt.subject_version,
            lock_version: receipt.lock_version,
            reference: receipt.submission_no,
        }))
    }

    /// 冻结草稿为正式提交（复制明细并重指向正式提交、推进主表指针）。
    async fn freeze_submission(
        &self,
        order: &mut PurchaseOrder,
        draft: &mut PurchaseOrderSubmission,
        draft_lines: &mut [PurchaseOrderSubmissionLine],
        actor: &AuditActor,
    ) -> Result<PurchaseOrderSubmission> {
        let next_no = self.next_submission_no(order).await?;
        let formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no.clone(),
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: draft.gross_amount,
                net_amount: draft.net_amount,
                tax_amount: draft.tax_amount,
            },
        )?;
        let mut formal = formal;
        formal.submit(Instant::now(), actor.id())?;
        // 正式提交行复制草稿快照并生成新身份；不得修改或复用旧草稿行主键。
        for line in draft_lines.iter_mut() {
            *line = PurchaseOrderSubmissionLine::new(
                PurchaseOrderSubmissionLineId::new(next_id()),
                PurchaseOrderSubmissionLineData {
                    purchase_order_submission_id: formal.base.id.clone().into(),
                    line_no: line.line_no,
                    line_type: line.line_type,
                    procurement_confirmation_line_id: line.procurement_confirmation_line_id.clone(),
                    sku_id: line.sku_id.clone(),
                    sku_revision_id: line.sku_revision_id.clone(),
                    product_name_snapshot: line.product_name_snapshot.clone(),
                    specification_snapshot: line.specification_snapshot.clone(),
                    quantity: line.quantity,
                    base_unit_code: line.base_unit_code.clone(),
                    unit_cost_gross: line.unit_cost_gross,
                    gross_amount: line.gross_amount,
                    net_amount: line.net_amount,
                    tax_amount: line.tax_amount,
                    input_tax_rate: line.input_tax_rate,
                    expected_delivery_date: line.expected_delivery_date,
                    sales_order_submission_line_id: line.sales_order_submission_line_id.clone(),
                    allocated_quantity: line.allocated_quantity,
                },
            )?;
        }
        order.submit_for_review(formal.base.id.clone(), actor.id())?;
        Ok(formal)
    }

    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order_submissions()
            .find_many(
                mongodb::bson::doc! { "purchase_order_id": order.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let max_no = existing
            .iter()
            .filter_map(|submission| {
                submission
                    .submission_no
                    .strip_prefix("SUB-")
                    .and_then(|value| value.parse::<u32>().ok())
            })
            .max()
            .unwrap_or(0);
        Ok(format!("SUB-{:06}", max_no + 1))
    }

    /// 构建审核待办（D03）。
    fn build_review_work_item(
        &self,
        order: &PurchaseOrder,
        submission: &PurchaseOrderSubmission,
    ) -> Result<WorkItem> {
        WorkItem::new(
            WorkItemId::new(next_id()),
            WorkItemData {
                work_item_type: WorkItemType::PurchaseOrderReview,
                approval_step_instance_id: None,
                business_object_type: "purchase_order".to_string(),
                business_object_id: order.base.id.clone(),
                subject_version: submission.base.id.clone(),
                assignment_mode: AssignmentMode::Pool,
                owner_role: "role-finance".to_string(),
                owner_organization_id: "company".to_string(),
                owner_user_id: None,
                assignment_source: AssignmentSource::SystemRule,
                priority: WorkItemPriority::Normal,
                due_at: None,
                reason_code: None,
                impact_summary: Some(format!("采购单 {} 待财务审核", order.purchase_no)),
            },
        )
        .map_err(Into::into)
    }
}

/// 采购提交命令的最小、可重放结果收据。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PurchaseSubmitReceipt {
    submission_id: String,
    submission_no: String,
    work_item_id: String,
    task_version: u64,
    subject_version: String,
    lock_version: u64,
}

/// 生成不暴露原始幂等键的稳定采购提交收据主键。
fn purchase_submit_audit_id(actor_id: &str, purchase_order_id: &str, key: &str) -> String {
    format!(
        "{PURCHASE_SUBMIT_RECEIPT_PREFIX}{}",
        stable_digest(&format!(
            "{actor_id}|purchase_order.submit|{purchase_order_id}|{key}"
        ))
    )
}

/// 把采购提交结果编码为审计消息。
fn purchase_submit_receipt_message(fingerprint: &str, receipt: &PurchaseSubmitReceipt) -> String {
    format!(
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}|{}|{}",
        receipt.submission_id,
        receipt.submission_no,
        receipt.work_item_id,
        receipt.task_version,
        receipt.subject_version,
        receipt.lock_version,
    )
}

/// 解析并校验采购提交命令收据。
fn parse_purchase_submit_receipt(message: &str, expected_fingerprint: &str) -> Result<PurchaseSubmitReceipt> {
    let (fingerprint, result) = message
        .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
        .and_then(|value| value.split_once(";result="))
        .ok_or_else(|| Error::Internal("采购提交幂等收据格式非法".to_string()))?;
    if fingerprint != expected_fingerprint {
        return Err(Error::ConflictError("幂等键已用于不同的采购提交命令".to_string()));
    }
    let fields = result.split('|').collect::<Vec<_>>();
    let [submission_id, submission_no, work_item_id, task_version, subject_version, lock_version] =
        fields.as_slice()
    else {
        return Err(Error::Internal("采购提交幂等收据结果非法".to_string()));
    };
    Ok(PurchaseSubmitReceipt {
        submission_id: (*submission_id).to_string(),
        submission_no: (*submission_no).to_string(),
        work_item_id: (*work_item_id).to_string(),
        task_version: parse_receipt_number(task_version, "待办版本")?,
        subject_version: (*subject_version).to_string(),
        lock_version: parse_receipt_number(lock_version, "采购单版本")?,
    })
}

/// 解析收据中的整数版本字段。
fn parse_receipt_number<T>(value: &str, field: &str) -> Result<T>
where
    T: std::str::FromStr,
{
    value
        .parse()
        .map_err(|_| Error::Internal(format!("采购提交幂等收据{field}非法")))
}

/// 对字段逐项加入长度前缀后计算命令载荷摘要。
fn command_fingerprint(parts: &[&str]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    format!("{:x}", digest.finalize())
}

/// 计算稳定 SHA-256 十六进制摘要。
fn stable_digest(value: &str) -> String {
    format!("{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::{
        parse_purchase_submit_receipt, purchase_submit_audit_id, purchase_submit_receipt_message,
        PurchaseSubmitReceipt,
    };

    #[test]
    fn submit_receipt_round_trips_and_hides_raw_key() {
        let fingerprint = "b".repeat(64);
        let receipt = PurchaseSubmitReceipt {
            submission_id: "submission-1".to_string(),
            submission_no: "SUB-000001".to_string(),
            work_item_id: "wi-1".to_string(),
            task_version: 1,
            subject_version: "submission-1".to_string(),
            lock_version: 2,
        };
        let message = purchase_submit_receipt_message(&fingerprint, &receipt);

        assert_eq!(
            parse_purchase_submit_receipt(&message, &fingerprint).unwrap(),
            receipt
        );
        let audit_id = purchase_submit_audit_id("actor-1", "po-1", "raw-secret-key");
        assert!(!audit_id.contains("raw-secret-key"));
        assert!(message.len() <= 256);
    }
}
