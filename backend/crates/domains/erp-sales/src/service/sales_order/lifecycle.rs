//! 销售审批生命周期与调用方事务内的销售持久化步骤。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId, SalesOrderWorkingCopyId};
use persistence_core::Executor;

use super::SalesOrderService;
use super::draft_working_copy::DraftStableLines;
use super::inquiry::{reconcile as reconcile_inquiry, release as release_inquiry};
use super::mapper::{build_working_copy_lines, header_snapshot};
use super::working_copy_persistence::replace_working_copy_lines;
use crate::dto::sales_order::{CreateSalesOrderRequest, SalesOrderDraftRequest};
use crate::entity::sales_order::{
    OriginSystem, SalesContentHash, SalesOrder, SalesOrderData, SalesOrderLine, SalesOrderSubmission,
    SalesOrderSubmissionLine, SalesOrderWorkingCopy, SalesOrderWorkingCopyLine, SalesOrderWorkingCopyUpdate,
};
use crate::repository::SalesOrderExt;
use crate::repository::prelude::*;
use crate::{Error, Result};

/// 正式发起销售审批时按新提交推进销售单商业和审核状态，进入 `PENDING_REVIEW` / `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
/// 更新人必须是调用方提交销售，不得回落到上次更新人。
/// 结果：状态迁移成功时返回 `Ok(())`，提交编号保持不变。
/// 失败条件：状态不允许时返回冲突。
///
/// 输入约定：
/// - `order`：待提交销售单
/// - `submitted_by`：本次提交销售
pub fn start_sales_order_approval(order: &mut SalesOrder, submitted_by: &str) -> Result<()> {
    Ok(order.start_approval_submission(submitted_by)?)
}

/// 撤回销售审批后恢复 `DRAFT` / `NOT_SUBMITTED`，但保留已用提交号给后续重提续号。
/// 结果：状态迁移成功时返回 `Ok(())`，提交编号保持不变。
/// 失败条件：非审批中时返回冲突。
///
/// 输入约定：
/// - `order`：审批中的销售单
pub fn cancel_sales_order_to_draft(order: &mut SalesOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval_submission(updated_by)?)
}

/// 最终审批正式生效之前确认来源仍为 `IN_APPROVAL`；不能由旧审批或草稿直接生成正式版本。
/// 结果：商业主状态与审核轨满足形式化前置状态时返回 `Ok(())`。
/// 失败条件：状态不是审批中时返回冲突。
///
/// 输入约定：
/// - `order`：待执行最终审批动作的销售单
pub fn ensure_final_approve_formalize(order: &SalesOrder) -> Result<()> {
    order.ensure_can_formalize().map_err(|_| {
        Error::ConflictError("只有审批中的销售单可以在审批通过后生效，请核对销售单状态".to_string())
    })
}

/// 销售提交的工作副本写入方案；调用方必须明确选择替换或新建。
#[derive(Clone)]
pub enum SalesOrderWorkingCopyPersistPlan {
    /// 替换已有工作副本的活跃行，并以旧行版本执行乐观锁检查。
    ReplaceExisting {
        /// 必须先于工作副本行保存的新稳定明细。
        created_stable_lines: Vec<SalesOrderLine>,
        /// 准备阶段读取的活跃行及其乐观锁版本。
        old_working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
        /// 按请求顺序重建的工作副本行。
        new_working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
    },
    /// 创建重新打开的首次提交工作副本及其明细。
    CreateNew {
        /// 必须先于工作副本保存的新稳定明细。
        created_stable_lines: Vec<SalesOrderLine>,
        /// 按请求顺序重建的工作副本行。
        new_working_copy_lines: Vec<SalesOrderWorkingCopyLine>,
    },
}

impl SalesOrderService {
    /// 外层核实客户和结算主体后准备新销售单主记录，使用已证明身份而不是重新从请求推断关系。
    /// 返回尚未持久化的销售单，创建入口固定为 ERP。
    /// 失败条件：销售单或创建凭证校验失败时保留领域错误分类。
    ///
    /// 输入约定：
    /// - `req`：建单输入及创建凭证
    /// - `customer_id`：已校验的客户稳定身份
    /// - `settlement_party_id`：已校验的结算主体
    /// - `actor`：当前认证建单人，同时冻结为销售责任人
    /// - `business_org_unit_id`：已解析的业务组织
    pub fn prepare_order(
        req: &CreateSalesOrderRequest,
        customer_id: CustomerAccountId,
        settlement_party_id: PartyId,
        actor: &AuditActor,
        business_org_unit_id: String,
    ) -> Result<SalesOrder> {
        let mut order = SalesOrder::new(
            SalesOrderId::new(id_generator::next_id()),
            SalesOrderData {
                sales_owner_user_id: actor.id().to_string(),
                business_org_unit_id,
                order_no: req.order_no.clone(),
                business_type: req.business_type,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id,
                contract_id: req.contract_id.clone(),
                settlement_party_id,
                source_status_code: None,
            },
            actor.id(),
        )?;
        order.set_creation_evidence(req.evidence_file_asset_ids.clone())?;
        Ok(order)
    }

