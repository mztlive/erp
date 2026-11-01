//! 导入查询使用的后台任务事实消费端口。

mod bulk_job;

pub use bulk_job::{BulkJobFactsPort, FailClosedBulkJobFacts};
