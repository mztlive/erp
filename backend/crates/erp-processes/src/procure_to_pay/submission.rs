//! 采购草稿冻结并调用统一 `start_approval`。

use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_core::common::time::Instant;
use erp_procurement::dto::purchase_order::{
    PURCHASE_SUBMIT_ACTION, SavePurchaseOrderLinePatch, SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult,
};
use erp_procurement::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseCommandReceiptIdentity,
    PurchaseSubmitReceipt,
};
use erp_procurement::repository::{PurchaseCommandExt, PurchaseOrderExt};
use erp_procurement::service::purchase_order::draft_edit::map_draft_edit_violation;
use erp_procurement::service::purchase_order::submission::assign_formal_purchase_no;
use erp_sales::repository::SalesOrderExt;
use erp_workflow::service::approval::execution::{command_recovery_delay, prepare_start};
use erp_workflow::service::approval::policy::ApprovalDomainAction;
use erp_workflow::service::document_registry::{find_approval_binding, find_registered_document};
use id_generator::next_id;
use persistence_core::{NoTransaction, Transactional};
use validator::Validate;

use super::PurchaseOrderProcess;
use super::adapter::{
    RECENT_HISTORY_LIMIT, build_purchase_order_snapshot, execute_purchase_order_domain_action,
    purchase_order_adapter, purchase_order_object_readable, purchase_order_responsible_org_id,
    purchase_order_start_command, purchase_order_subject_ref, require_frozen_binding,
    start_approval_command_kind,
};
use super::start_approval::{
    PurchaseOrderStartInput, PurchaseOrderStartPersistInput, PurchaseSubmitProcurementGuard,
    build_purchase_order_start_input, load_bound_definition_graph, load_start_receipt,
    persist_purchase_order_start, replay_purchase_order_start_with_executor,
};
use crate::audit::recover_command;
use crate::{Error, Result};

const PURCHASE_SUBMIT_RECEIPT_PREFIX: &str = "purchase-submit-command-";

/// 采购提交启动恢复入参。
struct RecoverPurchaseSubmitStartInput<'a> {
    /// 采购单主键。
    purchase_order_id: &'a str,
    /// 提交时冻结的审批主题版本。
    subject_version: u32,
    /// 提交幂等键。
    idempotency_key: &'a str,
    /// 提交人。
    actor: &'a AuditActor,
    /// 命令收据身份。
    identity: &'a PurchaseCommandReceiptIdentity,
    /// 命令载荷指纹。
    fingerprint: &'a str,
    /// 触发恢复的原始错误。
    original_error: Error,
}

