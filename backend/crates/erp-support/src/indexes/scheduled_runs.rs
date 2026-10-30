//! 调度身份、活动槽位和回报重试索引。
use std::time::Duration;

use mongodb::bson::doc;
use mongodb::options::IndexOptions;
use mongodb::{Database, IndexModel};
use persistence_core::Result;
use persistence_core::indexes::{named_index, unique_index};

use super::create_indexes;
use crate::repository::scheduled_runs::ScheduledRunExt;

/// 创建调度回执约束；未回报记录不设置 TTL 字段。
/// # 参数
/// `db` 为业务数据库。
/// # 返回
/// 所有索引建立成功。
/// # 错误
/// 数据冲突或数据库失败时拒绝启动。
pub async fn ensure(db: &Database) -> Result<()> {
    let active_job = IndexOptions::builder()
        .name("uk_scheduled_runs_active_job".to_string())
        .unique(true)
        .partial_filter_expression(doc! {"state":{"$in":["accepted","running"]}})
        .build();
    let ttl =
        IndexOptions::builder().name("ttl_scheduled_runs".to_string()).expire_after(Duration::ZERO).build();
    create_indexes(
        db,
        <Database as ScheduledRunExt>::SCHEDULED_RUNS,
        vec![
            unique_index("uk_scheduled_runs_id", doc! {"id":1}),
            IndexModel::builder().keys(doc! {"app":1,"job_id":1}).options(active_job).build(),
            named_index("idx_scheduled_runs_claim", doc! {"app":1,"state":1,"created_at":1,"id":1}),
            named_index(
                "idx_scheduled_runs_callback",
                doc! {"app":1,"state":1,"callback_done":1,"callback_after":1,"id":1},
            ),
            IndexModel::builder().keys(doc! {"expires_at":1}).options(ttl).build(),
        ],
    )
    .await
}