    /// 销售建单通过前置校验后，在调用方事务写稳定主记录，供后续草稿与提交引用。
    /// 失败条件：唯一约束冲突或仓储失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `order`：已完成领域构造和业务资格校验的销售单
    pub async fn create_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().create(order, executor).await?)
    }

    /// 销售状态在领域内迁移完成后，用原执行器和乐观锁更新主记录，防止覆盖并发命令。
    /// 结果：保存成功并将新版本和时间元数据回填到 `order`。
    /// 失败条件：版本冲突或仓储失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `order`：已完成领域状态迁移的销售单
    pub async fn persist_order(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().update(order, executor).await?)
    }

    /// 建单或重开工作副本时用原事务依次写新增稳定行、工作副本及其明细。
    /// 结果：全部插入成功时返回 `Ok(())`，不写入审批或审计记录。
    /// 任一唯一约束冲突或仓储失败时立即停止并返回错误。
    ///
    /// 输入约定：
    /// - `stable`：尚未保存的新稳定行
    /// - `copy`：已校验的工作副本
    /// - `lines`：按请求顺序排列的工作副本行
    /// - `executor`：调用方事务执行器
    pub async fn create_working_copy(
        &self,
        stable: &[SalesOrderLine],
        copy: &SalesOrderWorkingCopy,
        lines: &[SalesOrderWorkingCopyLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        reconcile_inquiry(
            &self.db,
            copy.sales_order_id.as_ref(),
            &copy.customer_id,
            &lines.iter().map(Into::into).collect::<Vec<_>>(),
            executor,
        )
        .await?;

        for line in stable {
            self.db.sales_order_lines().create(line, executor).await?;
        }
        self.db.sales_order_working_copies().create(copy, executor).await?;
        for line in lines {
            self.db.sales_order_working_copy_lines().create(line, executor).await?;
        }
        Ok(())
    }

    /// 首次提交或重提交时先写已冻结的提交头，再按传入顺序写与该头绑定的不可变行。
    /// 结果：全部插入成功时返回 `Ok(())`。
    /// 唯一约束冲突或仓储失败时立即停止并返回错误。
    ///
    /// 输入约定：
    /// - `submission`：已冻结的本轮提交
    /// - `lines`：按请求顺序排列的冻结提交行
    /// - `executor`：调用方事务执行器
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

    /// 保存销售草稿时先写明细再更新表头；同一稳定明细继续使用持久化行身份。
    /// 结果：保存替换内容，并把仓储成功写入后的表头元数据回填到 `copy`。
    /// 失败条件：行或表头版本变化、唯一约束冲突、无效明细及仓储失败时立即返回。
    /// 业务边界：仅软删移除行；保留行更新、重新加入行恢复，不重建既有唯一键。
    ///
    /// 输入约定：
    /// - `stable`：尚未保存的新稳定明细
    /// - `old_lines`：准备阶段读取的活跃行与 CAS 版本
    /// - `lines`：已校验并重建的本次草稿内容
    /// - `copy`：已完成领域修改的工作副本
    /// - `executor`：调用方事务执行器，全部行和表头写入共用
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

        reconcile_inquiry(
            &self.db,
            copy.sales_order_id.as_ref(),
            &copy.customer_id,
            &lines.iter().map(Into::into).collect::<Vec<_>>(),
            executor,
        )
        .await?;

        replace_working_copy_lines(&self.db, &copy.base.id.clone().into(), &old_lines, lines, executor)
            .await?;
        self.db.sales_order_working_copies().update(copy, executor).await?;
        Ok(())
    }

    /// 过程层验证回执、操作资格和权限后，才在同一事务保存提交与工作副本状态。
    /// 结果：销售单、工作副本与不可变提交在同一执行器中保存。
    /// 失败条件：工作副本行或表头版本变化、唯一约束冲突及仓储失败时立即停止。
    /// 业务边界：工作副本行复用既有身份；提交快照仅插入，不修改工作流或审计。
    ///
    /// 输入约定：
    /// - `order`：已进入提交状态的销售单
    /// - `copy`：已锁定的工作副本
    /// - `submission`：本轮不可变提交快照
    /// - `lines`：本轮不可变提交明细
    /// - `plan`：新建或替换工作副本的行持久化计划
    /// - `executor`：调用方事务执行器
    pub async fn persist_submission_start(
        &self,
        order: &mut SalesOrder,
        copy: &mut SalesOrderWorkingCopy,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
        plan: SalesOrderWorkingCopyPersistPlan,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        reconcile_inquiry(
            &self.db,
            &order.base.id,
            &order.customer_id,
            &lines.iter().map(Into::into).collect::<Vec<_>>(),
            executor,
        )
        .await?;

        match plan {
            SalesOrderWorkingCopyPersistPlan::ReplaceExisting {
                created_stable_lines,
                old_working_copy_lines,
                new_working_copy_lines,
            } => {
                for line in &created_stable_lines {
                    self.db.sales_order_lines().create(line, executor).await?;
                }
                replace_working_copy_lines(
                    &self.db,
                    &copy.base.id.clone().into(),
                    &old_working_copy_lines,
                    &new_working_copy_lines,
                    executor,
                )
                .await?;
                self.db.sales_order().submit_working_copy(copy, submission, lines, executor).await?;
            },
            SalesOrderWorkingCopyPersistPlan::CreateNew { created_stable_lines, new_working_copy_lines } => {
                self.create_working_copy(&created_stable_lines, copy, &new_working_copy_lines, executor)
                    .await?;
                self.create_submission(submission, lines, executor).await?;
            },
        }

        self.db.sales_orders().update(order, executor).await?;
        Ok(())
    }

    /// 销售草稿作废校验完成后保存主单和可选的已放弃首次副本，沿用原执行器。
    /// 结果：先保存销售单，再保存存在的工作副本，并回填仓储元数据。
    /// 任一版本冲突或仓储失败时立即停止并返回错误。
    ///
    /// 输入约定：
    /// - `order`：已完成作废迁移的销售单
    /// - `copy`：同时需要保存的已放弃工作副本
    /// - `executor`：调用方事务执行器
    pub async fn persist_void(
        &self,
        order: &mut SalesOrder,
        copy: Option<&mut SalesOrderWorkingCopy>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        release_inquiry(&self.db, &order.base.id, executor).await?;
        self.db.sales_orders().update(order, executor).await?;
        if let Some(copy) = copy {
            self.db.sales_order_working_copies().update(copy, executor).await?;
        }
        Ok(())
    }
}

