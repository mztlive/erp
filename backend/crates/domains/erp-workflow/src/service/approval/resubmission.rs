//! 重新提交的统一说明校验、冻结记录及旧驳回待办关闭。

use bpm::model::SubjectRef;
use erp_core::common::time::Instant;
use mongodb::Database;
use persistence_core::Executor;

use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::repository::BpmExt;
use crate::service::work_item::approval_rework::complete_rework;
use crate::{Error, Result};

/// 在新提交快照保存前校验重提规则，并关闭前次驳回处理待办。
/// 快照记录规范化说明，旧实例待办已关闭时返回。
///
/// 历史未结束、版本未递增、驳回后说明为空、身份冲突或数据库失败时返回错误。
///
/// `db`：审批数据库；`snapshot`：本次新实例的冻结快照。
/// `reason`：请求中的重新提交说明，必须同时进入入口命令摘要；`actor_id`：已完成业务提交授权的操作人。
/// `executor`：创建新提交及实例的同一事务执行器。
pub async fn complete_resubmission(
    db: &Database,
    snapshot: &mut ApprovalSubjectSnapshot,
    reason: Option<&str>,
    actor_id: &str,
    now: Instant,
    executor: &mut dyn Executor,
) -> Result<()> {
    let subject = SubjectRef::new(snapshot.document_type.as_str(), &snapshot.business_object_id)
        .map_err(|error| Error::ValidationError(error.to_string()))?;
    let previous = db
        .bpm_workflow()
        .previous_instance_by_subject(&subject, &snapshot.approval_process_instance_id, executor)
        .await?;
    if let Some(rejected_id) = snapshot.record_resubmission(previous.as_ref(), reason, actor_id)? {
        complete_rework(db, &rejected_id, actor_id, now, executor).await?;
    }
    Ok(())
}
