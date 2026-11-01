//! 跨领域只读仓储，通过拥有领域公开仓储归集正式事实。
pub(super) mod approval;
mod approval_history;
mod finance;
pub(super) mod names;
pub(super) mod sources;
mod submissions;
mod task_adjustments;
mod task_sources;
use mongodb::Database;

pub(super) struct TimelineRepository<'a> {
    pub(super) db: &'a Database,
}
