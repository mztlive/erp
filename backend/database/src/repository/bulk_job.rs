//! 域 D04 `bulk_job` 仓储：bulk_selection_snapshot、bulk_selection_item、background_job、background_job_item。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `extensions::BulkJobExt` 关联常量
//! 导入（conventions §4.3）。
//!
//! 筛选/行类型定义在本文件，经 `BulkJobExt` 的关联类型对外暴露。

use entities::bulk_job::{
    BackgroundJob, BackgroundJobId, BackgroundJobItem, BulkSelectionItem, BulkSelectionSnapshot,
    BulkSelectionSnapshotId, ItemStatus, JobStatus, JobType, SelectionItemStatus, SelectionStatus,
    SelectionType,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::BulkJobExt;
use super::{regex_filter::insert_literal_regex_filter, PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

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

/// `bulk_selection_snapshot` 集合名（单一来源：`BulkJobExt` 关联常量）。
const BULK_SELECTION_SNAPSHOTS: &str = <mongodb::Database as BulkJobExt>::BULK_SELECTION_SNAPSHOTS;
/// `bulk_selection_item` 集合名（单一来源：`BulkJobExt` 关联常量）。
const BULK_SELECTION_ITEMS: &str = <mongodb::Database as BulkJobExt>::BULK_SELECTION_ITEMS;
/// `background_job` 集合名（单一来源：`BulkJobExt` 关联常量）。
const BACKGROUND_JOBS: &str = <mongodb::Database as BulkJobExt>::BACKGROUND_JOBS;
/// `background_job_item` 集合名（单一来源：`BulkJobExt` 关联常量）。
const BACKGROUND_JOB_ITEMS: &str = <mongodb::Database as BulkJobExt>::BACKGROUND_JOB_ITEMS;

/// 选择快照列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionSnapshotRow {
    /// 实体主键。
    pub id: String,
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 数据截止水位（秒级时间戳）。
    pub data_cutoff_at: u64,
    /// 冻结目标数。
    pub item_count: u32,
    /// 创建人。
    pub created_by: String,
    /// 有效期截止时间（秒级时间戳）。
    pub expires_at: u64,
    /// 快照状态。
    pub status: SelectionStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 选择快照列表筛选条件。
#[derive(Debug, Clone)]
pub struct BulkSelectionSnapshotFilter {
    /// 选择类型；`None` 表示不筛选。
    pub selection_type: Option<SelectionType>,
    /// 快照状态；`None` 表示不筛选。
    pub status: Option<SelectionStatus>,
    /// 创建人；`None` 表示不筛选。
    pub created_by: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for BulkSelectionSnapshotFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(selection_type) = self.selection_type {
            filter.insert("selection_type", selection_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(created_by) = &self.created_by {
            filter.insert("created_by", created_by);
        }
        filter
    }
}

impl Pagination for BulkSelectionSnapshotFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, BulkSelectionSnapshot> {
    /// 分页检索选择快照列表（投影查询）。
    ///
    /// 只返回 [`BulkSelectionSnapshotRow`] 所需的列表字段，不加载整文档；
    /// `created_by` 精确匹配覆盖 `idx_bulk_selection_snapshots_created`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_snapshots(
        &self,
        filter: &BulkSelectionSnapshotFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionSnapshotRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(snapshot_projection())
            .build();
        let collection = self.collection().clone_with_type::<BulkSelectionSnapshotRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 选择项逐项结果投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionItemRow {
    /// 实体主键。
    pub id: String,
    /// 目标对象类型代码。
    pub object_type: String,
    /// 目标对象 ID。
    pub object_id: String,
    /// 预览时版本。
    pub expected_version: Option<String>,
    /// 预览时内容摘要。
    pub expected_hash: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub result_status: Option<SelectionItemStatus>,
    /// 失败原因代码（适用时）。
    pub result_code: Option<String>,
}

impl<'a> Repository<'a, BulkSelectionItem> {
    /// 分页检索快照逐项结果（投影查询）。
    ///
    /// 只返回 [`BulkSelectionItemRow`] 所需的逐项字段，不加载整文档；
    /// `result_status` 过滤覆盖 `idx_bulk_selection_items_result`。
    ///
    /// # 参数
    /// * `snapshot_id` - 选择快照 ID
    /// * `result_status` - 逐项执行结果；`None` 表示不筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_items(
        &self,
        snapshot_id: &BulkSelectionSnapshotId,
        result_status: Option<SelectionItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BulkSelectionItemRow>> {
        let mut filter = doc! { "selection_snapshot_id": snapshot_id.to_string() };
        if let Some(result_status) = result_status {
            filter.insert("result_status", result_status.as_str());
        }
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1 })
            .skip((page.max(1) - 1) * u64::from(page_size))
            .limit(i64::from(page_size))
            .projection(selection_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<BulkSelectionItemRow>();
        let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter, executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 后台任务列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobRow {
    /// 实体主键。
    pub id: String,
    /// 任务编号。
    pub job_no: String,
    /// 任务类型。
    pub job_type: JobType,
    /// 关联强类型领域任务类型代码。
    pub domain_job_type: Option<String>,
    /// 关联强类型领域任务 ID。
    pub domain_job_id: Option<String>,
    /// 批量或导出使用的不可变选择快照。
    pub selection_snapshot_id: Option<String>,
    /// 任务状态。
    pub status: JobStatus,
    /// 发起人。
    pub requested_by: String,
    /// 请求幂等身份。
    pub request_id: String,
    /// 合规输入包文件资产。
    pub input_file_asset_id: Option<String>,
    /// 结果文件资产。
    pub result_file_asset_id: Option<String>,
    /// 目标总数。
    pub total_count: u64,
    /// 已处理数。
    pub processed_count: u64,
    /// 成功数。
    pub success_count: u64,
    /// 跳过数。
    pub skipped_count: u64,
    /// 失败数。
    pub failed_count: u64,
    /// 开始执行时间（秒级时间戳）。
    pub started_at: Option<u64>,
    /// 结束时间（秒级时间戳）。
    pub finished_at: Option<u64>,
    /// 最近进度时间（秒级时间戳）。
    pub last_progress_at: Option<u64>,
    /// 结果下载到期时间（秒级时间戳）。
    pub result_expires_at: Option<u64>,
    /// 脱敏任务级错误摘要。
    pub error_summary: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 后台任务列表筛选条件。
#[derive(Debug, Clone)]
pub struct BackgroundJobFilter {
    /// 任务编号（忽略大小写字面量模糊匹配）；`None` 表示不筛选。
    pub job_no: Option<String>,
    /// 任务类型；`None` 表示不筛选。
    pub job_type: Option<JobType>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<JobStatus>,
    /// 发起人；`None` 表示不筛选。
    pub requested_by: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at` / `updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for BackgroundJobFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "job_no", self.job_no.as_deref());
        if let Some(job_type) = self.job_type {
            filter.insert("job_type", job_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(requested_by) = &self.requested_by {
            filter.insert("requested_by", requested_by);
        }
        filter
    }
}

impl Pagination for BackgroundJobFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, BackgroundJob> {
    /// 分页检索后台任务列表（投影查询，任务中心）。
    ///
    /// 只返回 [`BackgroundJobRow`] 所需的进度字段，不加载整文档；
    /// `job_no` 按字面量忽略大小写模糊匹配（复用 `repository::regex_filter`），
    /// 状态/发起人精确匹配覆盖 `idx_background_jobs_status_created`。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_background_jobs(
        &self,
        filter: &BackgroundJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(background_job_projection())
            .build();
        let collection = self.collection().clone_with_type::<BackgroundJobRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按任务编号查找后台任务。
    ///
    /// 查询覆盖 `uk_background_jobs_no` 唯一索引；任务中心按编号精确路由。
    ///
    /// # 参数
    /// * `job_no` - 任务编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_job_no(
        &self,
        job_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        self.find_one_by_field("job_no", job_no, executor).await
    }

    /// 按请求幂等身份查找后台任务。
    ///
    /// 查询覆盖 `uk_background_jobs_request_id` 唯一索引；幂等重试按
    /// `request_id` 定位既有任务（§6.1：涉及资金的变更必须具备幂等键）。
    ///
    /// # 参数
    /// * `request_id` - 请求幂等身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_request_id(
        &self,
        request_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        self.find_one_by_field("request_id", request_id, executor).await
    }
}

/// 后台任务逐项结果投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobItemRow {
    /// 实体主键。
    pub id: String,
    /// 稳定逐项序号。
    pub item_no: u32,
    /// 已有对象类型代码。
    pub object_type: Option<String>,
    /// 已有对象 ID。
    pub object_id: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub status: Option<ItemStatus>,
    /// 脱敏原因代码。
    pub result_code: Option<String>,
    /// 脱敏结果摘要。
    pub result_summary: Option<String>,
    /// 成功形成的对象类型代码。
    pub result_object_type: Option<String>,
    /// 成功形成的对象 ID。
    pub result_object_id: Option<String>,
}

impl<'a> Repository<'a, BackgroundJobItem> {
    /// 分页检索任务逐项结果（投影查询）。
    ///
    /// 只返回 [`BackgroundJobItemRow`] 所需的逐项字段，不加载整文档；
    /// `status` 过滤覆盖 `idx_background_job_items_status`。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `status` - 逐项执行结果；`None` 表示不筛选
    /// * `page` - 页码（1 起）
    /// * `page_size` - 单页条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页逐项结果行与总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_job_items(
        &self,
        job_id: &BackgroundJobId,
        status: Option<ItemStatus>,
        page: u64,
        page_size: u32,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<BackgroundJobItemRow>> {
        let mut filter = doc! { "background_job_id": job_id.to_string() };
        if let Some(status) = status {
            filter.insert("status", status.as_str());
        }
        let options = FindOptions::builder()
            .sort(doc! { "item_no": 1 })
            .skip((page.max(1) - 1) * u64::from(page_size))
            .limit(i64::from(page_size))
            .projection(job_item_projection())
            .build();
        let collection = self.collection().clone_with_type::<BackgroundJobItemRow>();
        let items = mongo_ops::find_many(&collection, filter.clone(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter, executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
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
            &self.db.collection::<BackgroundJob>(BACKGROUND_JOBS),
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
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `snapshot` - 待写入的选择快照
    /// * `items` - 待写入的冻结目标集合
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_snapshot_with_items(
        &self,
        snapshot: &BulkSelectionSnapshot,
        items: Vec<BulkSelectionItem>,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<BulkSelectionSnapshot>(BULK_SELECTION_SNAPSHOTS),
            snapshot,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<BulkSelectionItem>(BULK_SELECTION_ITEMS),
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
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `job` - 待写入的后台任务
    /// * `items` - 待写入的逐项结果行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_job_with_items(
        &self,
        job: &BackgroundJob,
        items: Vec<BackgroundJobItem>,
        executor: &mut dyn Executor,
    ) -> Result<BackgroundJobRegistration> {
        mongo_ops::insert_one(
            &self.db.collection::<BackgroundJob>(BACKGROUND_JOBS),
            job,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<BackgroundJobItem>(BACKGROUND_JOB_ITEMS),
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
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("updated_at") => "updated_at",
        _ => "created_at",
    };
    doc! { field: direction, "id": direction }
}

/// 选择快照列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn snapshot_projection() -> Document {
    doc! {
        "id": 1,
        "selection_type": 1,
        "data_cutoff_at": 1,
        "item_count": 1,
        "created_by": 1,
        "expires_at": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 选择项逐项结果投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn selection_item_projection() -> Document {
    doc! {
        "id": 1,
        "selection_snapshot_id": 1,
        "object_type": 1,
        "object_id": 1,
        "expected_version": 1,
        "expected_hash": 1,
        "result_status": 1,
        "result_code": 1,
    }
}

/// 后台任务列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn background_job_projection() -> Document {
    doc! {
        "id": 1,
        "job_no": 1,
        "job_type": 1,
        "domain_job_type": 1,
        "domain_job_id": 1,
        "selection_snapshot_id": 1,
        "status": 1,
        "requested_by": 1,
        "request_id": 1,
        "input_file_asset_id": 1,
        "result_file_asset_id": 1,
        "total_count": 1,
        "processed_count": 1,
        "success_count": 1,
        "skipped_count": 1,
        "failed_count": 1,
        "started_at": 1,
        "finished_at": 1,
        "last_progress_at": 1,
        "result_expires_at": 1,
        "error_summary": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 后台任务逐项结果投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn job_item_projection() -> Document {
    doc! {
        "id": 1,
        "background_job_id": 1,
        "item_no": 1,
        "object_type": 1,
        "object_id": 1,
        "status": 1,
        "result_code": 1,
        "result_summary": 1,
        "result_object_type": 1,
        "result_object_id": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, BackgroundJobFilter, BulkSelectionSnapshotFilter, QueryFilter};
    use entities::bulk_job::{JobStatus, JobType, SelectionStatus, SelectionType};
    use mongodb::bson::doc;

    #[test]
    fn snapshot_filter_applies_type_status_and_creator() {
        let filter = BulkSelectionSnapshotFilter {
            selection_type: Some(SelectionType::Export),
            status: Some(SelectionStatus::Confirmed),
            created_by: Some("admin-1".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("selection_type").unwrap(), "export");
        assert_eq!(document.get_str("status").unwrap(), "confirmed");
        assert_eq!(document.get_str("created_by").unwrap(), "admin-1");
    }

    #[test]
    fn background_job_filter_applies_no_regex_and_status() {
        let filter = BackgroundJobFilter {
            job_no: Some("job-001".to_string()),
            job_type: Some(JobType::Import),
            status: Some(JobStatus::Running),
            requested_by: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        let no = document.get_document("job_no").unwrap();
        assert_eq!(no.get_str("$regex").unwrap(), r"job\-001");
        assert_eq!(document.get_str("job_type").unwrap(), "import");
        assert_eq!(document.get_str("status").unwrap(), "running");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1, "id": -1 });
        assert_eq!(
            sort_doc(Some("updated_at"), true),
            doc! { "updated_at": 1, "id": 1 }
        );
        assert_eq!(
            sort_doc(Some("job_no"), false),
            doc! { "created_at": -1, "id": -1 },
            "白名单外字段回落默认排序"
        );
    }
}
