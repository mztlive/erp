//! 采购草稿冻结并调用统一 `start_approval`。

use database::{AccessControlExt, NoTransaction, PurchaseOrderExt, SalesOrderExt, Transactional};
use entities::common::time::Instant;
use entities::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId};
use entities::purchase_order::{
    LegacyReceiptIdScheme, PurchaseCommandReceipt, PurchaseCommandReceiptError,
    PurchaseCommandReceiptIdentity, PurchaseOrder, PurchaseOrderSubmission, PurchaseOrderSubmissionData,
    PurchaseOrderSubmissionLine, PurchaseReceiptWire,
};
use id_generator::next_id;
use validator::Validate;

use super::adapter::{
    build_purchase_order_snapshot, execute_purchase_order_domain_action, purchase_order_adapter,
    purchase_order_object_readable, purchase_order_responsible_org_id, purchase_order_start_command,
    purchase_order_subject_ref, require_frozen_binding, start_approval_command_kind, RECENT_HISTORY_LIMIT,
};
use super::draft_edit::map_draft_edit_violation;
use super::dto::{
    SavePurchaseOrderLine, SavePurchaseOrderLinePatch, SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult,
    PURCHASE_SUBMIT_ACTION,
};
use super::line_input::{build_submission_lines, compute_request_totals, to_line_inputs};
use super::start_approval::{
    build_purchase_order_start_input, load_bound_definition_graph, load_start_receipt,
    persist_purchase_order_start, replay_purchase_order_start_with_executor, PurchaseOrderStartInput,
    PurchaseOrderStartPersistInput, PurchaseSubmitProcurementGuard,
};
use super::PurchaseOrderService;
use crate::approval::execution::{command_may_have_committed, command_recovery_delay, prepare_start};
use crate::approval::policy::ApprovalDomainAction;
use crate::audit::AuditActor;
use crate::document_registry::{find_approval_binding, find_registered_document};
use crate::errors::{Error, Result};

