//! 域 D26 `publication` 仓储：product_publication(+_revision、_revision_media)、
//! product_publication_delivery。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `PublicationExt` 关联常量导入。
//!
//! 本域全部实体为正式对象（§4.5 不设业务软删除）；修订集合为不可变版本
//! （§4.4），只追加不覆盖，**不提供任何软删除方法**。
//!
//! 筛选/行类型定义在本文件，经 `PublicationExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::common::time::Instant;
use entities::ids::{InboxMessageId, IntegrationErrorTaskId, SkuId, SourceSystemId, WorkItemId};
use entities::integration_ops::ErrorClass;
use entities::money::Amount;
use entities::publication::{
    ProductPublication, ProductPublicationDelivery, ProductPublicationRevision, ProductPublicationRevisionId,
    ProductPublicationRevisionMedia, ProductPublicationStatus, PublicationDeliveryStatus, SafetyPauseCause,
    SafetyPauseSourceObjectType, SaleStatus, SystemSafetyPauseOperation,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::PublicationExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `product_publication_revision` 集合名（单一来源：`PublicationExt` 关联常量）。
const PRODUCT_PUBLICATION_REVISIONS: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS;
/// `product_publication_revision_media` 集合名（单一来源：`PublicationExt` 关联常量）。
const PRODUCT_PUBLICATION_REVISION_MEDIA: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISION_MEDIA;

/// 商品发布投递失败或结果未知的原子落库参数。
#[derive(Debug, Clone, Copy)]
pub struct PublicationDeliveryFailure<'a> {
    /// 目标状态。
    pub status: PublicationDeliveryStatus,
    /// 错误分类。
    pub error_class: ErrorClass,
    /// 稳定错误码。
    pub error_code: &'a str,
    /// 可展示的错误摘要。
    pub error_summary: &'a str,
    /// 事实发生时间。
    pub at: Instant,
}

/// 商品发布投递升级 W29 的原子落库参数。
#[derive(Debug, Clone, Copy)]
pub struct PublicationDeliveryEscalation<'a> {
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

impl<'a> Repository<'a, SystemSafetyPauseOperation> {
    /// 按可信来源事件身份查找安全暂停操作。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_safety_pause_by_event(
        &self,
        source_object_type: SafetyPauseSourceObjectType,
        source_object_id: &str,
        cause: SafetyPauseCause,
        source_version: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SystemSafetyPauseOperation>> {
        self.find_one(
            doc! {
                "source_object_type": source_object_type.as_str(),
                "source_object_id": source_object_id,
                "cause": cause.as_str(),
                "source_version": source_version,
            },
            executor,
        )
        .await
    }

    /// 按调用幂等键查找安全暂停操作。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_safety_pause_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SystemSafetyPauseOperation>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor)
            .await
    }

    /// 判断发布是否存在任一已提交的系统安全暂停证据。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn has_safety_pause_for_publication(
        &self,
        publication_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .find_one(
                doc! { "affected_publications.publication_id": publication_id },
                executor,
            )
            .await?
            .is_some())
    }

    /// 判断来源对象是否已有任一系统安全暂停证据。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn has_safety_pause_for_source(
        &self,
        source_object_type: SafetyPauseSourceObjectType,
        source_object_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self
            .find_one(
                doc! {
                    "source_object_type": source_object_type.as_str(),
                    "source_object_id": source_object_id,
                },
                executor,
            )
            .await?
            .is_some())
    }
}

