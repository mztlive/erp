//! 销售变更正式版本准备和事务内销售写入；保留原 NoTransaction 读取边界。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use persistence_core::NoTransaction;

use super::SalesReviewService;
use super::formalization::{build_change_revision, revision_gross};
use crate::entity::sales_review::{SalesChangeOrder, SalesChangeSubmission, SalesChangeSubmissionLine};
use crate::repository::prelude::*;
use crate::repository::{SalesOrderExt, SalesReviewExt};
use crate::{Error, Result};

impl SalesReviewService {
    /// 校验最终通过状态并准备销售修订；本方法只产生销售本域写入计划。
    ///
    /// # 参数
    /// * `id` - 销售变更单。
    /// * `actor` - 最终通过操作人。
    /// * `executor` - 读取变更单的调用方执行器；其余销售读取仍走 `NoTransaction`。
    ///
    /// # 返回
    /// 返回尚未落库的销售修订写入计划。
    ///
    /// # 错误
    /// 状态、基准版本或正式修订不满足原合同则失败；变更单、提交、销售单或当前版本不存在以及仓储读取失败时返回对应错误。
    pub async fn prepare_effective_change(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<EffectiveChangeWrite> {
        let change_order = self
            .db
            .sales_change_orders()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("销售变更单不存在".to_string()))?;
        super::state::ensure_final_approve_effective(&change_order)?;
        prepare_effective_change_write(&self.db, change_order, actor).await
    }
}

/// 读取销售变更来源并完成生效写入计划的领域计算。
///
/// # 错误
/// 基准版本漂移、提交缺失或版本构造失败时返回错误。
async fn prepare_effective_change_write(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    actor: &AuditActor,
) -> Result<EffectiveChangeWrite> {
    let submission_id = change_order.required_current_submission_id()?;
    let submission = db
        .sales_change_submissions()
        .find_by_id(submission_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("变更提交不存在".to_string()))?;
    let submission_lines = db
        .sales_change_submission_lines()
        .list_lines_by_submission(&submission.base.id.clone().into(), &mut NoTransaction)
        .await?;
    let order = db
        .sales_orders()
        .find_by_id(&change_order.sales_order_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("销售单不存在".to_string()))?;
    let current_revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    if !change_order.base_revision_matches(current_revision_id) {
        return Err(Error::ConflictError("基准版本已不是销售单当前版本，请刷新后重新发起变更".to_string()));
    }
    prepare_effective_revision_write(db, change_order, order, submission, submission_lines, actor).await
}

/// 在内存中构造生效修订和待写入的销售单、变更单，不在此处落库。
///
/// # 错误
/// 版本号递增、当前版本读取、修订构造或生效状态迁移失败时返回错误。
async fn prepare_effective_revision_write(
    db: &mongodb::Database,
    change_order: SalesChangeOrder,
    order: crate::entity::sales_order::SalesOrder,
    submission: SalesChangeSubmission,
    submission_lines: Vec<SalesChangeSubmissionLine>,
    actor: &AuditActor,
) -> Result<EffectiveChangeWrite> {
    let now = Instant::now();
    let current_revision_no = db
        .sales_order_revisions()
        .latest_revision_no(&change_order.sales_order_id, &mut NoTransaction)
        .await?;
    let revision_no =
        crate::entity::sales_order::SalesOrderRevision::next_revision_no(current_revision_no.unwrap_or(0))?;
    let revision = build_change_revision(&order, &submission, &submission_lines, revision_no, now)?;
    let current_revision_id = order
        .current_revision_id()
        .ok_or_else(|| Error::BusinessLogicError("销售单缺少当前版本".to_string()))?;
    let current_revision = db
        .sales_order_revisions()
        .find_by_id(current_revision_id, &mut NoTransaction)
        .await?
        .ok_or_else(|| Error::NotFound("销售单当前版本不存在".to_string()))?;
    let mut order_for_tx = order;
    order_for_tx.attach_revision(&revision.revision.base.id, actor.id());
    let mut change_for_tx = change_order;
    change_for_tx.apply_effective(revision.revision.base.id.clone().into(), actor.id())?;
    Ok(EffectiveChangeWrite {
        order: order_for_tx,
        change: change_for_tx,
        revision,
        current_gross: current_revision.gross_amount,
        posted_at: now,
    })
}

/// 销售变更最终通过的单次销售写入计划；不持有财务账户、审批或审计。
pub struct EffectiveChangeWrite {
    order: crate::entity::sales_order::SalesOrder,
    change: SalesChangeOrder,
    revision: super::formalization::RevisionAggregate,
    current_gross: Amount,
    posted_at: Instant,
}
impl EffectiveChangeWrite {
    /// 待最终生效的销售变更单标识。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回变更单稳定身份。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn change_id(&self) -> &str {
        &self.change.base.id
    }
    /// 写入正式销售版本；外层流程随后写入差额，再推进变更单。
    ///
    /// # 参数
    /// * `db` - 销售集合所在数据库。
    /// * `session` - 调用方事务执行器。
    ///
    /// # 返回
    /// 销售单和正式版本写入成功时返回 `Ok(())`。不在此处更新变更单。
    ///
    /// # 错误
    /// 仓储或形式化写入失败时返回对应错误。
    pub async fn persist_revision(
        &mut self,
        db: &mongodb::Database,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        db.sales_order()
            .formalize_submission(
                &mut self.order,
                &self.revision.revision,
                &self.revision.lines,
                &self.revision.goods_lines,
                &self.revision.voucher_lines,
                session,
            )
            .await?;
        Ok(())
    }
    /// 在差额与工作项成功后写回销售变更状态，不创建新的事务。
    ///
    /// # 参数
    /// * `db` - 销售集合所在数据库。
    /// * `session` - 调用方事务执行器。
    ///
    /// # 返回
    /// 变更单更新成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 仓储更新失败时返回对应错误。
    pub async fn persist_change(
        &mut self,
        db: &mongodb::Database,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        db.sales_change_orders().update(&mut self.change, session).await?;
        Ok(())
    }

    /// 冻结销售修订的来源与金额，供流程显式映射下游消费方事实。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回来源销售单、修订、业务类型、变更前后含税金额和入账时间。
    ///
    /// # 错误
    /// `revision_gross` 不产生 `Err`，本方法因此不失败。
    pub fn revision_fact(&self) -> Result<crate::ports::sales_review::SalesChangeRevisionFact> {
        Ok(crate::ports::sales_review::SalesChangeRevisionFact {
            sales_order_id: self.order.base.id.clone().into(),
            revision_id: self.revision.revision.base.id.clone().into(),
            business_type: self.order.business_type,
            current_gross: self.current_gross,
            new_gross: revision_gross(&self.revision)?,
            posted_at: self.posted_at,
        })
    }
}
