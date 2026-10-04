//! 销售审批生命周期与调用方事务内的销售持久化步骤。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, PartyId, SalesOrderId, SalesOrderWorkingCopyId};
use persistence_core::{Executor, NoTransaction};

use super::SalesOrderService;
use super::draft_working_copy::DraftStableLines;
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

/// 提交并启动：进入 `PENDING_REVIEW` / `IN_APPROVAL`。
///
/// 版本权威来源是提交记录 `submission_no`，本方法不改写该编号。
/// 更新人必须是调用方提交销售，不得回落到上次更新人。
///
/// # 参数
/// * `order` - 待提交销售单
/// * `submitted_by` - 本次提交销售
///
/// # 返回
/// 状态迁移成功时返回 `Ok(())`，提交编号保持不变。
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
/// # 返回
/// 状态迁移成功时返回 `Ok(())`，提交编号保持不变。
///
/// # 错误
/// 非审批中时返回冲突。
pub fn cancel_sales_order_to_draft(order: &mut SalesOrder, updated_by: &str) -> Result<()> {
    Ok(order.cancel_approval_submission(updated_by)?)
}

/// 最终通过前置：仅 `IN_APPROVAL` 可进入生效。
///
/// # 参数
/// * `order` - 待执行最终审批动作的销售单
///
/// # 返回
/// 商业主状态与审核轨满足形式化前置状态时返回 `Ok(())`。
///
/// # 错误
/// 状态不是审批中时返回冲突。
pub fn ensure_final_approve_formalize(order: &SalesOrder) -> Result<()> {
    order
        .ensure_can_formalize()
        .map_err(|_| Error::ConflictError("只有审批中的销售单可以由最终通过动作形式化".to_string()))
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
    /// 按已证明的客户与结算身份构造初始销售单。
    ///
    /// # 参数
    /// * `req` - 建单输入及创建凭证
    /// * `customer_id` - 已校验的客户稳定身份
    /// * `settlement_party_id` - 已校验的结算主体
    /// * `actor` - 当前认证建单人，同时冻结为销售责任人
    /// * `business_org_unit_id` - 已解析的业务组织
    ///
    /// # 返回
    /// 返回尚未持久化的销售单，创建入口固定为 ERP。
    ///
    /// # 错误
    /// 销售单或创建凭证校验失败时保留领域错误分类。
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

    /// 在调用方事务内插入已校验的销售稳定对象。
    ///
    /// # 参数
    /// * `order` - 已完成领域构造和业务资格校验的销售单
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 插入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一约束冲突或仓储失败时返回对应错误。
    pub async fn create_order(&self, order: &SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().create(order, executor).await?)
    }

    /// 在调用方执行器中以乐观锁保存销售状态迁移。
    ///
    /// # 参数
    /// * `order` - 已完成领域状态迁移的销售单
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 保存成功并将新版本和时间元数据回填到 `order`。
    ///
    /// # 错误
    /// 版本冲突或仓储失败时返回对应错误。
    pub async fn persist_order(&self, order: &mut SalesOrder, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.db.sales_orders().update(order, executor).await?)
    }

    /// 依次插入新稳定行、工作副本及其明细，沿用调用方事务。
    ///
    /// # 参数
    /// * `stable` - 尚未保存的新稳定行
    /// * `copy` - 已校验的工作副本
    /// * `lines` - 按请求顺序排列的工作副本行
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部插入成功时返回 `Ok(())`，不写入审批或审计记录。
    ///
    /// # 错误
    /// 任一唯一约束冲突或仓储失败时立即停止并返回错误。
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

    /// 先插入不可变提交头，再按顺序插入不可变提交行。
    ///
    /// # 参数
    /// * `submission` - 已冻结的本轮提交
    /// * `lines` - 按请求顺序排列的冻结提交行
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 全部插入成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一约束冲突或仓储失败时立即停止并返回错误。
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

    /// 保存草稿行后更新工作副本，保留同一稳定明细的已持久化行身份。
    ///
    /// # 参数
    /// * `stable` - 尚未保存的新稳定明细
    /// * `old_lines` - 准备阶段读取的活跃行与 CAS 版本
    /// * `lines` - 已校验并重建的本次草稿内容
    /// * `copy` - 已完成领域修改的工作副本
    /// * `executor` - 调用方事务执行器，全部行和表头写入共用
    ///
    /// # 返回
    /// 保存替换内容，并把仓储成功写入后的表头元数据回填到 `copy`。
    ///
    /// # 错误
    /// 行或表头版本变化、唯一约束冲突、无效明细及仓储失败时立即返回。
    ///
    /// # 关键业务约束
    /// 仅软删移除行；保留行更新、重新加入行恢复，不重建既有唯一键。
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
        replace_working_copy_lines(&self.db, &copy.base.id.clone().into(), &old_lines, lines, executor)
            .await?;
        self.db.sales_order_working_copies().update(copy, executor).await?;
        Ok(())
    }

    /// 在外层命令回执、资格与权限检查之后保存销售提交及工作副本。
    ///
    /// # 参数
    /// * `order` - 已进入提交状态的销售单
    /// * `copy` - 已锁定的工作副本
    /// * `submission` - 本轮不可变提交快照
    /// * `lines` - 本轮不可变提交明细
    /// * `plan` - 新建或替换工作副本的行持久化计划
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 销售单、工作副本与不可变提交在同一执行器中保存。
    ///
    /// # 错误
    /// 工作副本行或表头版本变化、唯一约束冲突及仓储失败时立即停止。
    ///
    /// # 关键业务约束
    /// 工作副本行复用既有身份；提交快照仅插入，不修改工作流或审计。
    pub async fn persist_submission_start(
        &self,
        order: &mut SalesOrder,
        copy: &mut SalesOrderWorkingCopy,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
        plan: SalesOrderWorkingCopyPersistPlan,
        executor: &mut dyn Executor,
    ) -> Result<()> {
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

    /// 保存已校验的作废状态及可选的已放弃首次提交工作副本。
    ///
    /// # 参数
    /// * `order` - 已完成作废迁移的销售单
    /// * `copy` - 同时需要保存的已放弃工作副本
    /// * `executor` - 调用方事务执行器
    ///
    /// # 返回
    /// 先保存销售单，再保存存在的工作副本，并回填仓储元数据。
    ///
    /// # 错误
    /// 任一版本冲突或仓储失败时立即停止并返回错误。
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
    /// 从草稿请求重建工作副本行，并更新表头金额与内容指纹。
    ///
    /// # 参数
    /// * `order` - 草稿所属的销售稳定对象
    /// * `working_copy` - 准备修改的工作副本
    /// * `stable` - 已证明归属的稳定行及新增行
    /// * `draft` - 本次编辑请求
    /// * `actor` - 当前认证操作人
    ///
    /// # 返回
    /// 返回重建后的明细，同时修改工作副本；本方法不执行数据库写入。
    ///
    /// # 错误
    /// 依次校验表头快照、明细、金额和内容指纹，任一失败时返回对应错误。
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
    /// 准备已有工作副本替换方案或重新打开的首次提交工作副本。
    ///
    /// # 参数
    /// * `order` - 本次提交的销售单
    /// * `active_working_copy` - 当前活跃工作副本；缺省时重开首次提交副本
    /// * `stable` - 已证明归属的稳定行及新增行
    /// * `draft` - 本次提交的完整草稿内容
    /// * `version` - 客户端期望的已有工作副本版本
    /// * `actor` - 当前认证提交人
    ///
    /// # 返回
    /// 返回准备后的工作副本、明细和互斥持久化方案；本方法不写库。
    ///
    /// # 错误
    /// 已有副本先检查版本再读取旧行；版本冲突、快照或明细非法、仓储失败时返回错误。
    pub async fn prepare_submission_copy(
        &self,
        order: &SalesOrder,
        active_working_copy: Option<SalesOrderWorkingCopy>,
        stable: DraftStableLines,
        draft: &SalesOrderDraftRequest,
        version: u64,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>, SalesOrderWorkingCopyPersistPlan)>
    {
        match active_working_copy {
            Some(working_copy) => {
                self.prepare_existing_submission_copy(order, working_copy, stable, draft, version, actor)
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

    async fn prepare_existing_submission_copy(
        &self,
        order: &SalesOrder,
        mut working_copy: SalesOrderWorkingCopy,
        stable: DraftStableLines,
        draft: &SalesOrderDraftRequest,
        version: u64,
        actor: &AuditActor,
    ) -> Result<(SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>, SalesOrderWorkingCopyPersistPlan)>
    {
        if !working_copy.matches_version(version) {
            return Err(Error::ConflictError("数据已被其他请求修改，请刷新后重试".to_string()));
        }
        let order_id = SalesOrderId::new(order.base.id.clone());
        let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
        let old_lines = self
            .db
            .sales_order_working_copy_lines()
            .list_lines_by_working_copy(&copy_id, &mut NoTransaction)
            .await?;
        let copy_lines = build_working_copy_lines(&order_id, &copy_id, &stable.all, &draft.lines)?;
        Self::update_submission_copy(order, &mut working_copy, draft, &copy_lines, actor)?;
        let plan = SalesOrderWorkingCopyPersistPlan::ReplaceExisting {
            created_stable_lines: stable.created,
            old_working_copy_lines: old_lines,
            new_working_copy_lines: copy_lines.clone(),
        };
        Ok((working_copy, copy_lines, plan))
    }

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
                assert_eq!(message, "只有审批中的销售单可以由最终通过动作形式化");
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