impl PurchaseOrderProcess {
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
    /// * `NotFound` - 采购单不存在或无权操作
    /// * `ConflictError` - 期望版本不一致、无绑定或重复提交
    /// * `BusinessLogicError` - 状态非草稿或草稿内容缺失
    ///
    /// # 关键业务约束
    /// 提交必须按当前采购对象范围重验，历史参与不授予提交。
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
        )?;
        if let Some(result) = self.replay_purchase_submit(&receipt_identity, &fingerprint, id, actor).await? {
            return Ok(result);
        }
        let adapter = purchase_order_adapter()?;
        let subject = purchase_order_subject_ref(id)?;
        let mut order = self.command_access(actor, "submit")?.current(id, &mut NoTransaction).await?;
        order
            .ensure_expected_version(req.expected_lock_version)
            .map_err(|_| Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()))?;
        order
            .ensure_draft_for_submission()
            .map_err(|_| Error::ConflictError("采购单已提交或已生效，请勿重复提交".to_string()))?;
        let binding =
            find_approval_binding(&self.db, id, &mut NoTransaction).await.map_err(crate::Error::from)?;
        let binding = require_frozen_binding(binding.as_ref())?.clone();
        let draft_id =
            order.draft_submission_id().map_err(|error| Error::BusinessLogicError(error.to_string()))?;
        let mut draft = self
            .db
            .purchase_order_submissions()
            .find_by_id(&draft_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("草稿提交不存在".to_string()))?;
        draft.ensure_draft().map_err(|_| Error::ConflictError("草稿提交已冻结".to_string()))?;
        let mut draft_lines =
            self.db.purchase_order().list_submission_lines(&draft_id, &mut NoTransaction).await?;
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
            let (submission, lines) =
                self.domain().freeze_submission_from_lines(&order, &draft, &requested_lines, actor).await?;
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
            self.domain().freeze_submission(&mut order, &mut draft, &mut draft_lines, actor).await?
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
        let existing_receipt =
            load_start_receipt(&self.db, &subject, order.approval_subject_version, &req.idempotency_key)
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
        let audit = actor
            .clone()
            .resource_log_with_id(next_id(), action, "purchase_order", order.base.id.clone(), None)?
            .with_command_id(Some(receipt_identity.receipt_id().to_string()))?
            .with_resource_number(Some(order.purchase_no.clone()))?;
        let db = self.db.clone();
        let input = PurchaseOrderStartPersistInput {
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
            object_scope: Some(self.command_access(actor, "submit")?),
            receipt: Some((
                receipt_identity.clone(),
                fingerprint.clone(),
                PurchaseSubmitReceipt::new(
                    order.purchase_no.clone(),
                    submission.base.id.clone(),
                    submission.submission_no.clone(),
                    String::new(),
                    order.approval_subject_version.to_string(),
                )
                .with_versions(0, order.base.version),
            )),
        };
        let first_task = self
            .db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { persist_purchase_order_start(&db, input, executor).await })
            })
            .await;
        let first_task = match first_task {
            Ok(task) => task,
            Err(error) if error.command_may_have_committed() => {
                return self
                    .recover_purchase_submit_start(RecoverPurchaseSubmitStartInput {
                        purchase_order_id: id,
                        subject_version: order.approval_subject_version,
                        idempotency_key: &req.idempotency_key,
                        actor,
                        identity: &receipt_identity,
                        fingerprint: &fingerprint,
                        original_error: error,
                    })
                    .await;
            },
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
        _actor: &AuditActor,
    ) -> Result<Option<SubmitPurchaseOrderResult>> {
        let Some(record) = self
            .db
            .purchase_command_receipts::<PurchaseSubmitReceipt>()
            .find_by_id_including_deleted(identity.receipt_id(), &mut NoTransaction)
            .await?
        else {
            return Ok(None);
        };
        let receipt = decode_submit_receipt(record, identity, expected_fingerprint)?;
        let order = self
            .db
            .purchase_orders()
            .find_by_id(purchase_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("采购提交幂等收据引用的采购单不存在".to_string()))?;
        let submission = self
            .db
            .purchase_order_submissions()
            .find_by_id(&receipt.submission_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::Internal("采购提交幂等回执引用的不可变提交不存在".to_string()))?;
        if submission.purchase_order_id.as_ref() != purchase_order_id
            || submission.submission_no != receipt.submission_no
            || order.purchase_no != receipt.purchase_no
            || order.base.version < receipt.lock_version
        {
            return Err(Error::Internal("采购提交幂等回执与领域结果事实不一致".to_string()));
        }
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
    ///
    /// # 参数
    /// * `input` - 采购提交恢复所需的主题版本、幂等键与收据身份
    ///
    /// # 返回
    /// 回读到已提交结果时返回提交视图；否则返回原始错误。
    ///
    /// # 错误
    /// 有界回读仍无法确认提交结果时返回原始错误，或传播不可恢复的冲突。
    ///
    /// # 关键业务约束
    /// 仅在 `command_may_have_committed` 场景进入；不得在确认未提交时吞掉错误。
    async fn recover_purchase_submit_start(
        &self,
        input: RecoverPurchaseSubmitStartInput<'_>,
    ) -> Result<SubmitPurchaseOrderResult> {
        const RECOVERY_ATTEMPTS: usize = 8;
        for attempt in 0..RECOVERY_ATTEMPTS {
            let recovered = self.check_purchase_start(&input).await;
            match recovered {
                Ok(Some(_)) => {
                    match self
                        .replay_purchase_submit(
                            input.identity,
                            input.fingerprint,
                            input.purchase_order_id,
                            input.actor,
                        )
                        .await
                    {
                        Ok(None) => {},
                        recovered => return recover_command(input.original_error, recovered),
                    }
                },
                Ok(None) => {},
                Err(error) if error.command_may_have_committed() => {},
                Err(error) => return recover_command(input.original_error, Err(error)),
            }
            if attempt + 1 < RECOVERY_ATTEMPTS {
                tokio::time::sleep(command_recovery_delay(attempt)).await;
            }
        }
        recover_command(input.original_error, Ok(None))
    }

    /// 在原有独立只读事务中核对启动事实；不重新执行采购提交。
    async fn check_purchase_start(
        &self,
        input: &RecoverPurchaseSubmitStartInput<'_>,
    ) -> Result<Option<String>> {
        let db = self.db.clone();
        let purchase_order_id = input.purchase_order_id.to_string();
        let idempotency_key = input.idempotency_key.to_string();
        let actor_id = input.actor.id().to_string();
        let subject_version = input.subject_version;
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = db
                        .purchase_orders()
                        .find_by_id(&purchase_order_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("采购单不存在".to_string()))?;
                    let sales_order = db
                        .sales_orders()
                        .find_by_id(&order.sales_order_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
                    let organization_id = purchase_order_responsible_org_id(&sales_order)?;
                    let _ = purchase_order_object_readable(&organization_id, &actor_id)?;
                    let binding = find_approval_binding(&db, &purchase_order_id, executor)
                        .await
                        .map_err(Error::from)?;
                    let binding = require_frozen_binding(binding.as_ref())?;
                    let subject = purchase_order_subject_ref(&purchase_order_id)?;
                    replay_purchase_order_start_with_executor(
                        &db,
                        &subject,
                        subject_version,
                        &idempotency_key,
                        binding,
                        &actor_id,
                        executor,
                    )
                    .await
                })
            })
            .await
    }
}