/// 发布列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRow {
    /// 实体主键。
    pub id: String,
    /// ERP SKU。
    pub sku_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 发布状态。
    pub status: ProductPublicationStatus,
    /// 当前商城生效版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductPublicationFilter {
    /// ERP SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 发布状态；`None` 表示不筛选。
    pub status: Option<ProductPublicationStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductPublicationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductPublicationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductPublication> {
    /// 分页检索商品发布列表（投影查询）。
    ///
    /// 只返回 [`ProductPublicationRow`] 所需的列表字段，不加载整文档；
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
    pub async fn search_product_publications(
        &self,
        filter: &ProductPublicationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductPublicationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_publication_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductPublicationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// 发布修订列表投影行（Decimal128 金额原样投影，不做舍入换算）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属稳定发布。
    pub product_publication_id: String,
    /// 修订序号（同一发布内从 1 递增）。
    pub revision_no: u32,
    /// 商城展示名称快照。
    pub name: String,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 生效区间开始。
    pub valid_from: i64,
    /// 生效区间结束。
    pub valid_to: Option<i64>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl<'a> Repository<'a, ProductPublicationRevision> {
    /// 按「发布 + 修订序号」查找唯一发布修订。
    ///
    /// 唯一性由 `uk_product_publication_revisions_publication_revision` 唯一索引
    /// 保证（数据模型 §6.15）；修订不可变，只读入口。
    ///
    /// # 参数
    /// * `product_publication_id` - 所属稳定发布
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的发布修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_revision_by_no(
        &self,
        product_publication_id: &entities::ids::ProductPublicationId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationRevision>> {
        self.find_one(
            doc! {
                "product_publication_id": product_publication_id.to_string(),
                "revision_no": revision_no,
            },
            executor,
        )
        .await
    }

    /// 列出指定发布的全部分区版本（投影查询，修订号降序）。
    ///
    /// 只返回 [`ProductPublicationRevisionRow`] 所需的列表字段，不加载整文档；
    /// 修订为不可变版本（§4.4），本方法只读，不提供任何删除入口。
    ///
    /// # 参数
    /// * `product_publication_id` - 所属稳定发布
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `revision_no` 降序的投影行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revisions_by_publication(
        &self,
        product_publication_id: &entities::ids::ProductPublicationId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductPublicationRevisionRow>> {
        let options = FindOptions::builder()
            .sort(doc! { "revision_no": -1 })
            .projection(product_publication_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<ProductPublicationRevisionRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "product_publication_id": product_publication_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ProductPublicationRevisionMedia> {
    /// 列出指定发布修订的受控媒体（按角色、展示顺序排序）。
    ///
    /// 媒体为发布版本的不可变受控行，本方法只读，不提供任何删除入口；
    /// `(product_publication_revision_id, media_role, sort_no)` 唯一由
    /// `uk_product_publication_revision_media_revision_role_sort` 唯一索引保证
    /// （数据模型 §6.15）。
    ///
    /// # 参数
    /// * `revision_id` - 所属发布修订
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `media_role`、`sort_no` 升序的媒体实体列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_media_by_revision(
        &self,
        revision_id: &ProductPublicationRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductPublicationRevisionMedia>> {
        self.find_many_sorted(
            doc! { "product_publication_revision_id": revision_id.to_string() },
            doc! { "media_role": 1, "sort_no": 1 },
            executor,
        )
        .await
    }
}

/// 发布投递列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 所属发布修订。
    pub publication_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 跨全部尝试不变的消息身份。
    pub message_key: String,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 最近真实发送时间。
    pub last_attempt_at: Option<i64>,
    /// 下次受控处理时间。
    pub next_attempt_at: Option<i64>,
    /// 商城确认时间。
    pub mall_ack_at: Option<i64>,
    /// 商城确认版本。
    pub mall_version: Option<String>,
    /// 稳定错误分类。
    pub error_class: Option<ErrorClass>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 脱敏错误摘要。
    pub error_summary: Option<String>,
    /// 原消息信封。
    pub inbox_message_id: Option<String>,
    /// W29 错误对象。
    pub error_task_id: Option<String>,
    /// W29 正式待办。
    pub work_item_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布投递列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductPublicationDeliveryFilter {
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 投递状态；`None` 表示不筛选。
    pub delivery_status: Option<PublicationDeliveryStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductPublicationDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(delivery_status) = self.delivery_status {
            filter.insert("delivery_status", delivery_status.as_str());
        }
        filter
    }
}

impl Pagination for ProductPublicationDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductPublicationDelivery> {
    /// 分页检索发布投递记录（投影查询）。
    ///
    /// 只返回 [`ProductPublicationDeliveryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛。投递状态查询由
    /// `idx_product_publication_deliveries_status` 索引支撑（§6.15）。
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
    pub async fn search_product_publication_deliveries(
        &self,
        filter: &ProductPublicationDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductPublicationDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_publication_delivery_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<ProductPublicationDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「发布修订 + 目标商城」查找唯一投递记录。
    ///
    /// # 参数
    /// * `revision_id` - 所属发布修订
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的投递记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_delivery_by_revision_and_mall(
        &self,
        revision_id: &ProductPublicationRevisionId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        self.find_one(
            doc! {
                "publication_revision_id": revision_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }

    /// 列出待发送与已到期重试的投递；发送中和结果未知不得被盲目重放。
    pub async fn list_processable_publication_deliveries(
        &self,
        at: Instant,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductPublicationDelivery>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": 1, "id": 1 })
            .limit(i64::from(limit))
            .build();
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$or": [
                    { "delivery_status": PublicationDeliveryStatus::PendingSend.as_str() },
                    {
                        "delivery_status": PublicationDeliveryStatus::Retrying.as_str(),
                        "next_attempt_at": { "$lte": at.unix_secs() },
                    },
                ],
            },
            options,
            executor,
        )
        .await
    }

    /// 以单文档 CAS 取得待发送或到期重试投递，并保持原投递与消息身份。
    pub async fn claim_publication_delivery(
        &self,
        id: &str,
        expected_version: u64,
        inbox_message_id: &InboxMessageId,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        let expected_version = publication_metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            publication_claim_filter(id, expected_version, at),
            publication_claim_pipeline(inbox_message_id, at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 将发送中或待查结果投递落为商城已确认。
    pub async fn confirm_publication_delivery(
        &self,
        id: &str,
        expected_version: u64,
        mall_version: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        let expected_version = publication_metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            publication_result_filter(id, expected_version),
            publication_confirm_pipeline(mall_version, at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 记录明确失败或结果未知。
    pub async fn fail_publication_delivery(
        &self,
        id: &str,
        expected_version: u64,
        failure: PublicationDeliveryFailure<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        let expected_version = publication_metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            publication_result_filter(id, expected_version),
            publication_fail_pipeline(
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

    /// 以单文档 CAS 安排沿原消息身份重试。
    pub async fn schedule_publication_retry(
        &self,
        id: &str,
        expected_version: u64,
        at: Instant,
        next_attempt_at: Instant,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        let expected_version = publication_metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            publication_retry_filter(id, expected_version),
            publication_retry_pipeline(at, next_attempt_at),
            executor,
        )
        .await
    }

    /// 以单文档 CAS 关联 W29 错误对象与正式待办并转人工。
    pub async fn escalate_publication_delivery(
        &self,
        id: &str,
        expected_version: u64,
        escalation: PublicationDeliveryEscalation<'_>,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        let expected_version = publication_metadata_version(expected_version)?;
        mongo_ops::find_one_and_update_pipeline(
            &self.collection(),
            publication_escalation_filter(id, expected_version),
            publication_escalation_pipeline(
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

fn publication_metadata_version(version: u64) -> Result<i64> {
    i64::try_from(version).map_err(|_| crate::Error::EntityMetadataOutOfRange("version"))
}

fn publication_claim_filter(id: &str, expected_version: i64, at: Instant) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$or": [
            { "delivery_status": PublicationDeliveryStatus::PendingSend.as_str() },
            {
                "delivery_status": PublicationDeliveryStatus::Retrying.as_str(),
                "next_attempt_at": { "$lte": at.unix_secs() },
            },
        ],
    }
}

fn publication_claim_pipeline(inbox_message_id: &InboxMessageId, at: Instant) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "delivery_status": PublicationDeliveryStatus::Sending.as_str(),
            "attempt_count": { "$add": ["$attempt_count", 1_i64] },
            "last_attempt_at": timestamp,
            "next_attempt_at": Bson::Null,
            "inbox_message_id": { "$ifNull": ["$inbox_message_id", inbox_message_id.to_string()] },
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn publication_result_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "delivery_status": {
            "$in": [
                PublicationDeliveryStatus::Sending.as_str(),
                PublicationDeliveryStatus::ResultUnknown.as_str(),
                PublicationDeliveryStatus::Failed.as_str(),
            ]
        },
    }
}

fn publication_confirm_pipeline(mall_version: &str, at: Instant) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "delivery_status": PublicationDeliveryStatus::Confirmed.as_str(),
            "next_attempt_at": Bson::Null,
            "mall_ack_at": timestamp,
            "mall_version": mall_version,
            "error_class": Bson::Null,
            "error_code": Bson::Null,
            "error_summary": Bson::Null,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn publication_fail_pipeline(
    status: PublicationDeliveryStatus,
    error_class: ErrorClass,
    error_code: &str,
    error_summary: &str,
    at: Instant,
) -> Vec<Document> {
    let timestamp = at.unix_secs();
    vec![doc! {
        "$set": {
            "delivery_status": status.as_str(),
            "next_attempt_at": Bson::Null,
            "mall_ack_at": Bson::Null,
            "mall_version": Bson::Null,
            "error_class": error_class.as_str(),
            "error_code": error_code,
            "error_summary": error_summary,
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": timestamp,
        }
    }]
}

fn publication_retry_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "delivery_status": PublicationDeliveryStatus::Failed.as_str(),
        "error_class": {
            "$in": [ErrorClass::TransientFailure.as_str(), ErrorClass::RateLimited.as_str()]
        },
    }
}

fn publication_retry_pipeline(at: Instant, next_attempt_at: Instant) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": PublicationDeliveryStatus::Retrying.as_str(),
            "next_attempt_at": next_attempt_at.unix_secs(),
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": at.unix_secs(),
        }
    }]
}

fn publication_escalation_filter(id: &str, expected_version: i64) -> Document {
    doc! {
        "id": id,
        "version": expected_version,
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "delivery_status": {
            "$in": [
                PublicationDeliveryStatus::Sending.as_str(),
                PublicationDeliveryStatus::ResultUnknown.as_str(),
                PublicationDeliveryStatus::Failed.as_str(),
            ]
        },
    }
}

fn publication_escalation_pipeline(
    error_class: ErrorClass,
    error_code: &str,
    error_summary: &str,
    error_task_id: &IntegrationErrorTaskId,
    work_item_id: &WorkItemId,
    at: Instant,
) -> Vec<Document> {
    vec![doc! {
        "$set": {
            "delivery_status": PublicationDeliveryStatus::Manual.as_str(),
            "next_attempt_at": Bson::Null,
            "error_class": error_class.as_str(),
            "error_code": error_code,
            "error_summary": error_summary,
            "error_task_id": error_task_id.to_string(),
            "work_item_id": work_item_id.to_string(),
            "version": { "$add": ["$version", 1_i64] },
            "updated_at": at.unix_secs(),
        }
    }]
}

/// D26 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `PublicationExt::publication()` 访问。
pub struct PublicationRepository<'a> {
    db: &'a Database,
}

impl<'a> PublicationRepository<'a> {
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

    /// 建立发布修订及其受控媒体（跨集合多步骤写入）。
    ///
    /// 依次写入 `product_publication_revisions` 与
    /// `product_publication_revision_media`，保证「不可变版本 + 媒体行」
    /// 原子可见（数据模型 §6.15：提交发布必须有至少一张主图）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下没有媒体（或只有部分媒体）的
    /// 发布版本；Service 必须通过事务传入执行器。
    ///
    /// # 参数
    /// * `revision` - 待写入的发布修订
    /// * `media` - 待写入的受控媒体行清单
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_revision_with_media(
        &self,
        revision: &ProductPublicationRevision,
        media: &[ProductPublicationRevisionMedia],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ProductPublicationRevision>(PRODUCT_PUBLICATION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ProductPublicationRevisionMedia>(PRODUCT_PUBLICATION_REVISION_MEDIA),
            media.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单收敛）。
///
/// # 参数
/// * `sort_by` - 排序字段；仅允许 `sku_id`/`updated_at`/`created_at`，
///   其余一律回退 `created_at` 降序
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    match sort_by {
        Some("sku_id") => doc! { "sku_id": direction },
        Some("updated_at") => doc! { "updated_at": direction },
        _ => doc! { "created_at": direction },
    }
}

/// 发布列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "target_mall_id": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发布修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_revision_projection() -> Document {
    doc! {
        "id": 1,
        "product_publication_id": 1,
        "revision_no": 1,
        "name": 1,
        "sale_status": 1,
        "sales_price_gross": 1,
        "valid_from": 1,
        "valid_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发布投递列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "publication_revision_id": 1,
        "target_mall_id": 1,
        "message_key": 1,
        "delivery_status": 1,
        "attempt_count": 1,
        "last_attempt_at": 1,
        "next_attempt_at": 1,
        "mall_ack_at": 1,
        "mall_version": 1,
        "error_class": 1,
        "error_code": 1,
        "error_summary": 1,
        "inbox_message_id": 1,
        "error_task_id": 1,
        "work_item_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        publication_claim_filter, publication_retry_pipeline, sort_doc, ProductPublicationDeliveryFilter,
        ProductPublicationFilter, QueryFilter,
    };
    use entities::common::time::Instant;
    use entities::ids::{SkuId, SourceSystemId};
    use entities::publication::{ProductPublicationStatus, PublicationDeliveryStatus};
    use mongodb::bson::doc;

    #[test]
    fn publication_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ProductPublicationFilter {
            sku_id: Some(SkuId::new("sku-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProductPublicationStatus::MallEffective),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sku_id").unwrap(), "sku-1");
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("status").unwrap(), "mall_effective");
    }

    #[test]
    fn delivery_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ProductPublicationDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            delivery_status: Some(PublicationDeliveryStatus::PendingSend),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("delivery_status").unwrap(), "pending_send");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("sku_id"), true), doc! { "sku_id": 1 });
        assert_eq!(
            sort_doc(Some("任意字段"), false),
            doc! { "created_at": -1 },
            "白名单外字段一律回退默认排序"
        );
    }

    #[test]
    fn publication_claim_filter_cas_only_accepts_pending_or_due_retry() {
        let filter = publication_claim_filter("delivery-1", 3, Instant::from_unix_secs(100));
        assert_eq!(filter.get_str("id").unwrap(), "delivery-1");
        assert_eq!(filter.get_i64("version").unwrap(), 3);
        let choices = filter.get_array("$or").unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(
            choices[0]
                .as_document()
                .unwrap()
                .get_str("delivery_status")
                .unwrap(),
            "pending_send"
        );
        assert_eq!(
            choices[1]
                .as_document()
                .unwrap()
                .get_str("delivery_status")
                .unwrap(),
            "retrying"
        );
    }

    #[test]
    fn publication_retry_pipeline_separates_schedule_from_update_time() {
        let pipeline = publication_retry_pipeline(Instant::from_unix_secs(100), Instant::from_unix_secs(160));
        let set = pipeline[0].get_document("$set").unwrap();
        assert_eq!(set.get_i64("updated_at").unwrap(), 100);
        assert_eq!(set.get_i64("next_attempt_at").unwrap(), 160);
    }
}
