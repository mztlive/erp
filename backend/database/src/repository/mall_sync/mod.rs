//! 域 D23 `mall_sync` 仓储：mall_sales_sync_job、mall_sales_sync_cursor、
//! mall_sales_order_snapshot、mall_sales_reconciliation_job(+_item)、master_mapping_task。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充
//! 域特有查询与跨集合多步骤写入入口。集合名常量统一从 `MallSyncExt` 关联常量取。
//!
//! 事实/历史类集合（`mall_sales_order_snapshot`、`mall_sales_reconciliation_item`
//! 等）不设业务软删除（数据模型 §4.5/§6.13：历史批次和处理证据永久可查）；
//! 同步水位游标每个来源商城一行，水位只前进（§6.13），`advance` 提供
//! 实体单调推进 + 版本 CAS 的原子写入口。
//!
//! 筛选/行类型定义在本文件，经 `MallSyncExt` 的关联类型对外暴露。
//!
//! - [`snapshot_ingest`]：快照落盘 exact/latest 批量事实与单调水位、批量插入（INT-R16）。

mod snapshot_ingest;

#[allow(unused_imports)]
pub use snapshot_ingest::SnapshotIngestScope;

use entities::catalog::{EnableStatus, VoucherCategoryProfileRevision};
use entities::common::time::Instant;
use entities::contract::{Contract, ContractStatus};
use entities::mall_sync::{
    ExternalOrderKey, MallSalesOrderSnapshot, MallSalesReconciliationItem, MallSalesReconciliationJob,
    MallSalesSyncCursor, MallSalesSyncJob, MallSalesSyncJobStatus, MallSalesSyncJobType,
    MallSnapshotReapplyOperation, MallSyncTriggerSource, MappingTaskStatus, MappingTaskType,
    MasterMappingTask, ReconciliationDifferenceType, ReconciliationItemStatus, ReconciliationJobStatus,
    SnapshotMappingStatus,
};
use entities::receivable::ReceivableAccount;
use entities::source_registry::{ExternalIdentityTarget, TargetStatus};
use entities::work_item::{WorkItem, WorkItemType};
use entities::AuditLog;
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::MallSyncExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};
/// `mall_sales_reconciliation_job` 集合名（单一来源：`MallSyncExt` 关联常量）。
const MALL_SALES_RECONCILIATION_JOBS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_RECONCILIATION_JOBS;
/// `mall_sales_reconciliation_item` 集合名（单一来源：`MallSyncExt` 关联常量）。
const MALL_SALES_RECONCILIATION_ITEMS: &str =
    <mongodb::Database as MallSyncExt>::MALL_SALES_RECONCILIATION_ITEMS;