/// 保留采购提交领域回执的身份损坏、载荷冲突与损坏结果错误分类。
fn decode_submit_receipt(
    record: PurchaseCommandReceipt<PurchaseSubmitReceipt>,
    identity: &PurchaseCommandReceiptIdentity,
    expected_fingerprint: &str,
) -> Result<PurchaseSubmitReceipt> {
    PurchaseCommandReceipt::<PurchaseSubmitReceipt>::decode(record, identity, expected_fingerprint)
        .map(PurchaseCommandReceipt::into_payload)
        .map_err(|error| match error {
            PurchaseCommandReceiptError::IdentityMismatch => {
                Error::Internal("采购提交幂等收据与业务对象不一致".to_string())
            },
            PurchaseCommandReceiptError::PayloadConflict => {
                Error::ConflictError("幂等键已用于不同的采购提交命令".to_string())
            },
            PurchaseCommandReceiptError::Corrupted(message) => Error::Internal(message),
        })
}

#[cfg(test)]
mod tests {
    use erp_procurement::entity::purchase_order::PurchaseSubmitReceipt;

    #[test]
    fn submit_receipt_freezes_first_task_identity() {
        let receipt = PurchaseSubmitReceipt::new(
            "PO-1".into(),
            "submission-1".into(),
            "SUB-1".into(),
            String::new(),
            "1".into(),
        )
        .with_versions(0, 2);
        let filled = receipt.clone().with_first_task(Some(&("wi-9".into(), 7)));
        assert_eq!(filled.work_item_id, "wi-9");
        assert_eq!(filled.task_version, 7);
        assert_eq!(receipt.with_first_task(None).task_version, 0);
    }
}
