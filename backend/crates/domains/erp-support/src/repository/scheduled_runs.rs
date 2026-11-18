//! 调度回执的原子存储；活动 Job ID 唯一索引实现 DISCARD_LATER。
use async_trait::async_trait;
use mongodb::bson::{DateTime, doc};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use persistence_core::{Error, NoTransaction, Result, mongo_ops};

use crate::entity::scheduled_run::{ScheduledRun, ScheduledState};
use crate::ports::scheduled_runs::ScheduledRunStore;

/// 调度回执仓储访问器。
pub trait ScheduledRunExt {
    const SCHEDULED_RUNS: &'static str = "scheduled_runs";
    /// 返回本域调度回执仓储。
    /// # 参数
    /// 无。
    /// # 返回
    /// 不发起 I/O 的仓储。
    /// # 错误
    /// 无。
    fn scheduled_runs(&self) -> MongoScheduledRuns;
}
impl ScheduledRunExt for Database {
    fn scheduled_runs(&self) -> MongoScheduledRuns {
        MongoScheduledRuns(self.collection(Self::SCHEDULED_RUNS))
    }
}

/// 调度回执集合的唯一拥有者。
pub struct MongoScheduledRuns(Collection<ScheduledRun>);

#[async_trait]
impl ScheduledRunStore for MongoScheduledRuns {
    async fn find(&self, key: &str) -> Result<Option<ScheduledRun>> {
        mongo_ops::find_one(&self.0, doc! {"id":key}, &mut NoTransaction).await
    }
    async fn insert(&self, run: &ScheduledRun) -> Result<()> {
        mongo_ops::insert_one(&self.0, run, &mut NoTransaction).await
    }
    async fn job_busy(&self, app: &str, job_id: i32) -> Result<bool> {
        Ok(mongo_ops::find_one(
            &self.0,
            doc! {"app":app,"job_id":job_id,"state":{"$in":["accepted","running"]}},
            &mut NoTransaction,
        )
        .await?
        .is_some())
    }
    async fn claim(&self, app: &str, owner: &str) -> Result<Option<ScheduledRun>> {
        let options = FindOptions::builder().sort(doc! {"created_at":1,"id":1}).limit(1).build();
        let mut rows =
            mongo_ops::find_many(&self.0, doc! {"app":app,"state":"accepted"}, options, &mut NoTransaction)
                .await?;
        let Some(mut run) = rows.pop() else { return Ok(None) };

        let changed = mongo_ops::update_one(
            &self.0,
            doc! {"id":&run.base.id,"state":"accepted"},
            doc! {"$set":{"state":"running","owner":owner},"$inc":{"version":1}},
            false,
            &mut NoTransaction,
        )
        .await?;
        if changed.modified_count == 0 {
            return Ok(None);
        }

        run.state = ScheduledState::Running;
        run.owner = Some(owner.into());
        run.base.version += 1;
        Ok(Some(run))
    }
    async fn finish(&self, run: &ScheduledRun, code: i32, message: &str) -> Result<()> {
        let changed = mongo_ops::update_one(
            &self.0,
            doc! {"id":&run.base.id,"state":"running","owner":&run.owner},
            doc! {"$set":{"state":"completed","handle_code":code,"handle_msg":message},
            "$inc":{"version":1}},
            false,
            &mut NoTransaction,
        )
        .await?;
        if changed.matched_count != 1 {
            return Err(Error::OptimisticLockingError);
        }
        Ok(())
    }
    async fn recover(&self, app: &str, owner: &str) -> Result<()> {
        mongo_ops::update_many(&self.0, doc! {"app":app,"state":"running","owner":{"$ne":owner}},
            doc! {"$set":{"state":"completed","handle_code":500,"handle_msg":"进程中断，业务结果请查询 ERP 原记录；本次不重放"},
                "$inc":{"version":1}}, &mut NoTransaction).await?;
        Ok(())
    }
    async fn callbacks(&self, app: &str, now: i64) -> Result<Vec<ScheduledRun>> {
        let options = FindOptions::builder().sort(doc! {"callback_after":1,"id":1}).limit(20).build();
        mongo_ops::find_many(
            &self.0,
            doc! {"app":app,"state":"completed","callback_done":false,"callback_after":{"$lte":now}},
            options,
            &mut NoTransaction,
        )
        .await
    }
    async fn callback_result(&self, run: &ScheduledRun, success: bool, now: i64) -> Result<()> {
        let mut fields = doc! {"callback_after":now.saturating_add(30)};
        if success {
            fields.insert("callback_done", true);
            fields.insert(
                "expires_at",
                DateTime::from_millis(now.saturating_add(30 * 86400).saturating_mul(1000)),
            );
        }

        mongo_ops::update_one(
            &self.0,
            doc! {"id":&run.base.id,"state":"completed","callback_done":false},
            doc! {"$set":fields},
            false,
            &mut NoTransaction,
        )
        .await?;
        Ok(())
    }
}
