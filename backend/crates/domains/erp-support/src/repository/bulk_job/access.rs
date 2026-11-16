//! 任务归属资格只读取存在性，不加载任务及私有文件引用。
use mongodb::Database;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use super::BulkJobRepository;
use crate::{BackgroundJob, BulkJobExt};

impl BulkJobRepository<'_> {
    /// 查询指定业务类型的真实未删除任务是否由该账号发起。
    /// # 参数
    /// id、domain 为固定任务及业务类型，requester 为当前账号，executor 为原授权快照。
    /// # 返回
    /// 只返回本人关系是否成立，不返回任务内容或私有文件引用。
    /// # 错误
    /// 数据库读取失败时返回错误。
    pub async fn is_requester(
        &self,
        id: &str,
        domain: &str,
        requester: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        mongo_ops::exists(
            &self.db.collection::<BackgroundJob>(Database::BACKGROUND_JOBS),
            doc! {"id": id, "domain_job_type": domain, "requested_by": requester, "deleted_at": 0},
            executor,
        )
        .await
    }
}