impl SalesOrderService {
    /// 保存销售草稿时用编辑请求准备工作副本行，按新内容重算表头金额和指纹以供后续提交。
    /// 返回重建后的明细，同时修改工作副本；本方法不执行数据库写入。
    /// 失败条件：依次校验表头快照、明细、金额和内容指纹，任一失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `order`：草稿所属的销售稳定对象
    /// - `working_copy`：准备修改的工作副本
    /// - `stable`：已证明归属的稳定行及新增行
    /// - `draft`：本次编辑请求
    /// - `actor`：当前认证操作人
    pub fn prepare_saved_working_copy(
        order: &SalesOrder,
        working_copy: &mut SalesOrderWorkingCopy,
        stable: &DraftStableLines,
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
    /// 准备提交时先区分有效副本替换与首次提交副本重开，返回待持久化的行及副本计划。
    /// 返回准备后的工作副本、明细和互斥持久化方案；本方法不写库。
    /// 已有副本先检查版本再读取旧行；版本冲突、快照或明细非法、仓储失败时返回错误。
    ///
    /// 输入约定：
    /// - `order`：本次提交的销售单
    /// - `active_working_copy`：当前活跃工作副本及客户端期望版本；缺省时重开首次提交副本
    /// - `stable`：已证明归属的稳定行及新增行
    /// - `draft`：本次提交的完整草稿内容
    /// - `actor`：当前认证提交人
    /// - `executor`：原提交事务执行器。
    pub async fn prepare_submission_copy(
        &self,
        order: &SalesOrder,
        active_working_copy: Option<(SalesOrderWorkingCopy, u64)>,
        stable: DraftStableLines,
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>, SalesOrderWorkingCopyPersistPlan)>
    {
        match active_working_copy {
            Some(working_copy) => {
                self.prepare_existing_submission_copy(order, working_copy, stable, draft, actor, executor)
                    .await
            },
            None => {
                let (working_copy, copy_lines) =
                    Self::build_reopened_first_submission_working_copy(order, &stable.all, draft, actor)?;
                let plan = SalesOrderWorkingCopyPersistPlan::CreateNew {
                    created_stable_lines: stable.created,
                    new_working_copy_lines: copy_lines.clone(),
                };
                Ok((working_copy, copy_lines, plan))
            },
        }
    }

    // 再次提交已有工作副本时，先用客户端版本检查草稿是否过期，再通过原事务加载旧行供对齐。
    async fn prepare_existing_submission_copy(
        &self,
        order: &SalesOrder,
        working_copy: (SalesOrderWorkingCopy, u64),
        stable: DraftStableLines,
        draft: &SalesOrderDraftRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>, SalesOrderWorkingCopyPersistPlan)>
    {
        let (mut working_copy, version) = working_copy;
        if !working_copy.matches_version(version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }

        let order_id = SalesOrderId::new(order.base.id.clone());
        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let old_lines =
            self.db.sales_order_working_copy_lines().list_lines_by_working_copy(&copy_id, executor).await?;
        let copy_lines = build_working_copy_lines(&order_id, &copy_id, &stable.all, &draft.lines)?;
        Self::update_submission_copy(order, &mut working_copy, draft, &copy_lines, actor)?;

        let plan = SalesOrderWorkingCopyPersistPlan::ReplaceExisting {
            created_stable_lines: stable.created,
            old_working_copy_lines: old_lines,
            new_working_copy_lines: copy_lines.clone(),
        };
        Ok((working_copy, copy_lines, plan))
    }

    // 保存待提交草稿时先递增 `draft_version`，以同一个新版本计算指纹和写工作副本，避免复用旧指纹身份。
    fn update_submission_copy(
        order: &SalesOrder,
        working_copy: &mut SalesOrderWorkingCopy,
        draft: &SalesOrderDraftRequest,
        lines: &[SalesOrderWorkingCopyLine],
        actor: &AuditActor,
    ) -> Result<()> {
        let (gross, net, tax) = SalesOrderWorkingCopyLine::amount_totals(lines);
        let next_version = working_copy.draft_version + 1;
        working_copy.update(
            SalesOrderWorkingCopyUpdate {
                content_hash: Some(SalesContentHash::draft(&working_copy.base.id, next_version)?.into_wire()),
                customer_id: Some(order.customer_id.clone()),
                contract_id: order.contract_id.clone(),
                contract_revision_id: draft.requested_contract_revision_id.clone(),
                settlement_party_id: Some(order.settlement_party_id.clone()),
                snapshot: Some(header_snapshot(draft)?),
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
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use entity_core::BaseModel;
    use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId};

    use super::ensure_final_approve_formalize;
    use crate::Error;
    use crate::entity::sales_order::{
        BusinessType, CommercialStatus, OriginSystem, ReviewStatus, SalesOrder, SalesOrderData,
    };

    #[test]
    fn final_approval_guard_keeps_service_conflict_and_does_not_require_attribution() {
        let mut order = SalesOrder::new(
            SalesOrderId::new("order-guard"),
            SalesOrderData {
                business_org_unit_id: "org-sales".to_string(),
                sales_owner_user_id: "sales-1".to_string(),
                order_no: "SO-GUARD".to_string(),
                business_type: BusinessType::GoodsService,
                origin_system: OriginSystem::Erp,
                source_identity_id: None,
                customer_id: CustomerAccountId::new("customer-1"),
                contract_id: None,
                settlement_party_id: PartyId::new("party-1"),
                source_status_code: None,
            },
            "sales-1",
        )
        .unwrap();
        order.base = BaseModel::fake();
        match ensure_final_approve_formalize(&order).unwrap_err() {
            Error::ConflictError(message) => {
                assert_eq!(message, "只有审批中的销售单可以在审批通过后生效，请核对销售单状态");
            },
            error => panic!("unexpected error class: {error}"),
        }

        order.commercial_status = CommercialStatus::PendingReview;
        order.review_status = ReviewStatus::InApproval;
        assert!(order.attribution.is_none());
        let before = order.clone();
        assert!(ensure_final_approve_formalize(&order).is_ok());
        assert_eq!(order, before);
    }
}
