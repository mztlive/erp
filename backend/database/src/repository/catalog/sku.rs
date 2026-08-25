use std::collections::{HashMap, HashSet};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, ListingStatus, Product, Sku, SkuRevision, SkuRevisionAttributeValue};
use entities::common::time::BusinessDate;
use entities::ids::{ProductId, SkuId};
use entities::money::{Amount, Quantity};

use super::super::extensions::CatalogExt;
use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{in_filter, sort_doc, SKUS, SKU_REVISIONS};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sku_revision_attribute_value` 集合名（单一来源：`CatalogExt` 关联常量）。
const SKU_REVISION_ATTRIBUTE_VALUES: &str = <mongodb::Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES;

/// SKU 列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRow {
    /// 实体主键。
    pub id: String,
    /// SKU 编号。
    pub sku_no: String,
    /// 所属 SPU。
    pub product_id: String,
    /// 唯一基础单位。
    pub base_unit_id: String,
    /// 规范化规格签名。
    pub specification_signature: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 上架状态；旧文档缺失值按原有可售行为兼容为已上架。
    #[serde(default = "legacy_sku_listing_status")]
    pub listing_status: ListingStatus,
    /// 当前 SKU 修订 ID。
    pub current_revision_id: Option<String>,
    /// 当前 SKU 修订名称；分页查询由 CatalogRepository 批量装配。
    #[serde(default)]
    pub name: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 上架概念引入前的 SKU 投影行保持原有可售行为。
fn legacy_sku_listing_status() -> ListingStatus {
    ListingStatus::Listed
}

/// SKU 列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuFilter {
    /// SKU 编号字面量正则（忽略大小写）；`None` 表示不筛选。
    pub sku_no: Option<String>,
    /// 关键字命中的 SKU 主键（SKU 编号或当前修订名称）；`None` 表示不筛选。
    pub ids: Option<Vec<String>>,
    /// 所属 SPU；`None` 表示不筛选。
    pub product_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 上架状态；`None` 表示不筛选。
    pub listing_status: Option<ListingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`sku_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "sku_no", self.sku_no.as_deref());
        if let Some(ids) = &self.ids {
            filter.extend(in_filter("id", ids.iter().cloned()));
        }
        if let Some(product_id) = &self.product_id {
            filter.insert("product_id", product_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(listing_status) = self.listing_status {
            let value = match listing_status {
                ListingStatus::Listed => doc! { "$in": [ListingStatus::Listed.as_str(), null] },
                ListingStatus::Unlisted => doc! { "$eq": ListingStatus::Unlisted.as_str() },
            };
            filter.insert("listing_status", value);
        }
        filter
    }
}

