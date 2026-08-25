//! 采购草稿冻结并调用统一 `start_approval`。

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, SalesOrderExt};
use entities::common::time::Instant;
use entities::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use entities::purchase_order::{
    PurchaseOrder, PurchaseOrderStatus, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, SubmissionStatus,
};
use id_generator::next_id;
use sha2::{Digest, Sha256};
use validator::Validate;

use super::adapter::{
    build_purchase_order_snapshot, execute_purchase_order_domain_action, purchase_order_adapter,
    purchase_order_object_readable, purchase_order_responsible_org_id, purchase_order_start_command,
    purchase_order_subject_ref, require_frozen_binding, start_approval_command_kind, RECENT_HISTORY_LIMIT,
};
use super::draft_edit::{ensure_payment_term_unchanged, resolve_line_patches};
use super::dto::{SavePurchaseOrderLine, SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult};
use super::start_approval::{
    build_purchase_order_start_input, load_bound_definition_graph, load_start_receipt,
    persist_purchase_order_start, PurchaseOrderStartInput, PurchaseOrderStartPersistInput,
    PurchaseSubmitProcurementGuard,
};
use super::PurchaseOrderService;
use crate::approval::execution::prepare_start;
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, find_registered_document};
use crate::errors::{Error, Result};

const PURCHASE_SUBMIT_RECEIPT_PREFIX: &str = "purchase-submit-command-";
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";

