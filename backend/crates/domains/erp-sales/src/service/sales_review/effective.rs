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
use crate::service::sales_order::inquiry::ensure_formal_change;
use crate::{Error, Result};

impl SalesReviewService {
    /// 最终审批通过时校验该变更可生效，并准备本域新销售修订；跨域差额交外层流程。
    /// 返回尚未落库的销售修订写入计划。
    /// 失败条件：状态、基准版本或正式修订不满足原合同则失败；变更单、提交、销售单或当前版本不存在以及仓储读取失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `id`：销售变更单。
    /// - `actor`：最终通过操作人。
    /// - `executor`：读取变更单的调用方执行器；其余销售读取仍走 `NoTransaction`。
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

// 开始变更生效前读取销售原单与冻结变更来源，随后在内存计算本域写入计划。
// 基准版本漂移、提交缺失或版本构造失败时返回错误。
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

// 以本轮冻结变更生成正式修订并迁移内存中的销售单和变更单，供后续事务落库。
// 版本号递增、当前版本读取、修订构造或生效状态迁移失败时返回错误。
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
    /// 返回变更单稳定身份。
    pub fn change_id(&self) -> &str {
        &self.change.base.id
    }

    /// 变更所属销售单标识。
    /// 返回销售单稳定身份。
    pub fn sales_order_id(&self) -> &str {
        self.change.sales_order_id.as_ref()
    }

    /// 变更所属销售单号。
    /// 返回销售业务单号。
    pub fn sales_order_no(&self) -> &str {
        &self.order.order_no
    }
    /// 在变更生效事务内先落新销售正式版本；外层流程据此记差额，再推进变更单。
    /// 结果：销售单和正式版本写入成功时返回 `Ok(())`。不在此处更新变更单。
    /// 失败条件：仓储或形式化写入失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `db`：销售集合所在数据库。
    /// - `session`：调用方事务执行器。
    pub async fn persist_revision(
        &mut self,
        db: &mongodb::Database,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        ensure_formal_change(
            db,
            &self.order.base.id,
            &self.order.customer_id,
            &self.revision.lines,
            &self.revision.goods_lines,
            session,
        )
        .await?;

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
    /// 差额和工作项处理成功后才写回变更状态，继续沿用外层生效事务。
    /// 结果：变更单更新成功时返回 `Ok(())`。
    /// 失败条件：仓储更新失败时返回对应错误。
    ///
    /// 输入约定：
    /// - `db`：销售集合所在数据库。
    /// - `session`：调用方事务执行器。
    pub async fn persist_change(
        &mut self,
        db: &mongodb::Database,
        session: &mut dyn persistence_core::Executor,
    ) -> Result<()> {
        db.sales_change_orders().update(&mut self.change, session).await?;
        Ok(())
    }

    /// 向下游提供新正式版本中已冻结的来源、主体及金额，由流程显式构造各消费方的输入。
    /// 返回来源销售单、修订、业务类型、变更前后含税金额和入账时间。
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