impl Pagination for SkuFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Sku> {
    /// 按稳定主键批量查询 SKU。
    ///
    /// # 参数
    /// * `ids` - SKU 稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU 实体。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_ids(&self, ids: &[SkuId], executor: &mut dyn Executor) -> Result<Vec<Sku>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(in_filter("id", ids.iter().map(ToString::to_string)), executor)
            .await
    }

    /// 批量查询一组商品下的全部 SKU。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU 实体。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("product_id", product_ids.iter().map(ToString::to_string)),
            executor,
        )
        .await
    }

    /// 分页检索 SKU 列表（投影查询）。
    ///
    /// 只返回 [`SkuRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`sku_no`）。
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
    pub async fn search_skus(
        &self,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>> {
        let options = FindOptions::builder()
            .sort(sku_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }
}

/// SKU 修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属稳定 SKU。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 公司审核后的 SKU 描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值（规范化精确查询字段）。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<String>,
    /// 重量（千克）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米）。
    pub volume_m3: Option<Quantity>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 公司对销售可见的含税价格（Decimal128 定点金额）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// SKU 修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct SkuRevisionFilter {
    /// 所属稳定 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 条码精确匹配（按实体构造时 trim 规范化后的持久化值）；`None` 表示不筛选。
    pub barcode: Option<String>,
    /// 修订启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(barcode) = &self.barcode {
            filter.insert("barcode", normalized_barcode(barcode));
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuRevision> {
    /// 分页检索 SKU 修订列表（投影查询）。
    ///
    /// 只返回 [`SkuRevisionRow`] 所需的列表字段（含 Decimal128 销售可见价，
    /// 不做舍入或换算）；排序字段白名单化（`created_at`/`revision_no`）。
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
    pub async fn search_sku_revisions(
        &self,
        filter: &SkuRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRevisionRow>> {
        let options = FindOptions::builder()
            .sort(sku_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按规范化条码精确查询全部「在用」SKU 修订。
    ///
    /// 条码走 `idx_sku_revisions_barcode` 精确查询索引；同一条码允许命中多个
    /// 在用 SKU 修订（数据模型 §6.3：同一条码出现多个在用 SKU 时由 Service
    /// 阻断正式启用并转人工，条码不是稳定身份）。
    ///
    /// # 参数
    /// * `barcode` - 条码原值（内部按 trim 规范化后精确匹配）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回启用状态且条码精确匹配的 SKU 修订集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_active_by_barcode(
        &self,
        barcode: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        self.find_many(
            doc! {
                "barcode": normalized_barcode(barcode),
                "status": EnableStatus::Active.as_str(),
            },
            executor,
        )
        .await
    }

    /// 批量查询一组 SKU 的修订（`$in`，一次取回）。
    ///
    /// 用于按 SKU 聚合修订明细，避免逐 SKU N+1。
    ///
    /// # 参数
    /// * `sku_ids` - SKU ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = sku_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "sku_id": { "$in": ids } }, executor).await
    }
}

impl<'a> CatalogRepository<'a> {
    /// 按 SKU 编号或当前修订名称解析公司 SKU 主键。
    ///
    /// 两个字段均按字面量部分匹配并忽略大小写；名称命中只接受稳定 SKU 当前修订，
    /// 避免历史名称继续污染供给列表关键字筛选。
    ///
    /// # 参数
    /// * `keyword` - 已去除首尾空白的关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重并按主键排序的命中 SKU 主键；无命中时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn resolve_sku_ids_by_keyword(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuId>> {
        let mut sku_filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut sku_filter, "sku_no", Some(keyword));
        let mut skus = Repository::<Sku>::new(self.db, SKUS)
            .find_many(sku_filter, executor)
            .await?;

        let mut revision_filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut revision_filter, "name", Some(keyword));
        let revisions = Repository::<SkuRevision>::new(self.db, SKU_REVISIONS)
            .find_many(revision_filter, executor)
            .await?;
        if !revisions.is_empty() {
            let revision_ids = revisions.into_iter().map(|revision| revision.base.id);
            skus.extend(
                Repository::new(self.db, SKUS)
                    .find_many(in_filter("current_revision_id", revision_ids), executor)
                    .await?,
            );
        }

        let mut ids = skus
            .into_iter()
            .map(|sku| SkuId::new(sku.base.id))
            .collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        Ok(ids)
    }

    /// 按 SPU 编号和 SKU 编号解析供给筛选所需的公司 SKU 主键。
    ///
    /// 两个字段均按字面量部分匹配并忽略大小写；同时提供时取交集。任一已提供
    /// 条件无命中时返回空集合，`None` 只表示两个条件均未提供。
    ///
    /// # 参数
    /// * `product_no` - 公司商品编号筛选
    /// * `sku_no` - 公司 SKU 编号筛选
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回可选的去重 SKU 主键集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn resolve_sku_ids_by_codes(
        &self,
        product_no: Option<&str>,
        sku_no: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<SkuId>>> {
        let by_product = match product_no {
            Some(product_no) => Some(self.sku_ids_by_product_no(product_no, executor).await?),
            None => None,
        };
        let by_sku = match sku_no {
            Some(sku_no) => Some(self.sku_ids_by_sku_no(sku_no, executor).await?),
            None => None,
        };
        let mut ids = match (by_product, by_sku) {
            (None, None) => return Ok(None),
            (Some(ids), None) | (None, Some(ids)) => ids,
            (Some(left), Some(right)) => left.into_iter().filter(|id| right.contains(id)).collect(),
        };
        ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        Ok(Some(ids))
    }

    /// 按商品编号解析其下全部 SKU 主键。
    ///
    /// # 参数
    /// * `product_no` - 公司商品编号字面量关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中商品下的全部未删除 SKU 主键。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn sku_ids_by_product_no(
        &self,
        product_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuId>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "product_no", Some(product_no));
        let products = Repository::<Product>::new(self.db, <mongodb::Database as CatalogExt>::PRODUCTS)
            .find_many(filter, executor)
            .await?;
        if products.is_empty() {
            return Ok(Vec::new());
        }
        let product_ids = products
            .into_iter()
            .map(|product| ProductId::new(product.base.id))
            .collect::<Vec<_>>();
        Ok(Repository::<Sku>::new(self.db, SKUS)
            .find_by_product_ids(&product_ids, executor)
            .await?
            .into_iter()
            .map(|sku| SkuId::new(sku.base.id))
            .collect())
    }

    /// 按 SKU 编号解析 SKU 主键。
    ///
    /// # 参数
    /// * `sku_no` - 公司 SKU 编号字面量关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中的未删除 SKU 主键。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn sku_ids_by_sku_no(&self, sku_no: &str, executor: &mut dyn Executor) -> Result<Vec<SkuId>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "sku_no", Some(sku_no));
        Ok(Repository::<Sku>::new(self.db, SKUS)
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|sku| SkuId::new(sku.base.id))
            .collect())
    }

    /// 分页查询 SKU 并批量装配当前修订名称。
    ///
    /// # 参数
    /// * `keyword` - SKU 编号或当前修订名称关键字；`None` 表示不筛选
    /// * `filter` - SKU 编号、归属、状态、分页与排序条件；调用方不得预填 `ids`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已带当前修订名称的 SKU 分页投影。
    ///
    /// # 错误
    /// MongoDB 查询、计数或当前修订批量读取失败时返回错误。
    pub async fn sku_page(
        &self,
        keyword: Option<&str>,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>> {
        let mut filter = filter.clone();
        filter.ids = match keyword {
            Some(keyword) => Some(
                self.resolve_sku_ids_by_keyword(keyword, executor)
                    .await?
                    .into_iter()
                    .map(|id| id.to_string())
                    .collect(),
            ),
            None => None,
        };
        let mut result = self.db.skus().search_skus(&filter, executor).await?;
        self.attach_current_sku_names(&mut result.items, executor).await?;
        Ok(result)
    }

    /// 分页查询 SKU 修订投影。
    ///
    /// # 参数
    /// * `filter` - SKU 修订、名称、条码、状态、分页与排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 SKU 修订分页投影。
    ///
    /// # 错误
    /// MongoDB 查询、计数或结果反序列化失败时返回错误。
    pub async fn sku_revision_page(
        &self,
        filter: &SkuRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRevisionRow>> {
        self.db
            .sku_revisions()
            .search_sku_revisions(filter, executor)
            .await
    }

    /// 按稳定 ID 读取单个未删除 SKU。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配 SKU；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn sku(&self, sku_id: &str, executor: &mut dyn Executor) -> Result<Option<Sku>> {
        self.db.skus().find_by_id(sku_id, executor).await
    }

    /// 读取一个商品下的全部未删除 SKU。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该商品的全部 SKU，包含启用和历史停用身份。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn skus_for_product(
        &self,
        product_id: &ProductId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        self.db
            .skus()
            .find_by_product_ids(std::slice::from_ref(product_id), executor)
            .await
    }

    /// 读取指定 SKU 的历史最大修订序号。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回历史最大修订号；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn latest_sku_revision_no(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let revisions = self
            .db
            .sku_revisions()
            .find_by_sku_ids(std::slice::from_ref(sku_id), executor)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max())
    }

    /// 返回当前启用修订中占用规范化条码的 SKU 身份。
    ///
    /// # 参数
    /// * `barcode` - 条码原值，Repository 按实体 trim 规则规范化
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重后的条码占用 SKU ID 集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn barcode_owner_sku_ids(
        &self,
        barcode: &str,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        Ok(self
            .db
            .sku_revisions()
            .find_active_by_barcode(barcode, executor)
            .await?
            .into_iter()
            .map(|revision| revision.sku_id.to_string())
            .collect())
    }

    /// 批量解析一组 SKU 的当前修订。
    ///
    /// 优先使用稳定主表当前修订指针；指针缺失或失效时回退到该 SKU 最大修订号。
    ///
    /// # 参数
    /// * `skus` - 待解析的 SKU 稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 `sku_id -> 当前 SKU 修订` 映射；没有修订的 SKU 不出现在映射中。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    pub async fn current_sku_revisions(
        &self,
        skus: &[Sku],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevision>> {
        let sku_ids = skus
            .iter()
            .map(|sku| SkuId::new(sku.base.id.clone()))
            .collect::<Vec<_>>();
        let revisions = self
            .db
            .sku_revisions()
            .find_by_sku_ids(&sku_ids, executor)
            .await?;
        Ok(select_current_sku_revisions(skus, revisions))
    }

    /// 解析单个 SKU 的当前修订。
    ///
    /// # 参数
    /// * `sku` - 已加载的 SKU 稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前指针命中或最大修订号对应的修订；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_sku_revision(
        &self,
        sku: &Sku,
        executor: &mut dyn Executor,
    ) -> Result<Option<SkuRevision>> {
        Ok(self
            .current_sku_revisions(std::slice::from_ref(sku), executor)
            .await?
            .remove(&sku.base.id))
    }

    /// 批量装配 SKU 列表投影的当前修订名称。
    ///
    /// # 参数
    /// * `rows` - 当前页 SKU 投影
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 成功时原位填充每行 `name` 字段。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    async fn attach_current_sku_names(&self, rows: &mut [SkuRow], executor: &mut dyn Executor) -> Result<()> {
        let revision_ids = rows
            .iter()
            .filter_map(|row| row.current_revision_id.clone())
            .collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(());
        }
        let revisions = self
            .db
            .sku_revisions()
            .find_many(in_filter("id", revision_ids), executor)
            .await?;
        let names = revisions
            .into_iter()
            .map(|revision| (revision.base.id, revision.name))
            .collect::<HashMap<_, _>>();
        for row in rows {
            row.name = row
                .current_revision_id
                .as_ref()
                .and_then(|revision_id| names.get(revision_id).cloned());
        }
        Ok(())
    }

    /// 建立「稳定 SKU + 首个 SKU 修订 + 修订规格属性值」（跨集合多步骤写入）。
    ///
    /// 依次写入 `skus`、`sku_revisions`、`sku_revision_attribute_values`，
    /// 保证「SKU 身份 + 修订快照 + 规格值」原子可见（数据模型 §6.3）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下有 SKU 没有修订的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `sku` - 待写入的稳定 SKU
    /// * `revision` - 待写入的 SKU 首个修订
    /// * `attribute_values` - 待写入的修订规格属性值
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sku_with_revision(
        &self,
        sku: &Sku,
        revision: &SkuRevision,
        attribute_values: &[SkuRevisionAttributeValue],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Sku>(SKUS), sku, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<SkuRevision>(SKU_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SkuRevisionAttributeValue>(SKU_REVISION_ATTRIBUTE_VALUES),
            attribute_values.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 从同一 SKU 的修订集合解析当前修订。
///
/// # 参数
/// * `sku` - SKU 稳定实体
/// * `revisions` - 该 SKU 的全部修订
///
/// # 返回
/// 优先返回当前指针命中的修订；否则返回最大修订号；无修订时返回 `None`。
///
/// # 错误
/// 无。
fn select_current_sku_revision<'a>(sku: &Sku, revisions: &'a [SkuRevision]) -> Option<&'a SkuRevision> {
    sku.stable
        .current_revision_id
        .as_deref()
        .and_then(|current_id| revisions.iter().find(|revision| revision.base.id == current_id))
        .or_else(|| {
            revisions
                .iter()
                .max_by_key(|revision| revision.revision.revision_no)
        })
}

