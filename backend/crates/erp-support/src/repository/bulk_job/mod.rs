//! 域 D04 `bulk_job` 仓储：bulk_selection_snapshot、bulk_selection_item、background_job、background_job_item。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本模块只补充域特有查询与
//! 跨集合多步骤写入入口。集合名直接引用 `extensions::BulkJobExt` 关联常量
//!（唯一来源，conventions §4.3），不做本地转存。
//!
//! 筛选/行类型按集合拆分定义，经 `BulkJobExt` 的关联类型对外暴露。

mod background_job;
mod background_job_item;
mod selection_item;
mod selection_snapshot;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, Result, mongo_ops};

pub use self::background_job::{BackgroundJobFilter, BackgroundJobRepositoryExt, BackgroundJobRow};
pub use self::background_job_item::{BackgroundJobItemRepositoryExt, BackgroundJobItemRow};
pub use self::selection_item::{BulkSelectionItemRepositoryExt, BulkSelectionItemRow};
pub use self::selection_snapshot::{
    BulkSelectionSnapshotFilter, BulkSelectionSnapshotRepositoryExt, BulkSelectionSnapshotRow,
};
use super::extensions::BulkJobExt;
use super::page::{created_updated_field, sort_direction};
use crate::entity::bulk_job::{BackgroundJob, BackgroundJobItem, BulkSelectionItem, BulkSelectionSnapshot};

/// 唯一请求身份仲裁后的后台任务登记结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum BackgroundJobRegistration {
    /// 本次原子写入任务、逐项行和审计。
    Created,
    /// 既有任务携带相同 v1 请求指纹。
    ReplaySame(BackgroundJob),
    /// 既有任务指纹不同，或为无指纹历史行。
    ConflictDifferentPayload(BackgroundJob),
}

/// D04 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `BulkJobExt::bulk_job()` 访问。
pub struct BulkJobRepository<'a> {
    db: &'a Database,
}

impl<'a> BulkJobRepository<'a> {
    /// 在唯一键竞争事务结束后按请求 ID 复核登记结果。
    ///
    /// 无指纹历史行采取失败关闭兼容策略：不猜测旧载荷，返回异载荷冲突，调用方
    /// 必须使用新的 request_id 重新提交。
    pub async fn registration_by_request_id(
        &self,
        requested: &BackgroundJob,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJobRegistration>> {
        let existing = mongo_ops::find_one(
            &self.db.collection::<BackgroundJob>(<Database as BulkJobExt>::BACKGROUND_JOBS),
            doc! {
                "request_id": &requested.request_id,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await?;
        Ok(existing.map(|existing| {
            if existing.request_fingerprint.is_some()
                && existing.request_fingerprint == requested.request_fingerprint
            {
                BackgroundJobRegistration::ReplaySame(existing)
            } else {
                BackgroundJobRegistration::ConflictDifferentPayload(existing)
            }
        }))
    }

    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 创建选择快照并冻结逐项目标（跨集合多步骤写入）。
    ///
    /// 依次写入 `bulk_selection_snapshots` 与 `bulk_selection_items`，保证
    /// 「快照 + 冻结目标集合」原子可见（数据模型 §6.1）。**必须收到事务
    /// 执行器**：本方法不构成原子边界，传入 `NoTransaction` 时两笔写入各自
    /// 自动提交，逐项失败会留下没有目标的空快照；Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `snapshot` - 待写入的选择快照
    /// * `items` - 待写入的冻结目标集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_snapshot_with_items(
        &self,
        snapshot: &BulkSelectionSnapshot,
        items: Vec<BulkSelectionItem>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<BulkSelectionSnapshot>(<Database as BulkJobExt>::BULK_SELECTION_SNAPSHOTS),
            snapshot,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<BulkSelectionItem>(<Database as BulkJobExt>::BULK_SELECTION_ITEMS),
            items,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 创建后台任务并登记逐项结果表（跨集合多步骤写入）。
    ///
    /// 依次写入 `background_jobs` 与 `background_job_items`，保证「任务注册 +
    /// 逐项行」原子可见（数据模型 §6.1）。**必须收到事务执行器**：本方法
    /// 不构成原子边界，传入 `NoTransaction` 时两笔写入各自自动提交，逐项
    /// 失败会留下没有逐项行的任务注册；Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `job` - 待写入的后台任务
    /// * `items` - 待写入的逐项结果行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 返回
    /// 成功插入任务与逐项行后返回 [`BackgroundJobRegistration::Created`]。
    /// 幂等回放与异载荷冲突不在本方法产生，由 [`Self::registration_by_request_id`]
    /// 在唯一键竞争后回查。
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_job_with_items(
        &self,
        job: &BackgroundJob,
        items: Vec<BackgroundJobItem>,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJobRegistration> {
        mongo_ops::insert_one(
            &self.db.collection::<BackgroundJob>(<Database as BulkJobExt>::BACKGROUND_JOBS),
            job,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<BackgroundJobItem>(<Database as BulkJobExt>::BACKGROUND_JOB_ITEMS),
            items,
            executor,
        )
        .await?;
        Ok(BackgroundJobRegistration::Created)
    }
}

/// 构建排序文档（排序字段白名单化，禁止透传任意字段名）。
///
/// 仅允许 `created_at` / `updated_at`；未知字段回落默认 `created_at`。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或白名单外字段时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = sort_direction(sort_ascending);
    let field = created_updated_field(sort_by);
    doc! { field: direction, "id": direction }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::sort_doc;

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(sort_doc(Some("updated_at"), true), doc! { "updated_at": 1, "id": 1 });
        assert_eq!(
            sort_doc(Some("job_no"), false),
            doc! { "created_at": -1, "id": -1 },
            "白名单外字段回落默认排序"
        );
    }
}
