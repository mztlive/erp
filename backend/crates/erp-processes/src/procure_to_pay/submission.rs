//! 采购草稿冻结并调用统一 `start_approval`。

mod preparation;

use application_core::AuditActor;
use erp_procurement::dto::purchase_order::{
    PURCHASE_SUBMIT_ACTION, SubmitPurchaseOrderRequest, SubmitPurchaseOrderResult,
};
use erp_procurement::entity::purchase_order::{
    PurchaseCommandReceipt, PurchaseCommandReceiptError, PurchaseCommandReceiptIdentity,
    PurchaseSubmitReceipt,
};
use erp_procurement::repository::{PurchaseCommandExt, PurchaseOrderExt};
use erp_sales::repository::SalesOrderExt;
use erp_workflow::service::approval::execution::command_recovery_delay;
use erp_workflow::service::document_registry::find_approval_binding;
use persistence_core::{NoTransaction, Transactional};
use preparation::PreparedPurchaseSubmit;
use validator::Validate;

use super::PurchaseOrderProcess;
use super::adapter::{
    purchase_order_object_readable, purchase_order_responsible_org_id, purchase_order_subject_ref,
    require_frozen_binding,
};
use super::start_approval::{persist_purchase_order_start, replay_purchase_order_start_with_executor};
use crate::audit::recover_command;
use crate::{Error, Result};

const PURCHASE_SUBMIT_RECEIPT_PREFIX: &str = "purchase-submit-command-";

/// 同一次提交的请求、调用人和领域幂等身份；准备、提交与恢复复用同一载荷。
struct PurchaseSubmitCommand<'a> {
    purchase_order_id: &'a str,
    request: &'a SubmitPurchaseOrderRequest,
    actor: &'a AuditActor,
    identity: &'a PurchaseCommandReceiptIdentity,
    fingerprint: &'a str,
}

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
        let fingerprint = req.request_fingerprint(id);
        let identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            PURCHASE_SUBMIT_RECEIPT_PREFIX,
            actor.id(),
            PURCHASE_SUBMIT_ACTION,
            Some(id),
            &req.idempotency_key,
        )?;
        if let Some(result) = self.replay_purchase_submit(&identity, &fingerprint, id, actor).await? {
            return Ok(result);
        }
        let command = PurchaseSubmitCommand {
            purchase_order_id: id,
            request: &req,
            actor,
            identity: &identity,
            fingerprint: &fingerprint,
        };
        let prepared = self.prepare_purchase_submit(&command).await?;
        self.persist_prepared_submit(&command, prepared).await
    }

    /// 提交既有采购启动写段；仅在结果可能已提交时进入有界回读恢复。
    async fn persist_prepared_submit(
        &self,
        command: &PurchaseSubmitCommand<'_>,
        prepared: PreparedPurchaseSubmit,
    ) -> Result<SubmitPurchaseOrderResult> {
        let PreparedPurchaseSubmit { input, output, purchase_order_id, subject_version } = prepared;
        let db = self.db.clone();
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
                        purchase_order_id: command.purchase_order_id,
                        subject_version,
                        idempotency_key: &command.request.idempotency_key,
                        actor: command.actor,
                        identity: command.identity,
                        fingerprint: command.fingerprint,
                        original_error: error,
                    })
                    .await;
            },
            Err(error) => return Err(error),
        };
        let (work_item_id, task_version) = first_task.unwrap_or((String::new(), 0));
        Ok(SubmitPurchaseOrderResult {
            purchase_order_id,
            purchase_no: output.purchase_no.clone(),
            submission_id: output.submission_id,
            submission_no: output.submission_no,
            work_item_id,
            task_version,
            subject_version: output.subject_version,
            lock_version: output.lock_version,
            reference: output.purchase_no,
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
    use super::{
        PURCHASE_SUBMIT_ACTION, PURCHASE_SUBMIT_RECEIPT_PREFIX, PurchaseCommandReceipt,
        PurchaseCommandReceiptIdentity, PurchaseSubmitReceipt, decode_submit_receipt,
    };
    use crate::Error;

    fn receipt_fixture()
    -> (PurchaseCommandReceiptIdentity, PurchaseCommandReceipt<PurchaseSubmitReceipt>, String) {
        let identity = PurchaseCommandReceipt::<PurchaseSubmitReceipt>::identity(
            PURCHASE_SUBMIT_RECEIPT_PREFIX,
            "actor-1",
            PURCHASE_SUBMIT_ACTION,
            Some("po-1"),
            "submit-key-1",
        )
        .unwrap();
        let result = PurchaseSubmitReceipt::new(
            "PO-1".into(),
            "submission-1".into(),
            "SUB-1".into(),
            "wi-9".into(),
            "1".into(),
        )
        .with_versions(7, 2);
        let fingerprint = "a".repeat(64);
        let record = PurchaseCommandReceipt::new(&identity, &fingerprint, result, "audit-1".into()).unwrap();
        (identity, record, fingerprint)
    }

    #[test]
    fn submit_receipt_decode_preserves_frozen_result() {
        let (identity, record, fingerprint) = receipt_fixture();
        let expected = record.payload().clone();
        let result = decode_submit_receipt(record, &identity, &fingerprint).unwrap();
        assert_eq!(result, expected);
        assert_eq!(result.work_item_id, "wi-9");
        assert_eq!(result.task_version, 7);
        assert_eq!(result.lock_version, 2);
    }

    #[test]
    fn submit_receipt_decode_keeps_identity_and_payload_error_categories() {
        let (identity, record, fingerprint) = receipt_fixture();
        let mut other_identity = identity.clone();
        other_identity.actor_id = "actor-2".into();
        let identity_error =
            decode_submit_receipt(record.clone(), &other_identity, &fingerprint).unwrap_err();
        assert!(
            matches!(identity_error, Error::Internal(message) if message == "采购提交幂等收据与业务对象不一致")
        );
        let payload_error = decode_submit_receipt(record, &identity, &"b".repeat(64)).unwrap_err();
        assert!(
            matches!(payload_error, Error::ConflictError(message) if message == "幂等键已用于不同的采购提交命令")
        );
    }

    #[test]
    fn submit_receipt_decode_rejects_corrupted_frozen_result() {
        let (identity, mut record, fingerprint) = receipt_fixture();
        record.result.task_version = 0;
        let error = decode_submit_receipt(record, &identity, &fingerprint).unwrap_err();
        assert!(matches!(error, Error::Internal(message) if message == "采购命令回执损坏"));
    }

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
