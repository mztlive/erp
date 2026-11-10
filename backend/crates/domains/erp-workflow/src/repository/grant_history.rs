//! 正式审批与转派证据的有界回填读取，不读取当前候选人。
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::{Deserialize, Serialize};

use crate::entity::work_item_command::WorkItemCommandReceipt;
use crate::repository::{BpmExt, WorkItemCommandExt, WorkItemExt};

/// 每页固定最多 25 条正式证据，调用方按末尾主键续页。
pub const HISTORY_PAGE_SIZE: i64 = 25;

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GrantHistoryKind {
    InvoiceApprovals,
    OrderAssignments,
    InvoiceReassignments,
}

pub enum GrantHistoryEntry {
    Invoice(ApprovalProcessInstance),
    Assignment(ApprovalNodeExecution),
    Reassignment(WorkItemCommandReceipt),
}
impl GrantHistoryEntry {
    /// 取得原始证据游标。
    /// # 参数
    /// 无。
    /// # 返回
    /// 拥有领域的稳定主键。
    /// # 错误
    /// 无。
    pub fn id(&self) -> &str {
        match self {
            Self::Invoice(row) => &row.base.id,
            Self::Assignment(row) => &row.base.id,
            Self::Reassignment(row) => &row.base.id,
        }
    }
}

/// 按来源集合的稳定主键读取下一页正式证据。
/// # 参数
/// db 为拥有集合的数据库，kind 为证据类型，after 为排他游标，executor 沿用调用事务。
/// # 返回
/// 最多 25 条；空页表示当前类型扫描结束。
/// # 错误
/// 数据库或反序列化失败时返回错误。
pub async fn page(
    db: &Database,
    kind: GrantHistoryKind,
    after: Option<&str>,
    executor: &mut dyn Executor,
) -> Result<Vec<GrantHistoryEntry>> {
    let filter = history_filter(kind, after);
    let options = FindOptions::builder().sort(doc! { "id": 1 }).limit(HISTORY_PAGE_SIZE).build();
    Ok(match kind {
        GrantHistoryKind::InvoiceApprovals => {
            mongo_ops::find_many(&db.approval_process_instances().collection(), filter, options, executor)
                .await?
                .into_iter()
                .map(GrantHistoryEntry::Invoice)
                .collect()
        },
        GrantHistoryKind::OrderAssignments => {
            mongo_ops::find_many(&db.approval_node_executions().collection(), filter, options, executor)
                .await?
                .into_iter()
                .map(GrantHistoryEntry::Assignment)
                .collect()
        },
        GrantHistoryKind::InvoiceReassignments => {
            mongo_ops::find_many(&db.work_item_command_receipts().collection(), filter, options, executor)
                .await?
                .into_iter()
                .map(GrantHistoryEntry::Reassignment)
                .collect()
        },
    })
}

fn history_filter(kind: GrantHistoryKind, after: Option<&str>) -> Document {
    let mut filter = doc! { "deleted_at": 0 };
    if let Some(after) = after {
        filter.insert("id", doc! { "$gt": after });
    }
    match kind {
        GrantHistoryKind::InvoiceApprovals => {
            filter.insert("subject.subject_kind", "sales_invoice_request");
            filter.insert("status", "APPROVED");
        },
        GrantHistoryKind::OrderAssignments => {},
        GrantHistoryKind::InvoiceReassignments => {
            filter.insert("result.kind", "reassigned");
        },
    }
    filter
}

/// 核验冻结审批人与实际生成的审批任务一一对应。
/// # 参数
/// db 为 workflow 数据库，execution 为节点执行，recipient 为冻结审批人，executor 为当前事务。
/// # 返回
/// 已生成未删除正式任务时为 true；受阻占位执行不产生资格。
/// # 错误
/// 数据库失败返回错误。
pub async fn assigned_to(
    db: &Database,
    execution: &str,
    recipient: &str,
    executor: &mut dyn Executor,
) -> Result<bool> {
    db.work_items().exists(doc! {"work_item_type":"DOCUMENT_APPROVAL", "approval_node_execution_id":execution, "responsibility_actor_ids":recipient}, executor).await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn history_pages_bind_formal_kind_deleted_state_and_exclusive_cursor() {
        assert_eq!(
            history_filter(GrantHistoryKind::InvoiceApprovals, Some("a")),
            doc! {
                "deleted_at": 0, "id": {"$gt":"a"}, "subject.subject_kind":"sales_invoice_request", "status":"APPROVED"
            }
        );
        assert_eq!(
            history_filter(GrantHistoryKind::InvoiceReassignments, None),
            doc! {"deleted_at":0, "result.kind":"reassigned"}
        );
        assert_eq!(
            history_filter(GrantHistoryKind::OrderAssignments, Some("node")),
            doc! {"deleted_at":0, "id":{"$gt":"node"}}
        );
    }
}
