//! 冻结销售变更提交、锁定工作副本，并在调用方事务内写入销售事实。

use application_core::AuditActor;
use erp_core::common::time::Instant;
use erp_core::ids::{
    SalesChangeOrderId, SalesChangeSubmissionId, SalesChangeSubmissionLineId, SalesOrderWorkingCopyId,
};
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};

use super::SalesReviewService;
use super::state::start_sales_change_approval;
use crate::entity::sales_order::SalesContentHash;
use crate::entity::sales_review::{
    SalesChangeOrder, SalesChangeSubmission, SalesChangeSubmissionData, SalesChangeSubmissionLine,
};
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
    pub fn change(&self) -> &SalesChangeOrder {
        &self.change_order
    }
    /// 不可变提交头，供审批主体版本和快照映射。
    pub fn submission(&self) -> &SalesChangeSubmission {
        &self.submission
    }
    /// 不可变提交行，供审批金额/数量快照映射。
    pub fn lines(&self) -> &[SalesChangeSubmissionLine] {
        &self.submission_lines
    }
    /// receipt 与业务单据守卫成功后，写入提交头、行、变更状态，再更新工作副本。
    ///
    /// # 错误
    /// 销售集合或 CAS 失败时原样返回；本方法不启动事务。
    pub async fn persist(&mut self, db: &mongodb::Database, executor: &mut dyn Executor) -> Result<()> {
        db.sales_review()
            .submit_sales_change(&mut self.change_order, &self.submission, &self.submission_lines, executor)
            .await?;
        db.sales_order_working_copies().update(&mut self.working_copy, executor).await?;
        Ok(())
    }
}
impl SalesReviewService {
    /// 在冻结审批绑定读取之后，按原读取时点准备下一不可变提交。
    ///
    /// # 错误
    /// 副本、版本、状态或快照不符合销售规则时失败。
    pub async fn prepare_submission(
        &self,
        mut change_order: SalesChangeOrder,
        actor: &AuditActor,
    ) -> Result<SalesChangeSubmissionWrite> {
        let (mut working_copy, copy_lines) = load_change_working_copy(&self.db, &change_order).await?;
        let submission_no = next_change_submission_no(&self.db, &change_order.base.id).await?;
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

/// 从变更工作副本构建变更提交快照。
///
/// # 参数
/// * `change_order` - 变更单
/// * `working_copy` - 变更工作副本
/// * `lines` - 工作副本行
/// * `submission_no` - 本次提交序号
/// * `actor` - 提交人
///
/// # 返回
/// 返回 `(变更提交实体, 变更提交行清单)`。
///
/// # 错误
/// 工作副本关系、字段映射或提交实体校验失败时返回错误。
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
    let submission = SalesChangeSubmission::new(SalesChangeSubmissionId::new(next_id()), data)?;
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

/// 加载变更工作副本及其明细。
///
/// 先取可编辑副本；撤回后再提交时按变更单定位已提交副本。
///
/// # 错误
/// 工作副本不存在时返回 `NotFound`。
async fn load_change_working_copy(
    db: &mongodb::Database,
    change_order: &SalesChangeOrder,
) -> Result<(
    crate::entity::sales_order::SalesOrderWorkingCopy,
    Vec<crate::entity::sales_order::SalesOrderWorkingCopyLine>,
)> {
    let working_copy = db
        .sales_order_working_copies()
        .find_resubmittable_sales_change_copy(
            &change_order.sales_order_id,
            &SalesChangeOrderId::new(change_order.base.id.clone()),
            &mut NoTransaction,
        )
        .await?
        .ok_or_else(|| Error::NotFound("变更工作副本不存在".to_string()))?;
    let copy_id = SalesOrderWorkingCopyId::new(working_copy.base.id.clone());
    let copy_lines =
        db.sales_order_working_copy_lines().list_lines_by_working_copy(&copy_id, &mut NoTransaction).await?;
    Ok((working_copy, copy_lines))
}

/// 计算下一次变更提交序号。
///
/// # 参数
/// * `db` - 数据库
/// * `change_order_id` - 销售变更单 ID
///
/// # 返回
/// 返回严格递增的下一提交序号。
///
/// # 错误
/// 仓储失败或提交序号溢出时返回错误。
async fn next_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    Ok(SalesChangeSubmission::next_submission_no(latest_change_submission_no(db, change_order_id).await?)?)
}

/// 读取已冻结的最大提交序号；尚无提交时返回 0。
///
/// # 参数
/// * `db` - 数据库
/// * `change_order_id` - 销售变更单 ID
///
/// # 返回
/// 返回当前最大提交序号。
///
/// # 错误
/// 仓储失败时返回错误。
pub async fn latest_change_submission_no(db: &mongodb::Database, change_order_id: &str) -> Result<u32> {
    Ok(db
        .sales_change_submissions()
        .latest_submission_no_by_change_order(&SalesChangeOrderId::new(change_order_id), &mut NoTransaction)
        .await?)
}
