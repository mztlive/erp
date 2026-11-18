//! 冻结销售变更提交、锁定工作副本，并在调用方事务内写入销售事实。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{
    SalesChangeOrderId, SalesChangeSubmissionId, SalesChangeSubmissionLineId, SalesOrderWorkingCopyId,
};
use id_generator::next_id;
use persistence_core::Executor;

use super::SalesReviewService;
use super::state::start_sales_change_approval;
use crate::entity::sales_order::SalesContentHash;
use crate::entity::sales_review::{
    SalesChangeOrder, SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
};
use crate::repository::prelude::*;
use crate::repository::{SalesOrderExt, SalesReviewExt};
use crate::{Error, Result};

/// 单次冻结提交的纯销售写入计划。
pub struct SalesChangeSubmissionWrite {
    change_order: SalesChangeOrder,
    working_copy: crate::entity::sales_order::SalesOrderWorkingCopy,
    submission: SalesChangeSubmission,
    submission_lines: Vec<SalesChangeSubmissionLine>,
}
impl SalesChangeSubmissionWrite {
    /// 已进入审批中、尚未写入的变更事实。
    /// 返回内存中的变更单。
    pub fn change(&self) -> &SalesChangeOrder {
        &self.change_order
    }
    /// 不可变提交头，供审批主体版本和快照映射。
    /// 返回尚未写入的提交头。
    pub fn submission(&self) -> &SalesChangeSubmission {
        &self.submission
    }
    /// 不可变提交行，供审批金额/数量快照映射。
    /// 返回尚未写入的提交行。
    pub fn lines(&self) -> &[SalesChangeSubmissionLine] {
        &self.submission_lines
    }
    /// 原命令回执及业务守卫检查完成后，在调用方事务内依次落冻结提交头与行、变更状态和工作副本。
    /// 结果：提交与工作副本都写入成功时返回 `Ok(())`。
    /// 失败条件：销售集合或 CAS 失败时原样返回；本方法不启动事务。
    ///
    /// 输入约定：
    /// - `db`：销售集合所在数据库。
    pub async fn persist(&mut self, db: &mongodb::Database, executor: &mut dyn Executor) -> Result<()> {
        db.sales_review()
            .submit_sales_change(&mut self.change_order, &self.submission, &self.submission_lines, executor)
            .await?;
        db.sales_order_working_copies().update(&mut self.working_copy, executor).await?;
        Ok(())
    }
}
impl SalesReviewService {
    /// 冻结审批绑定读取完成后，从当前变更草稿准备下一份不可变提交，供外层启动审批。
    /// 返回尚未写入的提交计划；变更单已在内存中进入审批中。
    /// 副本、版本、状态或快照不符合销售规则时失败。
    ///
    /// 输入约定：
    /// - `change_order`：冻结审批绑定读取之后的变更单。
    /// - `actor`：当前提交人。
    /// - `executor`：原用例事务执行器。
    pub async fn prepare_submission(
        &self,
        mut change_order: SalesChangeOrder,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<SalesChangeSubmissionWrite> {
        let (mut working_copy, copy_lines) =
            load_change_working_copy(&self.db, &change_order, executor).await?;
        let submission_no = next_change_submission_no(&self.db, &change_order.base.id, executor).await?;
        let (submission, submission_lines) =
            build_change_submission_with_no(&change_order, &working_copy, &copy_lines, submission_no, actor)?;

        working_copy.lock_for_submission_if_needed()?;
        start_sales_change_approval(
            &mut change_order,
            submission.base.id.clone().into(),
            SalesContentHash::submission(&submission.base.id)?.into_wire(),
            actor.id(),
        )?;
        Ok(SalesChangeSubmissionWrite { change_order, working_copy, submission, submission_lines })
    }
}

// 提交销售变更时按指定序号冻结工作副本及其明细，供本轮审批而非后续草稿读取。
// 返回 `(变更提交实体, 变更提交行清单)`。
// 工作副本关系、字段映射或提交实体校验失败时返回错误。
//
// 输入约定：
// - `change_order`：变更单
// - `working_copy`：变更工作副本
// - `lines`：工作副本行
// - `submission_no`：本次提交序号
// - `actor`：提交人
fn build_change_submission_with_no(
    change_order: &SalesChangeOrder,
    working_copy: &crate::entity::sales_order::SalesOrderWorkingCopy,
    lines: &[crate::entity::sales_order::SalesOrderWorkingCopyLine],
    submission_no: u32,
    actor: &AuditActor,
) -> Result<(SalesChangeSubmission, Vec<SalesChangeSubmissionLine>)> {
    let data = SalesChangeSubmissionData::from_sales_working_copy(
        change_order,
        working_copy,
        lines,
        submission_no,
        Instant::now(),
        actor.id(),
    )?;
    let line_datas = data.lines.clone();
    let mut submission = SalesChangeSubmission::new(SalesChangeSubmissionId::new(next_id()), data)?;
    submission.source_version = Some(change_order.base.version);

    let mut submission_lines = Vec::with_capacity(line_datas.len());
    for data in line_datas {
        submission_lines.push(SalesChangeSubmissionLine::new(
            SalesChangeSubmissionLineId::new(next_id()),
            submission.base.id.clone().into(),
            data,
        )?);
    }
    Ok((submission, submission_lines))
}

// 冻结变更提交前加载变更所绑定的工作副本与行，供同一准备阶段构建快照。
//
// 先取可编辑副本；撤回后再提交时按变更单定位已提交副本。
// 失败条件：工作副本不存在时返回 `NotFound`。
async fn load_change_working_copy(
    db: &mongodb::Database,
    change_order: &SalesChangeOrder,
    executor: &mut dyn Executor,
) -> Result<(
    crate::entity::sales_order::SalesOrderWorkingCopy,
    Vec<crate::entity::sales_order::SalesOrderWorkingCopyLine>,
)> {
    let working_copy = db
        .sales_order_working_copies()
        .find_resubmittable_sales_change_copy(
            &change_order.sales_order_id,
            &SalesChangeOrderId::new(change_order.base.id.clone()),
            executor,
        )
        .await?
        .ok_or_else(|| Error::NotFound("变更工作副本不存在".to_string()))?;
    let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
    let copy_lines =
        db.sales_order_working_copy_lines().list_lines_by_working_copy(&copy_id, executor).await?;
    Ok((working_copy, copy_lines))
}

// 重新提交变更时从最新冻结序号推导下一号，避免回退或重用旧审批版本。
// 返回严格递增的下一提交序号。
// 仓储失败或提交序号溢出时返回错误。
//
// 输入约定：
// - `change_order_id`：销售变更单 ID
// - `executor`：原用例事务执行器
async fn next_change_submission_no(
    db: &mongodb::Database,
    change_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<u32> {
    Ok(SalesChangeSubmission::next_submission_no(
        latest_change_submission_no(db, change_order_id, executor).await?,
    )?)
}

/// 读取已冻结的最大提交序号；尚无提交时返回 0。
/// 返回当前最大提交序号。
/// 仓储失败时返回错误。
///
/// 输入约定：
/// - `change_order_id`：销售变更单 ID
/// - `executor`：原用例事务执行器
pub async fn latest_change_submission_no(
    db: &mongodb::Database,
    change_order_id: &str,
    executor: &mut dyn Executor,
) -> Result<u32> {
    Ok(db
        .sales_change_submissions()
        .latest_submission_no_by_change_order(&SalesChangeOrderId::new(change_order_id), executor)
        .await?)
}
