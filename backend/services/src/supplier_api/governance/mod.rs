//! W20 供应商连接治理强命令、动作投影与后台任务执行。

mod command;
mod context;
mod jobs;
mod query;

#[cfg(test)]
mod tests;

pub(crate) use context::map_command_shape_rejection;
