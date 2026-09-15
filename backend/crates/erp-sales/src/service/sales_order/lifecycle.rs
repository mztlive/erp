//! Sales approval state rules independent of workflow action enums.
use crate::entity::sales_order::{CommercialStatus, ReviewStatus, SalesOrder};
use crate::{Error, Result};
/// 提交并启动：进入 `PENDING_REVIEW` / `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
/// 更新人必须是调用方提交销售，不得回落到上次更新人。
///
/// # 参数
/// * `order` - 待提交销售单
/// * `submitted_by` - 本次提交销售
///
/// # 错误
/// 状态不允许时返回冲突。
pub fn start_sales_order_approval(order: &mut SalesOrder, submitted_by: &str) -> Result<()> {
    Ok(order.start_approval_submission(submitted_by)?)
}

/// 撤回审批：回到 `DRAFT` / `NOT_SUBMITTED`，且提交号不回退。
///
/// # 参数
/// * `order` - 审批中的销售单
/// * `updated_by` - 操作人
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_sales_order_to_draft(order: &mut SalesOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval_submission(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_formalize(order: &SalesOrder) -> Result<()> {
    if order.commercial_status != CommercialStatus::PendingReview
        || order.review_status != ReviewStatus::InApproval
    {
        return Err(Error::ConflictError("只有审批中的销售单可以由最终通过动作形式化".to_string()));
    }
    Ok(())
}

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};
use persistence_core::{Executor, NoTransaction};

use super::SalesOrderService;
use super::mapper::{build_working_copy_lines, header_snapshot};
use crate::dto::sales_order::{CreateSalesOrderRequest, SalesOrderDraftRequest};
use crate::entity::sales_order::{
    SalesContentHash, SalesOrderData, SalesOrderLine, SalesOrderSubmission, SalesOrderSubmissionLine,
    SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyUpdate,
};
use crate::repository::SalesOrderExt;

/// Sales-owned write plan for replacing or creating the submission's working copy.
#[derive(Clone)]
pub struct SalesOrderWorkingCopyPersistPlan {
    /// New stable line identities, persisted before working-copy rows.
    pub created_stable_lines: Vec<SalesOrderLine>,
    /// Old editable rows to soft-delete before replacement.
    pub old_working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
    /// New editable rows in their original order.
    pub new_working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
    /// Replace rows on an existing working copy.
    pub replace_working_copy_lines: bool,
    /// Insert a newly reopened copy instead of updating an existing copy.
    pub create_working_copy: bool,
}

impl SalesOrderService {
    /// Construct the initial sales stable object from the verified contract identities.
    ///
    /// Sales constructor errors retain their original class; no current contract data is read here.
    pub fn prepare_order(
        req: &CreateSalesOrderRequest,
        customer_id: CustomerAccountId,
        settlement_party_id: PartyId,
        actor: &AuditActor,
        business_org_unit_id: String,
    ) -> Result<SalesOrder> {
        Ok(SalesOrder::new(
            SalesOrderId::new(id_generator::next_id()),
            SalesOrderData {
                sales_owner_user_id: actor.id().to_string(),
                business_org_unit_id,
                order_no: req.order_no.clone(),
                business_type: req.business_type,
                origin_system: crate::entity::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id,
                contract_id: Some(req.contract_id.clone()),
                settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?)
    }

    /// Create the sales stable object at its original position within the caller's root transaction.
    pub async fn create_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().create(order, executor).await?)
    }

    /// Persist a sales transition with the original optimistic-lock repository operation.
    pub async fn persist_order(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().update(order, executor).await?)
    }

    /// Insert new stable rows, the working copy and its frozen draft rows in the original order.
    ///
    /// The caller owns the transaction; no audit or workflow rows are written by sales.
    pub async fn create_working_copy(
        &self,
        stable: &[SalesOrderLine],
        copy: &SalesOrderWorkingCopy,
        lines: &[SalesOrderWorkingCopyLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for line in stable {
            self.db.sales_order_lines().create(line, executor).await?;
        }
        self.db.sales_order_working_copies().create(copy, executor).await?;
        for line in lines {
            self.db.sales_order_working_copy_lines().create(line, executor).await?;
        }
        Ok(())
    }

    /// Insert the immutable submission and then each immutable submission line.
    pub async fn create_submission(
        &self,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.sales_order_submissions().create(submission, executor).await?;
        for line in lines {
            self.db.sales_order_submission_lines().create(line, executor).await?;
        }
        Ok(())
    }

    /// Apply replacement draft rows before the working-copy optimistic-lock update.
    ///
    /// Stable additions, soft deletions and new rows share the caller's executor and fail immediately.
    pub async fn persist_saved_working_copy(
        &self,
        stable: &[SalesOrderLine],
        old_lines: Vec<SalesOrderWorkingCopyLine>,
        lines: &[SalesOrderWorkingCopyLine],
        copy: &mut SalesOrderWorkingCopy,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for line in stable {
            self.db.sales_order_lines().create(line, executor).await?;
        }
        for mut old in old_lines {
            self.db.sales_order_working_copy_lines().soft_delete(&mut old, executor).await?;
        }
        for line in lines {
            self.db.sales_order_working_copy_lines().create(line, executor).await?;
        }
        self.db.sales_order_working_copies().update(copy, executor).await?;
        Ok(())
    }

    /// Persist the sales portion of an approval start after the outer receipt/guard/catalog checks.
    ///
    /// No workflow or audit writes occur here; all sales writes preserve the original executor/order.
    pub async fn persist_submission_start(
        &self,
        order: &mut SalesOrder,
        copy: &mut SalesOrderWorkingCopy,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
        plan: SalesOrderWorkingCopyPersistPlan,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for line in &plan.created_stable_lines {
            self.db.sales_order_lines().create(line, executor).await?;
        }
        for mut old in plan.old_working_copy_lines {
            self.db.sales_order_working_copy_lines().soft_delete(&mut old, executor).await?;
        }
        if plan.replace_working_copy_lines {
            for line in &plan.new_working_copy_lines {
                self.db.sales_order_working_copy_lines().create(line, executor).await?;
            }
        }
        if plan.create_working_copy {
            self.db.sales_order_working_copies().create(copy, executor).await?;
            for line in &plan.new_working_copy_lines {
                self.db.sales_order_working_copy_lines().create(line, executor).await?;
            }
            self.create_submission(submission, lines, executor).await?;
        } else {
            self.db.sales_order().submit_working_copy(copy, submission, lines, executor).await?;
        }
        self.db.sales_orders().update(order, executor).await?;
        Ok(())
    }

    /// Persist an already validated void transition and optional abandoned first-submission copy.
    pub async fn persist_void(
        &self,
        order: &mut SalesOrder,
        copy: Option<&mut SalesOrderWorkingCopy>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        self.db.sales_orders().update(order, executor).await?;
        if let Some(copy) = copy {
            self.db.sales_order_working_copies().update(copy, executor).await?;
        }
        Ok(())
    }
}

impl SalesOrderService {
    /// Rebuild a saved draft using the original snapshot, amount and content-hash validation order.
    pub fn prepare_saved_working_copy(
        order: &SalesOrder,
        working_copy: &mut SalesOrderWorkingCopy,
        stable: &super::draft_working_copy::DraftStableLines,
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
    ) -> Result<Vec<SalesOrderWorkingCopyLine>> {
        let order_id = SalesOrderId::new(order.base.id.clone());
        let snapshot = header_snapshot(draft)?;
        let lines = build_working_copy_lines(
            &order_id,
            &working_copy.base.id.clone().into(),
            &stable.all,
            &draft.lines,
        )?;
        let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(&lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire()),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                contract_revision_id: draft.requested_contract_revision_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(snapshot),
                project_name: draft.project_name.clone(),
                business_remark: draft.business_remark.clone(),
                voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
                voucher_expiry_at: draft.voucher_expiry_at.map(|secs| Instant::from_unix_secs(secs as i64)),
                receivable_due_date: draft.receivable_due_date,
                gross_amount: Some(gross),
                net_amount: Some(net),
                tax_amount: Some(tax),
            },
            actor.id(),
        )?;
        working_copy.save_draft(
            SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire(),
            draft.editor_user_id.clone(),
        )?;

        Ok(lines)
    }
}

