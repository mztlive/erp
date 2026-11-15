//! 集成展示所需的跨领域只读仓储。

mod object_labels;
pub(crate) use object_labels::object_labels;
mod detail;
pub(super) use detail::{difference, error_task};
