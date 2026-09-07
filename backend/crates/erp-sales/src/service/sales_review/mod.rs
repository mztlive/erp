//! 销售变更本域查询、冻结准备和事务内写入。
//!
//! 销售变更单走统一审批启动/撤回/生效。采购二次确认、低毛利确认与卡券专用
//! 审批运行时已删除，不得回退旧责任动作或旧集合。

use crate::repository::SalesReviewExt;
use mongodb::Database;

mod command_source;
mod create;
mod effective;
mod formalization;
mod query;
mod state;
mod submission;
mod void;

pub use create::CreatedChangeWrite;
pub use effective::EffectiveChangeWrite;
pub use state::{
    cancel_sales_change_to_draft, ensure_final_approve_effective, persist_cancelled_change,
    start_sales_change_approval,
};
pub use submission::{latest_change_submission_no, SalesChangeSubmissionWrite};
pub use void::VoidChangeWrite;

/// 销售变更单列表筛选条件类型。
type SalesChangeOrderFilter = <mongodb::Database as SalesReviewExt>::SalesChangeOrderFilter;

/// 销售变更服务只持有自身仓储，不持有审批、审计、采购或财务能力。
pub struct SalesReviewService {
    db: Database,
}
impl SalesReviewService {
    /// 使用销售集合所在数据库创建单域服务。
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}

/// 合同 §4.4.4 的销售变更单域服务名称。
pub type SalesChangeOrderService = SalesReviewService;