const PURCHASE_SUBMIT_RECEIPT_PREFIX: &str = "purchase-submit-command-";

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
        let action = PURCHASE_SUBMIT_ACTION;
        let fingerprint = req.request_fingerprint(id);
        let receipt_identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            PURCHASE_SUBMIT_RECEIPT_PREFIX,
            actor.id(),
            action,
            Some(id),
            &req.idempotency_key,
            LegacyReceiptIdScheme::WholeStringJoined,
        )?;
        let audit_id = receipt_identity.receipt_id().to_string();
        if let Some(result) = self
            .replay_purchase_submit(&receipt_identity, &fingerprint, id, actor)
            .await?
        {
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
        order
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        order
            .ensure_draft_for_submission()
            .map_err(|_| Error::ConflictError("采购单已提交或已生效，请勿重复提交".to_string()))?;
        let binding = find_approval_binding(&self.db, id, &mut NoTransaction).await?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let draft_id = order
            .draft_submission_id()
            .map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let mut draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        draft
            .ensure_draft()
            .map_err(|_| Error::ConflictError("草稿提交已冻结".to_string()))?;
        let mut draft_lines = self
            .db
            .purchase_order()
            .list_submission_lines(&draft_id, &mut NoTransaction)
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
            order
                .ensure_payment_term_unchanged(req.payment_term_code.as_deref())
                .map_err(map_draft_edit_violation)?;
            let existing_lines = draft_lines.clone();
            let requested_lines =
                SavePurchaseOrderLinePatch::resolve_all(&req.line_patches, &existing_lines)?;
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
            None,
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
                receipt: Some((
                    fingerprint.clone(),
                    PurchaseSubmitReceipt {
                        purchase_no: order.purchase_no.clone(),
                        submission_id: submission.base.id.clone(),
                        submission_no: submission.submission_no.clone(),
                        work_item_id: String::new(),
                        task_version: 0,
                        subject_version: order.approval_subject_version.to_string(),
                        lock_version: order.base.version,
                    },
                )),
            },
        )
        .await;
        let first_task = match first_task {
            Ok(task) => task,
            Err(error) if command_may_have_committed(&error) => {
                return self
                    .recover_purchase_submit_start(
                        id,
                        order.approval_subject_version,
                        &req.idempotency_key,
                        actor,
                        &receipt_identity,
                        &fingerprint,
                        error,
                    )
                    .await;
            }
            Err(error) => return Err(error),
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
        identity: &PurchaseCommandReceiptIdentity,
        expected_fingerprint: &str,
        purchase_order_id: &str,
        actor: &AuditActor,
    ) -> Result<Option<SubmitPurchaseOrderResult>> {
        let mut audit = None;
        for candidate in identity.id_candidates() {
            if let Some(found) = self
                .db
                .audit_logs()
                .find_by_id(candidate, &mut NoTransaction)
                .await?
            {
                audit = Some(found);
                break;
            }
        }
        let Some(audit) = audit else {
            return Ok(None);
        };
        let receipt = match PurchaseCommandReceipt::<PurchaseSubmitReceipt>::decode(
            &audit,
            actor.id(),
            PURCHASE_SUBMIT_ACTION,
            Some(purchase_order_id),
            expected_fingerprint,
        ) {
            Ok(receipt) => receipt,
            Err(PurchaseCommandReceiptError::IdentityMismatch) => {
                return Err(Error::Internal("采购提交幂等收据与业务对象不一致".to_string()));
            }
            Err(PurchaseCommandReceiptError::PayloadConflict) => {
                return Err(Error::ConflictError("幂等键已用于不同的采购提交命令".to_string()));
            }
            Err(PurchaseCommandReceiptError::Corrupted(message)) => {
                return Err(Error::Internal(message));
            }
        }
        .into_payload();
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

    /// receipt 唯一竞争、瞬态事务或提交结果未知后，以 fresh session 有界回读。
    #[allow(clippy::too_many_arguments)]
    async fn recover_purchase_submit_start(
        &self,
        purchase_order_id: &str,
        subject_version: u32,
        idempotency_key: &str,
        actor: &AuditActor,
        identity: &PurchaseCommandReceiptIdentity,
        fingerprint: &str,
        original_error: Error,
    ) -> Result<SubmitPurchaseOrderResult> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let db = self.db.clone();
            let purchase_order_id_owned = purchase_order_id.to_string();
            let idempotency_key_owned = idempotency_key.to_string();
            let actor_id = actor.id().to_string();
            let recovered = self
                .db
                .client()
                .with_transaction(move |session| {
                    Box::pin(async move {
                        let order = db
                            .purchase_orders()
                            .find_by_id(&purchase_order_id_owned, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
                        let sales_order = db
                            .sales_orders()
                            .find_by_id(&order.sales_order_id, session)
                            .await?
                            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
                        let organization_id = purchase_order_responsible_org_id(&sales_order)?;
                        let _ = purchase_order_object_readable(&organization_id, &actor_id)?;
                        let binding = find_approval_binding(&db, &purchase_order_id_owned, session).await?;
                        let binding = require_frozen_binding(binding.as_ref())?;
                        let subject = purchase_order_subject_ref(&purchase_order_id_owned)?;
                        replay_purchase_order_start_with_executor(
                            &db,
                            &subject,
                            subject_version,
                            &idempotency_key_owned,
                            binding,
                            &actor_id,
                            session,
                        )
                        .await
                    })
                })
                .await;
            match recovered {
                Ok(Some(_)) => {
                    if let Some(result) = self
                        .replay_purchase_submit(identity, fingerprint, purchase_order_id, actor)
                        .await?
                    {
                        return Ok(result);
                    }
                }
                Ok(None) => {}
                Err(error) if command_may_have_committed(&error) => {}
                Err(error) => return Err(error),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        Err(original_error)
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
        let formal = PurchaseOrderSubmission::freeze_from_draft(
            PurchaseOrderSubmissionId::new(next_id()),
            next_no,
            draft,
            Instant::now(),
            actor.id(),
        )?;
        let formal_id = PurchaseOrderSubmissionId::new(formal.base.id.clone());
        for line in draft_lines.iter_mut() {
            *line = PurchaseOrderSubmissionLine::freeze_from_draft(
                PurchaseOrderSubmissionLineId::new(next_id()),
                formal_id.clone(),
                line,
            )?;
        }
        let _ = order;
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
        let inputs = to_line_inputs(requested_lines)?;
        let (gross, net, tax) = compute_request_totals(&inputs)?;
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
        let lines = build_submission_lines(&formal.base.id.clone().into(), &inputs)?;
        Ok((formal, lines))
    }

    /// 计算下一个提交序号（`SUB-{n}`，聚合内唯一）。
    async fn next_submission_no(&self, order: &PurchaseOrder) -> Result<String> {
        let existing = self
            .db
            .purchase_order()
            .list_submissions_by_order(&order.base.id.clone().into(), &mut NoTransaction)
            .await?;
        PurchaseOrderSubmission::next_submission_no(&existing).map_err(Into::into)
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
pub(super) struct PurchaseSubmitReceipt {
    /// 采购单号。
    pub(super) purchase_no: String,
    /// 形成的不可变提交。
    pub(super) submission_id: String,
    /// 提交序号。
    pub(super) submission_no: String,
    /// 审核待办；事务内首个入口任务写入后回填，无任务时保持空。
    pub(super) work_item_id: String,
    /// 审核待办乐观锁版本。
    pub(super) task_version: u64,
    /// 待办锁定的不可变采购提交版本。
    pub(super) subject_version: String,
    /// 采购单新乐观锁版本。
    pub(super) lock_version: u64,
}

impl PurchaseSubmitReceipt {
    /// 回填首个入口任务身份，保证回放与首次响应一致。
    ///
    /// # 参数
    /// * `first_task` - 事务内写入的首个入口任务身份；无任务时为空
    ///
    /// # 返回
    /// 返回携带真实任务身份（或无任务时保持空占位）的收据。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只能在收据编码前调用一次；同一命令首次响应与回放必须返回相同的任务身份。
    pub(super) fn with_first_task(mut self, first_task: Option<&(String, u64)>) -> Self {
        if let Some((work_item_id, task_version)) = first_task {
            self.work_item_id = work_item_id.clone();
            self.task_version = *task_version;
        }
        self
    }
}

impl PurchaseReceiptWire for PurchaseSubmitReceipt {
    /// 把提交结果编码为历史管道分隔 wire 文本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `purchase_no|submission_id|submission_no|work_item_id|task_version|subject_version|lock_version`。
    ///
    /// # 错误
    /// 无；全字段均为可显示文本。
    ///
    /// # 关键业务约束
    /// 字段顺序与存量收据一致，变更会破坏幂等回放。
    fn encode_wire(&self) -> entities::Result<String> {
        Ok(format!(
            "{}|{}|{}|{}|{}|{}|{}",
            self.purchase_no,
            self.submission_id,
            self.submission_no,
            self.work_item_id,
            self.task_version,
            self.subject_version,
            self.lock_version,
        ))
    }

    /// 从历史管道分隔 wire 文本解码提交结果。
    ///
    /// # 参数
    /// * `wire` - 已通过指纹校验的结果文本
    ///
    /// # 返回
    /// 七个字段齐全且版本字段为整数时返回结果；否则返回 `None`。
    ///
    /// # 关键业务约束
    /// 字段缺失或版本非法必须返回 `None`，由收据解码统一映射为内部错误。
    fn decode_wire(wire: &str) -> Option<Self> {
        let fields = wire.split('|').collect::<Vec<_>>();
        let [purchase_no, submission_id, submission_no, work_item_id, task_version, subject_version, lock_version] =
            fields.as_slice()
        else {
            return None;
        };
        Some(Self {
            purchase_no: (*purchase_no).to_string(),
            submission_id: (*submission_id).to_string(),
            submission_no: (*submission_no).to_string(),
            work_item_id: (*work_item_id).to_string(),
            task_version: task_version.parse().ok()?,
            subject_version: (*subject_version).to_string(),
            lock_version: lock_version.parse().ok()?,
        })
    }
}

#[cfg(test)]
mod tests {
    use entities::purchase_order::{
        LegacyReceiptIdScheme, PurchaseCommandReceipt, PurchaseCommandReceiptError,
    };
    use entities::{AccountKind, AuditLog, AuditLogData};
    use sha2::{Digest, Sha256};

    use super::{PurchaseSubmitReceipt, PURCHASE_SUBMIT_RECEIPT_PREFIX};
    use crate::purchase_order::dto::PURCHASE_SUBMIT_ACTION;

    /// 构造最小有效采购提交审计日志。
    ///
    /// # 参数
    /// * `message` - 已编码命令收据消息
    ///
    /// # 返回
    /// 返回用于纯函数校验的审计实体。
    ///
    /// # 错误
    /// 测试数据固定有效，不返回错误。
    fn submit_audit_fixture(message: String) -> AuditLog {
        AuditLog::new(
            "receipt-1".to_string(),
            AuditLogData {
                actor_id: "actor-1".to_string(),
                actor_account: "buyer".to_string(),
                actor_type: AccountKind::Admin,
                action: PURCHASE_SUBMIT_ACTION.to_string(),
                resource_type: "purchase_order".to_string(),
                resource_id: Some("po-1".to_string()),
                success: true,
                message: Some(message),
            },
        )
        .expect("audit fixture 必须合法")
    }

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
        let message = PurchaseCommandReceipt::new(fingerprint.clone(), receipt.clone())
            .encode_message()
            .unwrap();
        let audit = submit_audit_fixture(message.clone());

        assert_eq!(
            PurchaseCommandReceipt::<PurchaseSubmitReceipt>::decode(
                &audit,
                "actor-1",
                PURCHASE_SUBMIT_ACTION,
                Some("po-1"),
                &fingerprint,
            )
            .unwrap()
            .into_payload(),
            receipt
        );
        let identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            PURCHASE_SUBMIT_RECEIPT_PREFIX,
            "actor-1",
            PURCHASE_SUBMIT_ACTION,
            Some("po-1"),
            "raw-secret-key",
            LegacyReceiptIdScheme::WholeStringJoined,
        )
        .unwrap();
        assert!(!identity.receipt_id().contains("raw-secret-key"));
        assert!(message.len() <= 256);
        assert!(matches!(
            PurchaseCommandReceipt::<PurchaseSubmitReceipt>::decode(
                &audit,
                "actor-1",
                PURCHASE_SUBMIT_ACTION,
                Some("po-1"),
                &"a".repeat(64),
            ),
            Err(PurchaseCommandReceiptError::PayloadConflict)
        ));
    }

    /// 验证存量整串摘要收据 ID 保留为回读候选。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 历史 ID 不在候选或新写入 ID 非规范摘要时测试失败。
    #[test]
    fn legacy_submit_identity_remains_lookup_candidate() {
        let identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            PURCHASE_SUBMIT_RECEIPT_PREFIX,
            "actor-1",
            PURCHASE_SUBMIT_ACTION,
            Some("po-1"),
            "legacy-key",
            LegacyReceiptIdScheme::WholeStringJoined,
        )
        .unwrap();
        let legacy = format!(
            "{PURCHASE_SUBMIT_RECEIPT_PREFIX}{}",
            hex::encode(Sha256::digest(b"actor-1|purchase_order.submit|po-1|legacy-key"))
        );
        assert!(identity.id_candidates().contains(&legacy.as_str()));
    }

    /// 验证收据任务身份在事务内回填，无任务时保持空占位。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 无。
    ///
    /// # 错误
    /// 首次响应与回放的任务身份不一致时测试失败。
    #[test]
    fn submit_receipt_backfills_first_task_identity() {
        let receipt = PurchaseSubmitReceipt {
            purchase_no: "PO-1".to_string(),
            submission_id: "submission-1".to_string(),
            submission_no: "SUB-000001".to_string(),
            work_item_id: String::new(),
            task_version: 0,
            subject_version: "1".to_string(),
            lock_version: 2,
        };
        let filled = receipt.clone().with_first_task(Some(&("wi-9".to_string(), 7)));
        assert_eq!(filled.work_item_id, "wi-9");
        assert_eq!(filled.task_version, 7);
        let placeholder = receipt.with_first_task(None);
        assert_eq!(placeholder.work_item_id, "");
        assert_eq!(placeholder.task_version, 0);
    }
}