/// 批量解析 SKU 当前修订映射。
///
/// # 参数
/// * `skus` - SKU 稳定实体集合
/// * `revisions` - 这些 SKU 的全部修订
///
/// # 返回
/// 返回 `sku_id -> 当前修订` 映射，没有修订的 SKU 被忽略。
///
/// # 错误
/// 无。
fn select_current_sku_revisions(skus: &[Sku], revisions: Vec<SkuRevision>) -> HashMap<String, SkuRevision> {
    let mut grouped: HashMap<String, Vec<SkuRevision>> = HashMap::new();
    for revision in revisions {
        grouped
            .entry(revision.sku_id.to_string())
            .or_default()
            .push(revision);
    }
    skus.iter()
        .filter_map(|sku| {
            let revisions = grouped.get(&sku.base.id)?;
            select_current_sku_revision(sku, revisions)
                .cloned()
                .map(|revision| (sku.base.id.clone(), revision))
        })
        .collect()
}

/// 按实体构造时的 trim 规则规范化条码（与 `SkuRevision::new` 一致）。
///
/// # 参数
/// * `barcode` - 条码原值
///
/// # 返回
/// 返回去除首尾空白后的规范化值。
fn normalized_barcode(barcode: &str) -> &str {
    barcode.trim()
}

/// 构建 SKU 排序文档（白名单：`created_at`/`sku_no`）。
fn sku_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("sku_no") => "sku_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建 SKU 修订排序文档（白名单：`created_at`/`revision_no`）。
fn sku_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// SKU 列表投影字段。
fn sku_projection() -> Document {
    doc! {
        "id": 1,
        "sku_no": 1,
        "product_id": 1,
        "base_unit_id": 1,
        "specification_signature": 1,
        "status": 1,
        "listing_status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// SKU 修订列表投影字段。
fn sku_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "name": 1,
        "description": 1,
        "specification": 1,
        "barcode": 1,
        "source_main_image_asset_id": 1,
        "weight_kg": 1,
        "volume_m3": 1,
        "status": 1,
        "sales_visible_price_gross": 1,
        "market_price": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::catalog::{EnableStatus, ListingStatus};

    #[test]
    fn sku_revision_filter_normalizes_barcode_for_exact_query() {
        let filter = SkuRevisionFilter {
            sku_id: Some("sku-1".to_string()),
            name: None,
            barcode: Some(" 6901234567890 ".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("barcode").unwrap(), "6901234567890");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_filter_applies_listing_status() {
        let filter = SkuFilter {
            sku_no: None,
            ids: None,
            product_id: Some("product-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ListingStatus::Listed),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("product_id").unwrap(), "product-1");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let listing = document.get_document("listing_status").unwrap();
        assert_eq!(listing.get_array("$in").unwrap().len(), 2);
    }

    #[test]
    fn legacy_sku_row_without_listing_status_is_treated_as_listed() {
        let row: SkuRow = mongodb::bson::deserialize_from_document(doc! {
            "id": "sku-1",
            "sku_no": "SKU-001",
            "product_id": "product-1",
            "base_unit_id": "unit-1",
            "specification_signature": "",
            "status": "active",
            "current_revision_id": "sku-revision-1",
            "version": 1_i64,
            "created_at": 1_i64,
        })
        .unwrap();

        assert_eq!(row.listing_status, ListingStatus::Listed);
    }
}
