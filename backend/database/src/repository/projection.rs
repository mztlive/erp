//! 域 D27 `projection` 仓储：sales_order_projection(+_revision、_delivery)。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `ProjectionExt` 关联常量导入。
//!
//! 投影版本为不可变执行指令版本（§4.4），**只追加不覆盖，不提供任何软删除
//! 方法**；投影稳定身份按 `(sales_order_id, target_mall_id)` 唯一（§6.16）。
//!
//! 筛选/行类型定义在本文件，经 `ProjectionExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::time::Instant;
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, SalesOrderId, SalesOrderProjectionRevisionId, SourceSystemId,
    WorkItemId,
};
use entities::integration_ops::ErrorClass;
use entities::money::Amount;
use entities::projection::{
    CardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionDelivery,
    SalesOrderProjectionRevision,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::ProjectionExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sales_order_projection` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTIONS: &str = <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTIONS;
/// `sales_order_projection_revision` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTION_REVISIONS: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_REVISIONS;
/// `sales_order_projection_delivery` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTION_DELIVERIES: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_DELIVERIES;

/// 执行投影投递失败或结果未知的原子落库参数。
#[derive(Debug, Clone, Copy)]
pub struct ProjectionDeliveryFailure<'a> {
    /// 目标状态。
    pub status: ProjectionDeliveryStatus,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 稳定错误码。
    pub error_code: &'a str,
    /// 可展示的错误摘要。
    pub error_summary: &'a str,
    /// 事实发生时间。
    pub at: Instant,
}

/// 执行投影投递升级 W29 的原子落库参数。
#[derive(Debug, Clone, Copy)]
pub struct ProjectionDeliveryEscalation<'a> {
    /// 原投递错误分类。
    pub error_class: ErrorClass,
    /// 原投递稳定错误码。
    pub error_code: &'a str,
    /// 原投递错误摘要。
    pub error_summary: &'a str,
    /// 正式 W29 错误对象。
    pub error_task_id: &'a IntegrationErrorTaskId,
    /// 与错误对象配对的正式待办。
    pub work_item_id: &'a WorkItemId,
    /// 升级发生时间。
    pub at: Instant,
}

/// 投影列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRow {
    /// 实体主键。
    pub id: String,
    /// 卡券销售单。
    pub sales_order_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 商城最后确认版本。
    pub current_acked_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderProjectionFilter {
    /// 卡券销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderProjectionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        filter
    }
}

impl Pagination for SalesOrderProjectionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderProjection> {
    /// 分页检索销售单执行投影列表（投影查询）。
    ///
    /// 只返回 [`SalesOrderProjectionRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛，禁止透传任意字段名。
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
    pub async fn search_sales_order_projections(
        &self,
        filter: &SalesOrderProjectionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderProjectionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_projection_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesOrderProjectionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 投影修订列表投影行（Decimal128 金额原样投影，不做舍入换算）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属投影稳定身份。
    pub projection_id: String,
    /// 修订序号（同一投影内从 1 递增）。
    pub revision_no: u32,
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本。
    pub sales_order_revision_id: String,
    /// 商城客户标识。
    pub customer_external_identity: String,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
    /// ERP 生效时间。
    pub effective_at: i64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影修订号最小投影行。
#[derive(Debug, Deserialize)]
struct SalesOrderProjectionRevisionNoRow {
    /// 同一投影内修订序号。
    revision_no: u32,
}

impl<'a> Repository<'a, SalesOrderProjectionRevision> {
    /// 按「投影 + 修订序号」查找唯一投影修订。
    ///
    /// 唯一性由 `uk_sales_order_projection_revisions_projection_revision` 唯一
    /// 索引保证（数据模型 §6.16）；修订不可变，只读入口。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的投影修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_revision_by_no(
        &self,
        projection_id: &entities::ids::SalesOrderProjectionId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionRevision>> {
        self.find_one(
            doc! {
                "projection_id": projection_id.to_string(),
                "revision_no": revision_no,
            },
            executor,
        )
        .await
    }

    /// 列出指定投影的全部分区版本（投影查询，修订号降序）。
    ///
    /// 只返回 [`SalesOrderProjectionRevisionRow`] 所需的列表字段，不加载整文档；
    /// 修订为不可变版本（§4.4），本方法只读，不提供任何删除入口。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `revision_no` 降序的投影行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revisions_by_projection(
        &self,
        projection_id: &entities::ids::SalesOrderProjectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderProjectionRevisionRow>> {
        let options = FindOptions::builder()
            .sort(doc! { "revision_no": -1 })
            .projection(sales_order_projection_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionRevisionRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "projection_id": projection_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }

    /// 读取指定执行投影的历史最大修订序号。
    ///
    /// 查询只读取 `revision_no` 并限制一条；修订号分配与溢出校验由实体负责。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回历史最大修订序号；没有历史修订时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或投影反序列化失败时返回错误。
    pub async fn latest_revision_no(
        &self,
        projection_id: &entities::ids::SalesOrderProjectionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let rows = mongo_ops::find_many(
            &self
                .collection()
                .clone_with_type::<SalesOrderProjectionRevisionNoRow>(),
            latest_projection_revision_filter(projection_id),
            latest_projection_revision_options(),
            executor,
        )
        .await?;
        Ok(projection_revision_no_from_rows(rows))
    }

    /// 批量读取多个投影的全部修订，按投影 ID、修订号降序返回。
    pub async fn list_revisions_by_projections(
        &self,
        projection_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderProjectionRevisionRow>> {
        if projection_ids.is_empty() {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder()
            .sort(doc! { "projection_id": 1, "revision_no": -1 })
            .projection(sales_order_projection_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionRevisionRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "projection_id": { "$in": projection_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

/// 投影下发列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 待下发投影版本。
    pub projection_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近发送时间。
    pub last_attempt_at: Option<i64>,
    /// 下次受控处理时间。
    pub next_attempt_at: Option<i64>,
    /// 商城确认时间。
    pub mall_ack_at: Option<i64>,
    /// 商城执行基线。
    pub mall_execution_baseline: Option<String>,
    /// 稳定错误分类。
    pub error_class: Option<ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 脱敏错误摘要。
    pub error_summary: Option<String>,
    /// W29 错误对象。
    pub error_task_id: Option<String>,
    /// W29 正式待办。
    pub work_item_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影下发列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderProjectionDeliveryFilter {
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 下发状态；`None` 表示不筛选。
    pub status: Option<ProjectionDeliveryStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderProjectionDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SalesOrderProjectionDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderProjectionDelivery> {
    /// 分页检索投影下发记录（投影查询）。
    ///
    /// 只返回 [`SalesOrderProjectionDeliveryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛。下发状态查询由
    /// `idx_sales_order_projection_deliveries_status` 索引支撑（§6.16）。
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
    pub async fn search_sales_order_projection_deliveries(
        &self,
        filter: &SalesOrderProjectionDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderProjectionDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_projection_delivery_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量读取指定投影修订的投递列表。
    pub async fn list_deliveries_by_revisions(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderProjectionDeliveryRow>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1 })
            .projection(sales_order_projection_delivery_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionDeliveryRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "projection_revision_id": { "$in": revision_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }

    /// 按「投影修订 + 目标商城」查找唯一下发记录。
    ///
    /// # 参数
    /// * `revision_id` - 待下发投影版本
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的下发记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_delivery_by_revision_and_mall(
        &self,
        revision_id: &SalesOrderProjectionRevisionId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        self.find_one(
            doc! {
                "projection_revision_id": revision_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }

    /// 列出到期且可由受控 worker 处理的投递。
    ///
    /// 只返回 `pending_send`，或已到 `next_attempt_at` 的 `retrying` 记录；结果未知
    /// 与发送中记录不得在后台被盲目重放。
    pub async fn list_processable_deliveries(
        &self,
        at: Instant,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderProjectionDelivery>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1, "id": 1 })
            .limit(i64::from(limit))
            .build();
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$or": [
                    { "status": ProjectionDeliveryStatus::PendingSend.as_str() },
                    {
                        "status": ProjectionDeliveryStatus::Retrying.as_str(),
                        "next_attempt_at": { "$lte": at.unix_secs() },
                    },
                ],
            },
            options,
            executor,
        )
        .await
    }

    /// 以单文档 CAS 取得一条待发送或到期重试投递。
    ///
    /// 命中时保持原投递 ID 和消息键，原子写入消息信封、发送次数、最近发送时间
    /// 及 `sending` 状态。未命中返回当前事实，由 Service 区分并发、终态与版本冲突。
    pub async fn claim_for_send(
        &self,
        id: &str,
        expected_version: u64,
        inbox_message_id: &InboxMessageId,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        let expected_version = metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            claim_delivery_filter(id, expected_version, at),
            claim_delivery_pipeline(inbox_message_id, at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 将发送中或结果未知投递落为商城已确认。
    pub async fn confirm_delivery(
        &self,
        id: &str,
        expected_version: u64,
        mall_execution_baseline: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        let expected_version = metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            result_delivery_filter(id, expected_version),
            confirmed_delivery_pipeline(mall_execution_baseline, at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 记录明确失败或结果未知，保留原投递和消息身份。
    pub async fn fail_delivery(
        &self,
        id: &str,
        expected_version: u64,
        failure: ProjectionDeliveryFailure<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        let expected_version = metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            result_delivery_filter(id, expected_version),
            failed_delivery_pipeline(
                failure.status,
                failure.error_class,
                failure.error_code,
                failure.error_summary,
                failure.at,
            ),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 安排沿原消息键重试。
    pub async fn schedule_retry(
        &self,
        id: &str,
        expected_version: u64,
        at: Instant,
        next_attempt_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        let expected_version = metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            retry_delivery_filter(id, expected_version),
            retry_delivery_pipeline(at, next_attempt_at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 关联 W29 错误对象和正式待办并转人工。
    pub async fn escalate_delivery(
        &self,
        id: &str,
        expected_version: u64,
        escalation: ProjectionDeliveryEscalation<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        let expected_version = metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            escalation_delivery_filter(id, expected_version),
            escalation_delivery_pipeline(
                escalation.error_class,
                escalation.error_code,
                escalation.error_summary,
                escalation.error_task_id,
                escalation.work_item_id,
                escalation.at,
            ),
            executor,
        )
        .await
    }
}

fn metadata_version(version: u64) -> Result<i64> {
    i64::try_from(version).map_err(|_| crate::Error::EntityMetadataOutOfRange("version"))
}

fn claim_delivery_filter(id: &str, expected_version: i64, at: Instant) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$or": [
            { "status": ProjectionDeliveryStatus::PendingSend.as_str() },
            {
                "status": ProjectionDeliveryStatus::Retrying.as_str(),
                "next_attempt_at": { "$lte": at.unix_secs() },
            },
        ],
    }
}

fn claim_delivery_pipeline(inbox_message_id: &InboxMessageId, at: Instant) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "status": ProjectionDeliveryStatus::Sending.as_str(),
            "attempt_count": { "$add": ["$attempt_count", 1_i64] },
            "last_attempt_at": timestamp,
            "next_attempt_at": Bson::Null,
            "inbox_message_id": {
                "$ifNull": ["$inbox_message_id", inbox_message_id.to_string()]
            },
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn result_delivery_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": {
            "$in": [
                ProjectionDeliveryStatus::Sending.as_str(),
                ProjectionDeliveryStatus::ResultUnknown.as_str(),
                ProjectionDeliveryStatus::Failed.as_str(),
            ]
        },
    }
}

fn confirmed_delivery_pipeline(mall_execution_baseline: &str, at: Instant) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "status": ProjectionDeliveryStatus::Confirmed.as_str(),
            "next_attempt_at": Bson::Null,
            "mall_ack_at": timestamp,
            "mall_execution_baseline": mall_execution_baseline,
            "error_class": Bson::Null,
            "error_code": Bson::Null,
            "error_summary": Bson::Null,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn failed_delivery_pipeline(
    status: ProjectionDeliveryStatus,
    error_class: ErrorClass,
    error_code: &str,
    error_summary: &str,
    at: Instant,
) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "status": status.as_str(),
            "next_attempt_at": Bson::Null,
            "mall_ack_at": Bson::Null,
            "mall_execution_baseline": Bson::Null,
            "error_class": error_class.as_str(),
            "error_code": error_code,
            "error_summary": error_summary,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn retry_delivery_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": ProjectionDeliveryStatus::Failed.as_str(),
        "error_class": {
            "$in": [
                ErrorClass::TransientFailure.as_str(),
                ErrorClass::RateLimited.as_str(),
            ]
        },
    }
}

fn retry_delivery_pipeline(at: Instant, next_attempt_at: Instant) -> Vec<Document> {
    let updated_at = at.unix_secs();
    let next_attempt_at = next_attempt_at.unix_secs();
    vec![doc! {
        "$set": {
            "status": ProjectionDeliveryStatus::Retrying.as_str(),
            "next_attempt_at": next_attempt_at,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": updated_at,
        }
    }]
}

fn escalation_delivery_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "status": {
            "$in": [
                ProjectionDeliveryStatus::Sending.as_str(),
                ProjectionDeliveryStatus::ResultUnknown.as_str(),
                ProjectionDeliveryStatus::Failed.as_str(),
            ]
        },
    }
}

fn escalation_delivery_pipeline(
    error_class: ErrorClass,
    error_code: &str,
    error_summary: &str,
    error_task_id: &IntegrationErrorTaskId,
    work_item_id: &WorkItemId,
    at: Instant,
) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "status": ProjectionDeliveryStatus::Manual.as_str(),
            "next_attempt_at": Bson::Null,
            "error_class": error_class.as_str(),
            "error_code": error_code,
            "error_summary": error_summary,
            "error_task_id": error_task_id.to_string(),
            "work_item_id": work_item_id.to_string(),
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

/// D27 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ProjectionExt::projection()` 访问。
pub struct ProjectionRepository<'a> {
    db: &'a Database,
}

impl<'a> ProjectionRepository<'a> {
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

    /// 建立投影稳定身份及其首个投影版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_order_projections` 与 `sales_order_projection_revisions`，
    /// 保证「稳定投影 + 首个不可变版本」原子可见（数据模型 §6.16）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有投影没有版本的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `projection` - 待写入的投影稳定身份
    /// * `revision` - 待写入的首个投影版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_projection_revision(
        &self,
        projection: &SalesOrderProjection,
        revision: &SalesOrderProjectionRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjection>(SALES_ORDER_PROJECTIONS),
            projection,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionRevision>(SALES_ORDER_PROJECTION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立投影版本及其下发记录（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_order_projection_revisions` 与
    /// `sales_order_projection_deliveries`，保证「不可变版本 + 下发记录」
    /// 原子可见（数据模型 §6.16）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下没有下发记录的投影版本；
    /// Service 必须通过事务传入执行器。
    ///
    /// # 参数
    /// * `revision` - 待写入的投影版本
    /// * `delivery` - 待写入的下发记录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_projection_revision_with_delivery(
        &self,
        revision: &SalesOrderProjectionRevision,
        delivery: &SalesOrderProjectionDelivery,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionRevision>(SALES_ORDER_PROJECTION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionDelivery>(SALES_ORDER_PROJECTION_DELIVERIES),
            delivery,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单收敛）。
///
/// # 参数
/// * `sort_by` - 排序字段；仅允许 `sales_order_id`/`updated_at`/`created_at`，
///   其余一律回退 `created_at` 降序
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    match sort_by {
        Some("sales_order_id") => doc! { "sales_order_id": direction },
        Some("updated_at") => doc! { "updated_at": direction },
        _ => doc! { "created_at": direction },
    }
}

/// 投影列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "target_mall_id": 1,
        "current_acked_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 投影修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_revision_projection() -> Document {
    doc! {
        "id": 1,
        "projection_id": 1,
        "revision_no": 1,
        "projection_source": 1,
        "sales_order_revision_id": 1,
        "customer_external_identity": 1,
        "face_value": 1,
        "card_count": 1,
        "card_form": 1,
        "effective_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 构建执行投影最大修订号查询条件。
fn latest_projection_revision_filter(projection_id: &entities::ids::SalesOrderProjectionId) -> Document {
    doc! {
        "projection_id": projection_id.to_string(),
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构建执行投影最大修订号的最小投影与有界排序。
fn latest_projection_revision_options() -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "revision_no": -1 })
        .limit(1)
        .projection(doc! { "revision_no": 1, "_id": 0 })
        .build()
}

/// 从已按修订号倒序返回的零或一条投影中读取最大修订号。
fn projection_revision_no_from_rows(rows: Vec<SalesOrderProjectionRevisionNoRow>) -> Option<u32> {
    rows.into_iter().next().map(|row| row.revision_no)
}

/// 投影下发列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "projection_revision_id": 1,
        "target_mall_id": 1,
        "status": 1,
        "attempt_count": 1,
        "last_attempt_at": 1,
        "next_attempt_at": 1,
        "mall_ack_at": 1,
        "mall_execution_baseline": 1,
        "error_class": 1,
        "error_code": 1,
        "error_summary": 1,
        "error_task_id": 1,
        "work_item_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        claim_delivery_filter, latest_projection_revision_filter, latest_projection_revision_options,
        projection_revision_no_from_rows, retry_delivery_pipeline, sort_doc, QueryFilter,
        SalesOrderProjectionDeliveryFilter, SalesOrderProjectionFilter, SalesOrderProjectionRevisionNoRow,
    };
    use entities::common::time::Instant;
    use entities::ids::{SalesOrderId, SalesOrderProjectionId, SourceSystemId};
    use entities::projection::ProjectionDeliveryStatus;
    use mongodb::bson::doc;

    #[test]
    fn projection_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderProjectionFilter {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
    }

    #[test]
    fn delivery_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderProjectionDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProjectionDeliveryStatus::Retrying),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("status").unwrap(), "retrying");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("sales_order_id"), true),
            doc! { "sales_order_id": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false),
            doc! { "created_at": -1 },
            "白名单外字段一律回退默认排序"
        );
    }

    #[test]
    fn delivery_claim_filter_cas_only_accepts_pending_or_due_retry() {
        let filter = claim_delivery_filter("delivery-1", 7, Instant::from_unix_secs(100));
        assert_eq!(filter.get_str("id").unwrap(), "delivery-1");
        assert_eq!(filter.get_i64("version").unwrap(), 7);
        let choices = filter.get_array("$or").unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(
            choices[0].as_document().unwrap().get_str("status").unwrap(),
            "pending_send"
        );
        assert_eq!(
            choices[1].as_document().unwrap().get_str("status").unwrap(),
            "retrying"
        );
    }

    #[test]
    fn retry_pipeline_separates_schedule_from_update_time() {
        let pipeline = retry_delivery_pipeline(Instant::from_unix_secs(100), Instant::from_unix_secs(160));
        let set = pipeline[0].get_document("$set").unwrap();
        assert_eq!(set.get_i64("updated_at").unwrap(), 100);
        assert_eq!(set.get_i64("next_attempt_at").unwrap(), 160);
    }

    #[test]
    fn latest_revision_query_is_minimal_bounded_and_reads_empty_history() {
        assert_eq!(
            latest_projection_revision_filter(&SalesOrderProjectionId::new("projection-1")),
            doc! { "projection_id": "projection-1", "deleted_at": 0_i64 }
        );
        let options = latest_projection_revision_options();
        assert_eq!(options.sort, Some(doc! { "revision_no": -1 }));
        assert_eq!(options.limit, Some(1));
        assert_eq!(options.projection, Some(doc! { "revision_no": 1, "_id": 0 }));
        assert_eq!(projection_revision_no_from_rows(Vec::new()), None);
        assert_eq!(
            projection_revision_no_from_rows(vec![SalesOrderProjectionRevisionNoRow { revision_no: 5 }]),
            Some(5)
        );
    }
}
