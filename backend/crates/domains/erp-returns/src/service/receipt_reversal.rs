//! 回款冲正的本域构造、版本守卫、累计额度和事务内持久化。

use erp_core::common::time::Instant;
use erp_core::ids::{CustomerReceiptId, ReceiptReversalId};
use erp_core::money::Amount;
use id_generator::next_id;
use mongodb::Database;
use persistence_core::Executor;

use super::ReturnsService;
use super::approval::{ensure_receipt_reversal_final_approve_posting, start_receipt_reversal_approval};
use super::shared::{
    DEFAULT_FINANCE_REVIEWER, RECEIPT_REVERSAL_COMMAND_PREFIX, ensure_cumulative_within, or_not_found,
    reject_if_reversed, return_command_no,
};
use super::version_conflict::conflict_if_stale_version;
use crate::dto::{CommitReceiptReversalRequest, CreateReceiptReversalRequest};
use crate::entity::returns::{ReceiptReversal, ReceiptReversalData, ReceiptReversalStatus};
use crate::repository::ReturnsExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 在请求校验后按原时点分配 ID，并由实体规范化创建数据。
/// 返回初始草稿回款冲正单。
/// 编号、原因、经办复核人、创建人为空或超长、金额非正或经办与复核人相同时返回 `Logic`。
pub fn build_create(req: CreateReceiptReversalRequest, actor_id: &str) -> Result<ReceiptReversal> {
    Ok(ReceiptReversal::new(
        ReceiptReversalId::new(next_id()),
        ReceiptReversalData {
            reversal_no: req.reversal_no,
            original_customer_receipt_id: req.original_customer_receipt_id,
            reason_code: req.reason_code,
            reason_text: req.reason_text,
            amount: req.amount,
            handled_by: req.handled_by,
            reviewed_by: req.reviewed_by,
            occurred_at: req.occurred_at,
            evidence_attachment_id: None,
        },
        actor_id,
    )?)
}

/// 按原回款窄事实构造一次提交草稿；先分配 ID，再读取发生时间。
/// 原回款的读取与版本/过账校验仍由根流程在原时点组织。
/// 返回指向该原回款的草稿冲正单。
/// `req` 为一次提交命令；`source_fact_id` 为原回款身份。
/// `receipt_amount` 为省略金额时沿用的原回款金额；`actor_id` 为经办人，同时作为创建人。
/// 原因、经办复核人或金额不满足实体不变量时返回 `Logic`。
pub fn build_commit(
    req: &CommitReceiptReversalRequest,
    source_fact_id: CustomerReceiptId,
    receipt_amount: Amount,
    actor_id: &str,
) -> Result<ReceiptReversal> {
    Ok(ReceiptReversal::new(
        ReceiptReversalId::new(next_id()),
        ReceiptReversalData {
            reversal_no: return_command_no(RECEIPT_REVERSAL_COMMAND_PREFIX, actor_id, &req.idempotency_key),
            original_customer_receipt_id: source_fact_id,
            reason_code: None,
            reason_text: req.reason.clone(),
            amount: req.amount.unwrap_or(receipt_amount),
            handled_by: actor_id.to_string(),
            reviewed_by: DEFAULT_FINANCE_REVIEWER.to_string(),
            occurred_at: Instant::now(),
            evidence_attachment_id: None,
        },
        actor_id,
    )?)
}

/// 回款冲正提交前核对当前实体和客户端所见版本；不匹配返回 `ConflictError` 并保留原并发冲突文案。
pub fn ensure_receipt_reversal_version(reversal: &ReceiptReversal, expected_version: u64) -> Result<()> {
    conflict_if_stale_version(reversal.matches_version(expected_version))
}

/// 提交回款冲正时先比对期望版本，再从草稿进入审批并递增审批主题版本。
/// 版本不匹配返回 `ConflictError` 且不改实体；非草稿或审批版本溢出返回 `Logic`。
pub fn prepare_receipt_reversal_submit(reversal: &mut ReceiptReversal, expected_version: u64) -> Result<()> {
    ensure_receipt_reversal_version(reversal, expected_version)?;
    start_receipt_reversal_approval(reversal)?;
    Ok(())
}

impl ReturnsService {
    /// 客户端请求直接过账回款冲正时恒返回 `ConflictError`；最终过账由审批运行时触发。
    pub fn reject_receipt_reversal_client_post() -> Result<std::convert::Infallible> {
        Err(Error::ConflictError("回款冲正过账只能由审批最终通过动作执行，客户端不得直接过账".to_string()))
    }