/// 同步作业列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesSyncJobRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: entities::ids::SourceSystemId,
    /// 作业类型。
    pub job_type: MallSalesSyncJobType,
    /// 本次查询时间边界起。
    pub range_start: Option<Instant>,
    /// 本次查询时间边界止。
    pub range_end: Option<Instant>,
    /// 按单补拉的原来源销售单号。
    pub external_order_no: Option<String>,
    /// 触发来源。
    pub trigger_source: MallSyncTriggerSource,
    /// 人工触发理由。
    pub trigger_reason: Option<String>,
    /// 人工触发人。
    pub triggered_by: Option<String>,
    /// 失败重试沿用的原作业。
    pub source_job_id: Option<entities::ids::MallSalesSyncJobId>,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 作业状态。
    pub status: MallSalesSyncJobStatus,
    /// 处理页数。
    pub page_count: u64,
    /// 处理条数。
    pub item_count: u64,
    /// 错误条数。
    pub error_count: u64,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 同步作业列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallSalesSyncJobFilter {
    /// 来源商城；`None` 表示不筛选。
    pub source_system_id: Option<entities::ids::SourceSystemId>,
    /// 作业类型；`None` 表示不筛选。
    pub job_type: Option<MallSalesSyncJobType>,
    /// 作业状态；`None` 表示不筛选。
    pub status: Option<MallSalesSyncJobStatus>,
    /// 任务开始时间起（含）；`None` 表示不限。
    pub started_at_from: Option<Instant>,
    /// 任务开始时间止（含）；`None` 表示不限。
    pub started_at_to: Option<Instant>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`started_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallSalesSyncJobFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(job_type) = self.job_type {
            filter.insert("job_type", job_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let (Some(from), Some(to)) = (self.started_at_from, self.started_at_to) {
            filter.insert(
                "started_at",
                doc! { "$gte": from.unix_secs(), "$lte": to.unix_secs() },
            );
        }
        filter
    }
}

impl Pagination for MallSalesSyncJobFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallSalesSyncJob> {
    /// 分页检索同步作业列表（投影查询）。
    ///
    /// 只返回 [`MallSalesSyncJobRow`] 所需的列表字段，不加载整文档；
    /// 排序字段经白名单映射，非法字段回退默认 `created_at`。
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
    pub async fn search_mall_sales_sync_jobs(
        &self,
        filter: &MallSalesSyncJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallSalesSyncJobRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(mall_sales_sync_job_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallSalesSyncJobRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查找指定来源商城运行中的增量任务。
    ///
    /// 数据模型 §6.13：同一来源商城只允许一个有效增量任务推进水位；
    /// 并发推进由 `MallSalesSyncCursor` 的版本 CAS 兜底，本方法供
    /// Service 在创建增量任务前做存在性判定。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回运行中的增量任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_running_incremental_by_source(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesSyncJob>> {
        self.find_one(
            doc! {
                "source_system_id": source_system_id.to_string(),
                "job_type": MallSalesSyncJobType::Incremental.as_str(),
                "status": MallSalesSyncJobStatus::Running.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, MallSalesSyncCursor> {
    /// 按来源商城查找同步水位游标（单行语义）。
    ///
    /// 唯一性由 `uk_mall_sales_sync_cursors_source` 唯一索引保证
    /// （§6.13：每个来源商城一个当前水位）。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的水位游标；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_source(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesSyncCursor>> {
        self.find_one(
            doc! {
                "source_system_id": source_system_id.to_string(),
            },
            executor,
        )
        .await
    }

    /// 单调前移同步水位（实体校验 + 版本 CAS 写）。
    ///
    /// 先经 [`MallSalesSyncCursor::move_forward`] 执行实体不变量（水位不得
    /// 回退，§6.13），再按 `id + version` 条件更新持久化水位；并发推进方
    /// 以陈旧 version 收到 [`crate::Error::OptimisticLockingError`]。
    /// 实体校验失败（新水位早于当前水位）同样归类为
    /// `OptimisticLockingError`：水位被并发任务或回放数据提前推进，属于
    /// 写入方无法按当前实体状态生效的冲突语义，P3 统一映射为 409。
    /// 本方法是单集合「读后写」原子入口，无跨集合原子性要求，可传
    /// `NoTransaction`；多步骤场景（快照落盘 + 前移水位）必须由 Service
    /// 放在同一事务内调用本方法。
    ///
    /// # 参数
    /// * `cursor` - 水位游标（内存实体，成功后版本递增）
    /// * `new_water` - 新高水位（已安全处理的商城更新时间）
    /// * `success_job_id` - 本次成功的同步任务
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 错误
    /// 新水位回退时返回实体校验错误（归类 `OptimisticLockingError`）；
    /// 版本或删除状态不匹配时返回 `OptimisticLockingError`。
    pub async fn advance(
        &self,
        cursor: &mut MallSalesSyncCursor,
        new_water: Instant,
        success_job_id: entities::ids::MallSalesSyncJobId,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        cursor
            .move_forward(new_water, success_job_id)
            .map_err(|_| crate::Error::OptimisticLockingError)?;
        self.update(cursor, executor).await
    }
}

/// 商城销售单快照列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesOrderSnapshotRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: entities::ids::SourceSystemId,
    /// 一期来源单号原值。
    pub external_order_no: String,
    /// 二进制比较键。
    pub external_order_key: ExternalOrderKey,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// ERP 实际观察时间。
    pub observed_at: Instant,
    /// 映射状态。
    pub mapping_status: SnapshotMappingStatus,
    /// 成功形成的销售版本。
    pub applied_sales_order_revision_id: Option<entities::ids::SalesOrderRevisionId>,
    /// 来源任务。
    pub sync_job_id: entities::ids::MallSalesSyncJobId,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商城销售单快照列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallSalesOrderSnapshotFilter {
    /// 来源商城；`None` 表示不筛选。
    pub source_system_id: Option<entities::ids::SourceSystemId>,
    /// 映射状态；`None` 表示不筛选。
    pub mapping_status: Option<SnapshotMappingStatus>,
    /// 观察时间起（含）；`None` 表示不限。
    pub observed_at_from: Option<Instant>,
    /// 观察时间止（含）；`None` 表示不限。
    pub observed_at_to: Option<Instant>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`source_updated_at`、`observed_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallSalesOrderSnapshotFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(mapping_status) = self.mapping_status {
            filter.insert("mapping_status", mapping_status.as_str());
        }
        if let (Some(from), Some(to)) = (self.observed_at_from, self.observed_at_to) {
            filter.insert(
                "observed_at",
                doc! { "$gte": from.unix_secs(), "$lte": to.unix_secs() },
            );
        }
        filter
    }
}

impl Pagination for MallSalesOrderSnapshotFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallSalesOrderSnapshot> {
    /// 按事实键精确查找快照（去重幂等判定）。
    ///
    /// 唯一性由 `uk_mall_sales_order_snapshots_fact_key` 唯一索引保证
    /// （§6.13/P2 §5：`business_fact_key` 即
    /// `(source_system_id, external_order_key, source_updated_at)`，
    /// 重复推送时保留最新快照），服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `external_order_key` - 来源单二进制比较键
    /// * `source_updated_at` - 商城更新时间
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的快照；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_fact_key(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        external_order_key: &ExternalOrderKey,
        source_updated_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesOrderSnapshot>> {
        self.find_one(
            doc! {
                "source_system_id": source_system_id.to_string(),
                "external_order_key": external_order_key.to_bson_binary(),
                "source_updated_at": source_updated_at.unix_secs(),
            },
            executor,
        )
        .await
    }

    /// 查找指定来源单的最新快照（按商城更新时间倒序取首条）。
    ///
    /// 用于「同一来源单收到更早 `source_updated_at` 的快照直接丢弃」
    /// （§6.13）的判定。
    ///
    /// # 参数
    /// * `source_system_id` - 来源商城
    /// * `external_order_key` - 来源单二进制比较键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新快照；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_latest_by_order(
        &self,
        source_system_id: &entities::ids::SourceSystemId,
        external_order_key: &ExternalOrderKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesOrderSnapshot>> {
        let filter = doc! {
            "source_system_id": source_system_id.to_string(),
            "external_order_key": external_order_key.to_bson_binary(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let options = FindOptions::builder()
            .sort(doc! { "source_updated_at": -1 })
            .limit(1)
            .build();
        let mut items = mongo_ops::find_many(&self.collection(), filter, options, executor).await?;
        Ok(items.pop())
    }

    /// 分页检索快照列表（投影查询）。
    ///
    /// 只返回 [`MallSalesOrderSnapshotRow`] 所需的列表字段；规范化快照归档
    /// （`normalized_snapshot`，最大 64KB）与原始报文引用不进入列表投影。
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
    pub async fn search_mall_sales_order_snapshots(
        &self,
        filter: &MallSalesOrderSnapshotFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallSalesOrderSnapshotRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(mall_sales_order_snapshot_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallSalesOrderSnapshotRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 核对作业列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesReconciliationJobRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: entities::ids::SourceSystemId,
    /// 核对批次号。
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 商城清单数量。
    pub source_count: u64,
    /// ERP 数量。
    pub erp_count: u64,
    /// 差异数量。
    pub difference_count: u64,
    /// 作业状态。
    pub status: ReconciliationJobStatus,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 核对作业列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallSalesReconciliationJobFilter {
    /// 来源商城；`None` 表示不筛选。
    pub source_system_id: Option<entities::ids::SourceSystemId>,
    /// 作业状态；`None` 表示不筛选。
    pub status: Option<ReconciliationJobStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`source_list_as_of`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallSalesReconciliationJobFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_system_id) = &self.source_system_id {
            filter.insert("source_system_id", source_system_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for MallSalesReconciliationJobFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallSalesReconciliationJob> {
    /// 分页检索核对作业列表（投影查询）。
    ///
    /// 只返回 [`MallSalesReconciliationJobRow`] 所需的列表字段，不加载整文档。
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
    pub async fn search_mall_sales_reconciliation_jobs(
        &self,
        filter: &MallSalesReconciliationJobFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallSalesReconciliationJobRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(mall_sales_reconciliation_job_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<MallSalesReconciliationJobRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按核对批次号精确查找核对作业。
    ///
    /// 唯一性由 `uk_mall_sales_reconciliation_jobs_job_no` 唯一索引保证
    /// （§6.13：`job_no` 唯一），用于重跑与幂等判定。
    ///
    /// # 参数
    /// * `job_no` - 核对批次号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除核对作业；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_job_no(
        &self,
        job_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesReconciliationJob>> {
        self.find_one(
            doc! {
                "job_no": job_no,
            },
            executor,
        )
        .await
    }
}

/// 核对差异明细列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallSalesReconciliationItemRow {
    /// 实体主键。
    pub id: String,
    /// 所属核对作业。
    pub reconciliation_job_id: entities::ids::MallSalesReconciliationJobId,
    /// 来源单号。
    pub external_order_no: String,
    /// 二进制比较键。
    pub external_order_key: ExternalOrderKey,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
    /// 明细状态。
    pub status: ReconciliationItemStatus,
    /// 按单号补拉任务。
    pub single_order_sync_job_id: Option<entities::ids::MallSalesSyncJobId>,
    /// 人工处理结论。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 核对差异明细列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallSalesReconciliationItemFilter {
    /// 所属核对作业；`None` 表示不筛选。
    pub reconciliation_job_id: Option<entities::ids::MallSalesReconciliationJobId>,
    /// 明细状态；`None` 表示不筛选。
    pub status: Option<ReconciliationItemStatus>,
    /// 差异类型；`None` 表示不筛选。
    pub difference_type: Option<ReconciliationDifferenceType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`source_updated_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallSalesReconciliationItemFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(reconciliation_job_id) = &self.reconciliation_job_id {
            filter.insert("reconciliation_job_id", reconciliation_job_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(difference_type) = self.difference_type {
            filter.insert("difference_type", difference_type.as_str());
        }
        filter
    }
}

impl Pagination for MallSalesReconciliationItemFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallSalesReconciliationItem> {
    /// 分页检索核对差异明细列表（投影查询）。
    ///
    /// 只返回 [`MallSalesReconciliationItemRow`] 所需的列表字段，不加载整文档。
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
    pub async fn search_mall_sales_reconciliation_items(
        &self,
        filter: &MallSalesReconciliationItemFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallSalesReconciliationItemRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(mall_sales_reconciliation_item_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<MallSalesReconciliationItemRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「核对作业 + 来源单比较键」精确查找差异明细。
    ///
    /// 唯一性由 `uk_mall_sales_reconciliation_items_job_key` 唯一索引保证
    /// （§6.13：`(reconciliation_job_id, external_order_key)` 唯一）。
    ///
    /// # 参数
    /// * `reconciliation_job_id` - 所属核对作业
    /// * `external_order_key` - 来源单二进制比较键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的差异明细；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_job_and_key(
        &self,
        reconciliation_job_id: &entities::ids::MallSalesReconciliationJobId,
        external_order_key: &ExternalOrderKey,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSalesReconciliationItem>> {
        self.find_one(
            doc! {
                "reconciliation_job_id": reconciliation_job_id.to_string(),
                "external_order_key": external_order_key.to_bson_binary(),
            },
            executor,
        )
        .await
    }
}

/// 映射任务列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MasterMappingTaskRow {
    /// 实体主键。
    pub id: String,
    /// 待处理快照。
    pub source_snapshot_id: entities::ids::MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 任务状态。
    pub status: MappingTaskStatus,
    /// 业务责任角色；未形成唯一责任路由时为空。
    pub owner_role: Option<String>,
    /// 业务责任用户 ID。
    pub owner_user_id: Option<String>,
    /// 处理结论。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 映射任务列表筛选条件。
#[derive(Debug, Clone)]
pub struct MasterMappingTaskFilter {
    /// 待处理快照；`None` 表示不筛选。
    pub source_snapshot_id: Option<entities::ids::MallSalesOrderSnapshotId>,
    /// 映射类型；`None` 表示不筛选。
    pub mapping_type: Option<MappingTaskType>,
    /// 任务状态；`None` 表示不筛选。
    pub status: Option<MappingTaskStatus>,
    /// 责任角色；`None` 表示不筛选。
    pub owner_role: Option<String>,
    /// 责任用户 ID；`None` 表示不筛选。
    pub owner_user_id: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`resolved_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MasterMappingTaskFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_snapshot_id) = &self.source_snapshot_id {
            filter.insert("source_snapshot_id", source_snapshot_id.to_string());
        }
        if let Some(mapping_type) = self.mapping_type {
            filter.insert("mapping_type", mapping_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(owner_role) = &self.owner_role {
            filter.insert("owner_role", owner_role);
        }
        if let Some(owner_user_id) = &self.owner_user_id {
            filter.insert("owner_user_id", owner_user_id);
        }
        filter
    }
}

impl Pagination for MasterMappingTaskFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MasterMappingTask> {
    /// 分页检索映射任务列表（投影查询）。
    ///
    /// 只返回 [`MasterMappingTaskRow`] 所需的列表字段，不加载整文档。
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
    pub async fn search_master_mapping_tasks(
        &self,
        filter: &MasterMappingTaskFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MasterMappingTaskRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(master_mapping_task_projection())
            .build();
        let collection = self.collection().clone_with_type::<MasterMappingTaskRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查找指定快照 + 映射类型的进行中任务。
    ///
    /// 「同一快照、映射类型只允许一个进行中任务」（§6.13）由部分唯一索引
    /// `uk_master_mapping_tasks_snapshot_type_pending` 保证，本方法用于
    /// Service 创建任务前的存在性判定与领办查询。
    ///
    /// # 参数
    /// * `source_snapshot_id` - 待处理快照
    /// * `mapping_type` - 映射类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的待处理任务；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_pending_by_snapshot_and_type(
        &self,
        source_snapshot_id: &entities::ids::MallSalesOrderSnapshotId,
        mapping_type: MappingTaskType,
        executor: &mut dyn Executor,
    ) -> Result<Option<MasterMappingTask>> {
        self.find_one(
            doc! {
                "source_snapshot_id": source_snapshot_id.to_string(),
                "mapping_type": mapping_type.as_str(),
                "status": MappingTaskStatus::Pending.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, WorkItem> {
    /// 查询映射任务关联的正式责任任务，按创建时间稳定排序。
    ///
    /// # 参数
    /// * `mapping_task_id` - 映射任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的正式任务；调用方必须校验责任事实唯一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_for_master_mapping_task(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "work_item_type": WorkItemType::BusinessException.as_str(),
                "business_object_type": "MASTER_MAPPING_TASK",
                "business_object_id": mapping_task_id,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ExternalIdentityTarget> {
    /// 查询外部身份映射的全部目标历史，最新有效期优先。
    ///
    /// # 参数
    /// * `mapping_id` - 外部身份映射 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部目标历史，按 `valid_from` 降序、ID 升序稳定排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_for_external_identity_map(
        &self,
        mapping_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityTarget>> {
        self.find_many_sorted(
            doc! { "external_identity_map_id": mapping_id },
            doc! { "valid_from": -1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询外部身份映射当前有效目标，按生效时间稳定排序。
    ///
    /// # 参数
    /// * `mapping_id` - 外部身份映射 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回状态为 `Active` 的目标，按 `valid_from` 与 ID 升序排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_active_for_external_identity_map(
        &self,
        mapping_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ExternalIdentityTarget>> {
        self.find_many_sorted(
            doc! {
                "external_identity_map_id": mapping_id,
                "status": TargetStatus::Active.as_str(),
            },
            doc! { "valid_from": 1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, AuditLog> {
    /// 查询映射任务的不可变审计时间线。
    ///
    /// # 参数
    /// * `mapping_task_id` - 映射任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回资源匹配的审计记录，按创建时间与 ID 升序排列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_master_mapping_task_history(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<AuditLog>> {
        self.find_many_sorted(
            doc! {
                "resource_type": "MASTER_MAPPING_TASK",
                "resource_id": mapping_task_id,
            },
            doc! { "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ReceivableAccount> {
    /// 按来源销售版本查找应收结果。
    ///
    /// # 参数
    /// * `revision_id` - 来源销售单修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回引用该销售版本的应收账户；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_source_sales_order_revision(
        &self,
        revision_id: &entities::ids::SalesOrderRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ReceivableAccount>> {
        self.find_one(
            doc! { "source_sales_order_revision_id": revision_id.to_string() },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, Contract> {
    /// 查找当前客户范围内指向指定结算主体的生效合同。
    ///
    /// # 参数
    /// * `customer_ids` - 当前操作人可参与的客户 ID 集合
    /// * `settlement_party_id` - 目标结算主体 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回任一客户范围内的生效合同；客户集合为空或无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_effective_for_settlement_party(
        &self,
        customer_ids: &[String],
        settlement_party_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Contract>> {
        if customer_ids.is_empty() {
            return Ok(None);
        }
        self.find_one(
            doc! {
                "customer_id": { "$in": customer_ids },
                "settlement_party_id": settlement_party_id,
                "status": ContractStatus::Effective.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, VoucherCategoryProfileRevision> {
    /// 查找 SKU 当前启用的卡券类目扩展修订。
    ///
    /// # 参数
    /// * `sku_id` - 目标 SKU ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回状态为 `Active` 的卡券类目扩展修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_active_by_sku(
        &self,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<VoucherCategoryProfileRevision>> {
        self.find_one(
            doc! {
                "sku_id": sku_id,
                "status": EnableStatus::Active.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, MallSnapshotReapplyOperation> {
    /// 按映射任务与幂等摘要查询重新归集操作。
    pub async fn find_reapply_by_idempotency(
        &self,
        mapping_task_id: &str,
        idempotency_key_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSnapshotReapplyOperation>> {
        self.find_one(
            doc! {
                "mapping_task_id": mapping_task_id,
                "idempotency_key_hash": idempotency_key_hash,
            },
            executor,
        )
        .await
    }

    /// 查询映射任务最近一次重新归集操作。
    pub async fn latest_reapply_for_task(
        &self,
        mapping_task_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallSnapshotReapplyOperation>> {
        let operations = self
            .find_many_sorted(
                doc! { "mapping_task_id": mapping_task_id },
                doc! { "last_updated_at": -1, "created_at": -1 },
                executor,
            )
            .await?;
        Ok(operations.into_iter().next())
    }
}

/// D23 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `MallSyncExt::mall_sync()` 访问。
pub struct MallSyncRepository<'a> {
    db: &'a Database,
}

impl<'a> MallSyncRepository<'a> {
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

    /// 创建核对作业并写入全部差异明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `mall_sales_reconciliation_job` 与
    /// `mall_sales_reconciliation_items`，保证「核对批次 + 差异明细」
    /// 原子可见（§6.13：核对只生成差异和任务，历史批次和处理证据永久可查）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，明细唯一索引冲突会留下只有作业没有明细的
    /// 半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `job` - 待写入的核对作业
    /// * `items` - 待写入的差异明细（必须属于 `job`）
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当明细唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service
    /// 映射为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_reconciliation_job_with_items(
        &self,
        job: &MallSalesReconciliationJob,
        items: &[MallSalesReconciliationItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<MallSalesReconciliationJob>(MALL_SALES_RECONCILIATION_JOBS),
            job,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<MallSalesReconciliationItem>(MALL_SALES_RECONCILIATION_ITEMS),
            items.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单映射，非法字段回退 `created_at`）。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 或不在白名单时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = match sort_by {
        Some("started_at") => "started_at",
        Some("source_updated_at") => "source_updated_at",
        Some("observed_at") => "observed_at",
        Some("source_list_as_of") => "source_list_as_of",
        Some("resolved_at") => "resolved_at",
        _ => "created_at",
    };
    doc! { field: direction }
}

/// 同步作业列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn mall_sales_sync_job_projection() -> Document {
    doc! {
        "id": 1,
        "source_system_id": 1,
        "job_type": 1,
        "range_start": 1,
        "range_end": 1,
        "external_order_no": 1,
        "trigger_source": 1,
        "trigger_reason": 1,
        "triggered_by": 1,
        "source_job_id": 1,
        "started_at": 1,
        "finished_at": 1,
        "status": 1,
        "page_count": 1,
        "item_count": 1,
        "error_count": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 快照列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn mall_sales_order_snapshot_projection() -> Document {
    doc! {
        "id": 1,
        "source_system_id": 1,
        "external_order_no": 1,
        "external_order_key": 1,
        "source_updated_at": 1,
        "content_hash": 1,
        "source_status_code": 1,
        "observed_at": 1,
        "mapping_status": 1,
        "applied_sales_order_revision_id": 1,
        "sync_job_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 核对作业列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn mall_sales_reconciliation_job_projection() -> Document {
    doc! {
        "id": 1,
        "source_system_id": 1,
        "job_no": 1,
        "source_list_as_of": 1,
        "source_count": 1,
        "erp_count": 1,
        "difference_count": 1,
        "status": 1,
        "started_at": 1,
        "finished_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 核对差异明细列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn mall_sales_reconciliation_item_projection() -> Document {
    doc! {
        "id": 1,
        "reconciliation_job_id": 1,
        "external_order_no": 1,
        "external_order_key": 1,
        "source_status_code": 1,
        "source_updated_at": 1,
        "difference_type": 1,
        "status": 1,
        "single_order_sync_job_id": 1,
        "resolution": 1,
        "resolved_by": 1,
        "resolved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 映射任务列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn master_mapping_task_projection() -> Document {
    doc! {
        "id": 1,
        "source_snapshot_id": 1,
        "mapping_type": 1,
        "status": 1,
        "owner_role": 1,
        "owner_user_id": 1,
        "resolution": 1,
        "resolved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, MallSalesSyncJobFilter, QueryFilter};
    use entities::common::time::Instant;
    use mongodb::bson::doc;

    #[test]
    fn sync_job_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallSalesSyncJobFilter {
            source_system_id: Some(entities::ids::SourceSystemId::new("sys-mall")),
            job_type: Some(entities::mall_sync::MallSalesSyncJobType::Incremental),
            status: Some(entities::mall_sync::MallSalesSyncJobStatus::Running),
            started_at_from: Some(Instant::from_unix_secs(1_700_000_000)),
            started_at_to: Some(Instant::from_unix_secs(1_700_000_100)),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("job_type").unwrap(), "incremental");
        assert_eq!(document.get_str("status").unwrap(), "running");
        let range = document.get_document("started_at").unwrap();
        assert_eq!(range.get_i64("$gte").unwrap(), 1_700_000_000);
        assert_eq!(range.get_i64("$lte").unwrap(), 1_700_000_100);
    }

    #[test]
    fn sort_doc_whitelists_known_fields_and_defaults_otherwise() {
        assert_eq!(
            sort_doc(Some("source_updated_at"), true),
            doc! { "source_updated_at": 1 }
        );
        assert_eq!(sort_doc(Some("resolved_at"), false), doc! { "resolved_at": -1 });
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("id"), false), doc! { "created_at": -1 });
    }
}
