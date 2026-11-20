//! 正式审批节点任务与读取授权事件保持同一提交边界。
use bpm::engine::TaskIntent;
use erp_identity::service::resource_access::append_event;
use erp_workflow::authorization::grant_facts::assignment_evidence;
use erp_workflow::service::approval::execution::PlannedWrites;
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, Result};

/// 为销售、采购审批的正式人工任务追加读取授权事件，供后台按实际分派计算单据读取权限。
/// 节点、任务与事件同事务写入；证据缺失、错配或事件写入失败时，调用方不得提交。
pub async fn append_assignments(
    db: &Database,
    writes: &PlannedWrites,
    executor: &mut dyn Executor,
) -> Result<()> {
    if !matches!(writes.instance.subject.subject_kind(), "sales_order" | "purchase_order") {
        return Ok(());
    }

    for intent in &writes.create_tasks {
        let TaskIntent::HumanTaskRequested { execution_id, assignee, .. } = intent else {
            continue;
        };
        let execution = writes
            .created_executions
            .iter()
            .find(|e| e.base.id == execution_id.as_ref())
            .ok_or_else(|| Error::Internal("正式分配缺少节点执行证据".into()))?;
        append_event(db, assignment_evidence(&writes.instance, execution, assignee.as_str())?, executor)
            .await?;
    }
    Ok(())
}