impl SalesOrderService {
    /// Prepare either an existing copy replacement or a reopened first-submission copy.
    ///
    /// Existing versions are checked before old rows are loaded; no rows are persisted here.
    pub async fn prepare_submission_copy(
        &self,
        order: &SalesOrder,
        active_working_copy: Option<SalesOrderWorkingCopy>,
        stable: super::draft_working_copy::DraftStableLines,
        draft: &SalesOrderDraftRequest,
        version: u64,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>, SalesOrderWorkingCopyPersistPlan)>
    {
        let order_id = SalesOrderId::new(order.base.id.clone());
        let (working_copy, copy_lines, working_copy_plan) = match active_working_copy {
            Some(mut working_copy) => {
                if !working_copy.matches_version(version) {
                    return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
                }
                let copy_id = erp_core::ids::SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
                let old_lines = self
                    .db
                    .sales_order_working_copy_lines()
                    .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
                    .await?;
                let copy_lines = build_working_copy_lines(&order_id, &copy_id, &stable.all, &draft.lines)?;
                let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(&copy_lines);
                let next_version = working_copy.draft_version + 1;
                working_copy.update(
                    SalesOrderWorkingCopyUpdate {
                        content_hash: Some(
                            SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire(),
                        ),
                        customer_id: Some(order.customer_id.clone()),
                        contract_id: order.contract_id.clone(),
                        contract_revision_id: draft.requested_contract_revision_id.clone(),
                        settlement_party_id: Some(order.settlement_party_id.clone()),
                        snapshot: Some(header_snapshot(draft)?),
                        project_name: draft.project_name.clone(),
                        business_remark: draft.business_remark.clone(),
                        voucher_category_sku_id: draft.voucher_category_sku_id.clone(),
                        voucher_expiry_at: draft
                            .voucher_expiry_at
                            .map(|secs| Instant::from_unix_secs(secs as i64)),
                        receivable_due_date: draft.receivable_due_date,
                        gross_amount: Some(gross),
                        net_amount: Some(net),
                        tax_amount: Some(tax),
                    },
                    actor.id(),
                )?;
                working_copy.save_draft(
                    SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire(),
                    draft.editor_user_id.clone(),
                )?;
                let plan = SalesOrderWorkingCopyPersistPlan {
                    created_stable_lines: stable.created,
                    old_working_copy_lines: old_lines,
                    new_working_copy_lines: copy_lines.clone(),
                    replace_working_copy_lines: true,
                    create_working_copy: false,
                };
                (working_copy, copy_lines, plan)
            },
            None => {
                let (working_copy, copy_lines) =
                    Self::build_reopened_first_submission_working_copy(order, &stable.all, draft, actor)?;
                let plan = SalesOrderWorkingCopyPersistPlan {
                    created_stable_lines: stable.created,
                    old_working_copy_lines: Vec::new(),
                    new_working_copy_lines: copy_lines.clone(),
                    replace_working_copy_lines: false,
                    create_working_copy: true,
                };
                (working_copy, copy_lines, plan)
            },
        };
        Ok((working_copy, copy_lines, working_copy_plan))
    }
}