    /// 按主键在调用方原执行器中读取回款冲正单。
    /// 返回读到的冲正单。
    /// `executor` 为原用例授权事务或读取阶段的执行器；`id` 为冲正单主键。
    /// 不存在时返回 `NotFound`；仓储读取失败时返回对应错误。
    pub async fn load_receipt_reversal(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversal> {
        or_not_found(self.db.receipt_reversals().find_by_id(id, executor).await?, "回款冲正单不存在")
    }

    /// 审批最终通过、尚未写反向财务事实时，沿调用方事务加载回款冲正单，先拒绝已冲正，再要求仍在审批中。
    /// 缺单返回 `NotFound`，已冲正返回 `BusinessLogicError`，非审批中返回 `ConflictError`；传播读取错误。
    pub async fn prepare_receipt_reversal_post(
        db: &Database,
        reversal_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<ReceiptReversal> {
        let reversal = or_not_found(
            db.receipt_reversals().find_by_id(reversal_id, executor).await?,
            "回款冲正单不存在",
        )?;
        reject_if_reversed(reversal.status == ReceiptReversalStatus::Reversed, "已冲正单据不能再过账")?;
        ensure_receipt_reversal_final_approve_posting(&reversal)?;
        Ok(reversal)
    }

    /// 在确认原回款已过账后，沿原执行器累计同一原单已过账的回款冲正金额（排除本单），加上本次与原金额比较。
    /// 超限返回 `BusinessLogicError`；传播聚合错误。
    pub async fn validate_receipt_reversal_amount(
        db: &Database,
        reversal: &ReceiptReversal,
        receipt_amount: Amount,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let reversed_before = db
            .receipt_reversals()
            .posted_reversal_total_by_receipt(
                &reversal.original_customer_receipt_id,
                &reversal.base.id,
                executor,
            )
            .await?;
        ensure_cumulative_within(
            receipt_amount,
            reversed_before,
            reversal.amount,
            "累计冲正金额不得超过原回款金额",
        )
    }

    /// 正式财务写入成功后，在原事务把审批中的回款冲正单标记过账并以原版本 CAS 写回；审计与销售进度刷新仍由根流程完成。
    /// 非审批中返回 `Logic`；传播仓储、版本竞争及其他写入错误。
    pub async fn persist_posted_receipt_reversal(
        db: &Database,
        reversal: &mut ReceiptReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        reversal.mark_posted()?;
        Self::persist_receipt_reversal(db, reversal, executor).await
    }

    /// 在注册/审批编排的原写入时点创建本域冲正单，复用根事务。
    /// `reversal` 为待创建的冲正单；沿用调用方执行器 `executor`。
    /// 传播仓储写入错误。
    pub async fn persist_created_receipt_reversal(
        db: &Database,
        reversal: &ReceiptReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.receipt_reversals().create(reversal, executor).await?;
        Ok(())
    }

    /// 使用已有仓储版本条件写回冲正单，不开启事务或生成额外时间/ID。
    /// `reversal` 为待写回的冲正单；沿用调用方执行器 `executor`。
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_receipt_reversal(
        db: &Database,
        reversal: &mut ReceiptReversal,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        db.receipt_reversals().update(reversal, executor).await?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn commit_request() -> CommitReceiptReversalRequest {
        CommitReceiptReversalRequest {
            source_fact_id: "original-receipt".to_string(),
            amount: None,
            reason: " 错记回款冲正 ".to_string(),
            idempotency_key: " correction-key ".to_string(),
        }
    }

    #[test]
    fn commit_uses_source_amount_or_explicit_partial_amount_without_customer_requirement() {
        let source_amount = Amount::from_str("100").unwrap();
        let req = commit_request();
        let whole =
            build_commit(&req, CustomerReceiptId::new("original-receipt"), source_amount, "actor").unwrap();
        let partial = build_commit(
            &CommitReceiptReversalRequest { amount: Some(Amount::from_str("25").unwrap()), ..req },
            CustomerReceiptId::new("original-receipt"),
            source_amount,
            "actor",
        )
        .unwrap();

        assert_eq!(whole.amount, source_amount);
        assert_eq!(partial.amount, Amount::from_str("25").unwrap());
        assert_eq!(whole.reversal_no, partial.reversal_no);
        assert_eq!(whole.reason_text, "错记回款冲正");
        assert_eq!(whole.handled_by, "actor");
        assert_eq!(whole.reviewed_by, "finance_reviewer");
        assert_eq!(whole.original_customer_receipt_id, CustomerReceiptId::new("original-receipt"));
        assert_eq!(whole.status, ReceiptReversalStatus::Draft);
        assert_eq!(whole.approval_subject_version, 0);
    }

    #[test]
    fn submit_stale_version_precedes_invalid_status_and_leaves_entity_unchanged() {
        let mut reversal = build_commit(
            &commit_request(),
            CustomerReceiptId::new("receipt"),
            Amount::from_str("100").unwrap(),
            "actor",
        )
        .unwrap();
        start_receipt_reversal_approval(&mut reversal).unwrap();
        reversal.mark_posted().unwrap();
        let before = reversal.clone();

        let error = prepare_receipt_reversal_submit(&mut reversal, 0).unwrap_err();
        assert!(
            matches!(error, Error::ConflictError(message) if message == "数据已被其他请求修改，请刷新后重试")
        );
        assert_eq!(reversal, before);

        let version = reversal.base.version;
        let error = prepare_receipt_reversal_submit(&mut reversal, version).unwrap_err();
        assert!(matches!(error, Error::Logic(_)));
        assert_eq!(reversal, before);
    }

    #[test]
    fn submit_freezes_subject_version_and_client_post_keeps_original_conflict() {
        let mut reversal = build_commit(
            &commit_request(),
            CustomerReceiptId::new("receipt"),
            Amount::from_str("100").unwrap(),
            "actor",
        )
        .unwrap();
        let version = reversal.base.version;
        prepare_receipt_reversal_submit(&mut reversal, version).unwrap();
        assert_eq!(reversal.status, ReceiptReversalStatus::InApproval);
        assert_eq!(reversal.approval_subject_version, 1);
        assert_eq!(reversal.base.version, version);

        assert!(
            matches!(ReturnsService::reject_receipt_reversal_client_post(), Err(Error::ConflictError(message)) if message == "回款冲正过账只能由审批最终通过动作执行，客户端不得直接过账")
        );
    }
}
