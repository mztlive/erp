//! 工作项命令与工作台显示共用的唯一只读事实来源。
//! 命令 recipe 与显示 recipe 分别保留原查询边界；本 reader 不授权、写入或开启事务。
pub(in crate::workbench) mod amount;
pub(in crate::workbench) mod changes;
pub(in crate::workbench) mod command;
mod counterparties;
pub(in crate::workbench) mod fulfillment;
pub(in crate::workbench) mod funds;
pub(in crate::workbench) mod inventory;
pub(in crate::workbench) mod purchase;
mod recipe;
pub(in crate::workbench) mod sales;
mod sources;
pub(crate) use command::object_ids;
use erp_workflow::ports::{ObjectFact, ObjectFactMap, ObjectKind, SubjectBrief};
use mongodb::Database;
/// 按调用方 Executor 读取权威事实；构造不会执行 I/O。
#[derive(Clone)]
pub struct WorkItemFactsReader {
    pub(super) db: Database,
}
impl WorkItemFactsReader {
    /// 绑定与调用流程相同的数据库句柄。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