impl PurchaseOrderService {
    /// 提交采购单并调用统一 `start_approval`。
    ///
    /// 同一事务内：锁定采购单与 `BusinessDocument`；若尚无正式号则分配不可复用
    /// `purchase_no` 并一次性写入两者；冻结提交；递增 `approval_subject_version`
    /// 并启动审批。无绑定或无发布定义时失败关闭。
    ///
    /// # 参数
    /// * `id` - 采购单 ID
    /// * `req` - 提交请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回提交结果（正式号、提交 ID 与冻结版本）。
    ///
    /// # 错误
    /// * `NotFound` - 采购单不存在
    /// * `ConflictError` - 期望版本不一致、无绑定或重复提交
    /// * `BusinessLogicError` - 状态非草稿或草稿内容缺失
    pub async fn submit(
        &self,
        id: &str,
        req: SubmitPurchaseOrderRequest,
        actor: &AuditActor,
    ) -> Result<SubmitPurchaseOrderResult> {
        req.validate()?;
        for patch in &req.line_patches {
            patch.validate()?;
        }
        let action = "purchase_order.submit";
        let request_shape = format!("{:?}|{:?}", req.payment_term_code, req.line_patches);
        let fingerprint = command_fingerprint(&[id, &req.expected_lock_version.to_string(), &request_shape]);
        let audit_id = purchase_submit_audit_id(actor.id(), id, &req.idempotency_key);
        if let Some(result) = self.replay_purchase_submit(&audit_id, &fingerprint, id).await? {
            return Ok(result);
        }
        let adapter = purchase_order_adapter()?;
        let subject = purchase_order_subject_ref(id)?;
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
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
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
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let organization_id = purchase_order_responsible_org_id(&sales_order)?;
        let _ = purchase_order_object_readable(&organization_id, actor.id())?;
        assign_formal_purchase_no(&mut order)?;
        let mut document = find_registered_document(&self.db, id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("业务单据未注册".to_string()))?;
        let now = Instant::now();
        if document.document_no.is_empty() {
            document.assign_document_no(order.purchase_no.clone(), now)?;
        }
        let mut superseded_draft = draft.clone();
        superseded_draft.mark_superseded()?;
        let procurement_guard = if req.line_patches.is_empty() {
            None
        } else {
            ensure_payment_term_unchanged(&order.payment_term_code, req.payment_term_code.as_deref())?;
            let existing_lines = draft_lines.clone();
            let requested_lines = resolve_line_patches(&req.line_patches, &existing_lines)?;
            let (submission, lines) = self
                .freeze_submission_from_lines(&order, &draft, &requested_lines, actor)
                .await?;
            draft_lines = lines;
            draft = submission;
            Some(PurchaseSubmitProcurementGuard {
                requested_lines,
                existing_lines,
                actor_id: actor.id().to_string(),
            })
        };
        let submission = if procurement_guard.is_some() {
            draft
        } else {
            self.freeze_submission(&mut order, &mut draft, &mut draft_lines, actor)
                .await?
        };
        execute_purchase_order_domain_action(
            &mut order,
            ApprovalDomainAction::PurchaseOrderSubmit,
            &submission.base.id,
            actor.id(),
        )?;
        let snapshot =
            build_purchase_order_snapshot(&order, &sales_order, &submission, &draft_lines, actor.id(), now)?;
        let start = purchase_order_start_command(
            id,
            order.approval_subject_version,
            actor.id(),
            &req.idempotency_key,
        );
        let owner_role = adapter.owner_role;
        let _ = (start_approval_command_kind(&start), RECENT_HISTORY_LIMIT);
        let graph = load_bound_definition_graph(&self.db, &binding).await?;
        let existing_receipt = load_start_receipt(
            &self.db,
            &subject,
            order.approval_subject_version,
            &req.idempotency_key,
        )
        .await?;
        let start_input = build_purchase_order_start_input(PurchaseOrderStartInput {
            graph,
            binding: &binding,
            subject,
            subject_version: order.approval_subject_version,
            actor_id: actor.id(),
            organization_id: &organization_id,
            idempotency_key: &req.idempotency_key,
            receipt: existing_receipt,
            now,
        })?;
        let prepared = prepare_start(start_input)?;
        let audit = actor.clone().resource_log_with_id(
            audit_id.clone(),
            action,
            "purchase_order",
            order.base.id.clone(),
            Some(purchase_submit_receipt_message(
                &fingerprint,
                &PurchaseSubmitReceipt {
                    purchase_no: order.purchase_no.clone(),
                    submission_id: submission.base.id.clone(),
                    submission_no: submission.submission_no.clone(),
                    work_item_id: String::new(),
                    task_version: 0,
                    subject_version: order.approval_subject_version.to_string(),
                    lock_version: order.base.version,
                },
            )),
        )?;
        let first_task = persist_purchase_order_start(
            &self.db,
            PurchaseOrderStartPersistInput {
                order: order.clone(),
                document,
                superseded_draft,
                submission: submission.clone(),
                submission_lines: draft_lines,
                procurement_guard,
                snapshot_payload: snapshot,
                prepared,
                owner_role,
                organization_id,
                now,
                audit,
            },
        )
        .await;
        let first_task = match first_task {
            Ok(task) => task,
            Err(error) => {
                if let Some(result) = self.replay_purchase_submit(&audit_id, &fingerprint, id).await? {
                    return Ok(result);
                }
                return Err(error);
            }
        };
        let (work_item_id, task_version) = first_task.unwrap_or((String::new(), 0));
        Ok(SubmitPurchaseOrderResult {
            purchase_order_id: order.base.id.clone(),
            purchase_no: order.purchase_no.clone(),
            submission_id: submission.base.id.clone(),
            submission_no: submission.submission_no.clone(),
            work_item_id,
            task_version,
            subject_version: order.approval_subject_version.to_string(),
            lock_version: order.base.version,
            reference: order.purchase_no.clone(),
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
        let _order = self
            .db
            .purchase_orders()
            .find_by_id(purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("采购提交幂等收据引用的采购单不存在".to_string()))?;
        Ok(Some(SubmitPurchaseOrderResult {
            purchase_order_id: purchase_order_id.to_string(),
            purchase_no: receipt.purchase_no.clone(),
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
                    sales_order_line_id: line.sales_order_line_id.clone(),
                    sales_order_revision_line_id: line.sales_order_revision_line_id.clone(),
                    sales_order_submission_line_id: line.sales_order_submission_line_id.clone(),
                    allocated_quantity: line.allocated_quantity,
                },
            )?;
        }
        Ok(formal)
    }

    /// 按提交命令携带的草稿补丁直接构造正式采购提交。
    async fn freeze_submission_from_lines(
        &self,
        order: &PurchaseOrder,
        draft: &PurchaseOrderSubmission,
        requested_lines: &[SavePurchaseOrderLine],
        actor: &AuditActor,
    ) -> Result<(PurchaseOrderSubmission, Vec<PurchaseOrderSubmissionLine>)> {
        let next_no = self.next_submission_no(order).await?;
        let (gross, net, tax) = self.compute_request_totals(requested_lines).await?;
        let mut formal = PurchaseOrderSubmission::new(
            PurchaseOrderSubmissionId::new(next_id()),
            PurchaseOrderSubmissionData {
                purchase_order_id: order.base.id.clone().into(),
                submission_no: next_no,
                supplier_id: draft.supplier_id.clone(),
                purchase_type: draft.purchase_type,
                fulfillment_responsibility: draft.fulfillment_responsibility,
                supplier_revision_id: draft.supplier_revision_id.clone(),
                supplier_snapshot: draft.supplier_snapshot.clone(),
                payment_term_snapshot: draft.payment_term_snapshot.clone(),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
            },
        )?;
        formal.submit(Instant::now(), actor.id())?;
        let lines = self
            .build_lines_from_request(&formal.base.id.clone().into(), requested_lines)
            .await?;
        Ok((formal, lines))
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
}

/// 首次提交分配不可复用正式号。已有正式号时保持不变。
///
/// # 错误
/// 编号非法时返回校验错误。
fn assign_formal_purchase_no(order: &mut PurchaseOrder) -> Result<()> {
    if !order.purchase_no.is_empty() {
        return Ok(());
    }
    Ok(order.assign_purchase_no(format!("PO-{}", order.base.id))?)
}

/// 采购提交命令的最小、可重放结果收据。
#[derive(Debug, Clone, PartialEq, Eq)]
struct PurchaseSubmitReceipt {
    purchase_no: String,
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
        "{COMMAND_FINGERPRINT_PREFIX}{fingerprint};result={}|{}|{}|{}|{}|{}|{}",
        receipt.purchase_no,
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
    let [purchase_no, submission_id, submission_no, work_item_id, task_version, subject_version, lock_version] =
        fields.as_slice()
    else {
        return Err(Error::Internal("采购提交幂等收据结果非法".to_string()));
    };
    Ok(PurchaseSubmitReceipt {
        purchase_no: (*purchase_no).to_string(),
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
    hex::encode(digest.finalize())
}

/// 计算稳定 SHA-256 十六进制摘要。
fn stable_digest(value: &str) -> String {
    hex::encode(Sha256::digest(value.as_bytes()))
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
            purchase_no: "PO-1".to_string(),
            submission_id: "submission-1".to_string(),
            submission_no: "SUB-000001".to_string(),
            work_item_id: "wi-1".to_string(),
            task_version: 1,
            subject_version: "1".to_string(),
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
