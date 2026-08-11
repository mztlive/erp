use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{
    EnableStatus, Product, ProductKind, ProductListingStatus, ProductRevision, ProductRevisionMedia,
    SkuCoverageStatus,
};
use entities::common::time::BusinessDate;
use entities::ids::{ProductId, ProductRevisionId};
use entities::money::Amount;

use super::super::extensions::CatalogExt;
use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::product_pipeline::product_list_pipeline;
use super::shared::{in_filter, sort_doc, PRODUCT_REVISIONS};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `product_revision_media` 集合名（单一来源：`CatalogExt` 关联常量）。
const PRODUCT_REVISION_MEDIAS: &str = <mongodb::Database as CatalogExt>::PRODUCT_REVISION_MEDIAS;

/// 商品列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRow {
    /// 实体主键。
    pub id: String,
    /// 商品编号。
    pub product_no: String,
    /// 商品业务类型。
    pub product_kind: ProductKind,
    /// 当前商品名称。
    pub name: Option<String>,
    /// 当前商品分类 ID。
    pub category_id: Option<String>,
    /// 当前商品品牌 ID。
    pub brand_id: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 从当前启用 SKU 继承的上架状态。
    pub listing_status: ProductListingStatus,
    /// 当前已上架 SKU 数。
    pub listed_sku_count: u32,
    /// 当前启用 SKU 总数。
    pub sku_count: u32,
    /// 当前存在有效供给关系的启用 SKU 数。
    pub supplied_sku_count: u32,
    /// 当前已填写销售价的启用 SKU 数。
    pub priced_sku_count: u32,
    /// 当前商品修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品列表聚合分页结果。
#[derive(Debug, Deserialize)]
struct ProductFacet {
    /// 当前页数据。
    items: Vec<ProductRow>,
    /// 总数聚合行。
    total: Vec<ProductTotal>,
}

/// 商品列表总数聚合行。
#[derive(Debug, Deserialize)]
struct ProductTotal {
    /// 符合筛选的商品数量。
    count: i64,
}

/// 商品列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductFilter {
    /// 商品编号字面量正则（忽略大小写）；`None` 表示不筛选。
    pub product_no: Option<String>,
    /// 商品与 SKU 统一关键字；`None` 表示不筛选。
    pub keyword: Option<String>,
    /// 商品业务类型；`None` 表示不筛选。
    pub product_kind: Option<ProductKind>,
    /// 当前商品分类；`None` 表示不筛选。
    pub category_id: Option<String>,
    /// 当前商品品牌；`None` 表示不筛选。
    pub brand_id: Option<String>,
    /// 当前启用 SKU 的有效供给供应商；`None` 表示不筛选。
    pub supplier_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 当前启用 SKU 继承上架状态；`None` 表示不筛选。
    pub listing_status: Option<ProductListingStatus>,
    /// 当前启用 SKU 的有效供给覆盖状态；`None` 表示不筛选。
    pub supply_coverage: Option<SkuCoverageStatus>,
    /// 当前启用 SKU 销售价下限（含）；`None` 表示无下限。
    pub sales_price_min: Option<Amount>,
    /// 当前启用 SKU 销售价上限（含）；`None` 表示无上限。
    pub sales_price_max: Option<Amount>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`product_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "product_no", self.product_no.as_deref());
        if let Some(product_kind) = self.product_kind {
            filter.insert("product_kind", product_kind.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 商品修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属商品 SPU。
    pub product_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的商品名称。
    pub name: String,
    /// 公司审核后的商品描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// ERP 分类 ID。
    pub category_id: String,
    /// ERP 品牌 ID。
    pub brand_id: String,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct ProductRevisionFilter {
    /// 所属商品 SPU；`None` 表示不筛选。
    pub product_id: Option<String>,
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

impl QueryFilter for ProductRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(product_id) = &self.product_id {
            filter.insert("product_id", product_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductRevision> {
    /// 分页检索商品修订列表（投影查询）。
    ///
    /// 只返回 [`ProductRevisionRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`revision_no`）。
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
    pub async fn search_product_revisions(
        &self,
        filter: &ProductRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRevisionRow>> {
        let options = FindOptions::builder()
            .sort(product_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 批量查询一组商品的修订（`$in`，一次取回）。
    ///
    /// 用于按 SKU 所属商品聚合修订明细，避免逐商品 N+1。
    ///
    /// # 参数
    /// * `product_ids` - 商品 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevision>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = product_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "product_id": { "$in": ids } }, executor)
            .await
    }
}

impl<'a> Repository<'a, ProductRevisionMedia> {
    /// 按商品修订 ID 批量读取媒体行。
    ///
    /// # 参数
    /// * `revision_ids` - 商品修订 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的媒体行。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_media_by_revision_ids(
        &self,
        revision_ids: &[ProductRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevisionMedia>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "product_revision_id",
                revision_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

impl<'a> CatalogRepository<'a> {
    /// 分页查询商品及当前启用 SKU 的聚合筛选结果。
    ///
    /// 统一关键字覆盖商品编号/名称与 SKU 编号/名称/规格/条码；上架状态、
    /// 供给覆盖和销售价区间均按当前启用 SKU 实时派生，不在商品主表冗余落库。
    ///
    /// # 参数
    /// * `filter` - 商品、当前修订与 SKU 聚合筛选条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页商品投影与满足筛选条件的总数。
    ///
    /// # 错误
    /// MongoDB 聚合、游标读取或结果反序列化失败时返回错误。
    pub async fn search_products(
        &self,
        filter: &ProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRow>> {
        let facet = self
            .aggregate_products(product_list_pipeline(filter), executor)
            .await?;
        Ok(PageResult {
            items: facet.items,
            total: facet.total.first().map_or(0, |row| row.count),
        })
    }

    /// 执行商品列表类型化聚合并收集唯一的 facet 结果。
    async fn aggregate_products(
        &self,
        pipeline: Vec<Document>,
        executor: &mut dyn Executor,
    ) -> Result<ProductFacet> {
        let collection = self
            .db
            .collection::<Product>(<mongodb::Database as CatalogExt>::PRODUCTS);
        let rows = match executor.session() {
            Some(session) => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .session(&mut *session)
                    .await?
                    .stream(session)
                    .try_collect::<Vec<_>>()
                    .await?
            }
            None => {
                collection
                    .aggregate(pipeline)
                    .with_type::<ProductFacet>()
                    .await?
                    .try_collect::<Vec<_>>()
                    .await?
            }
        };
        Ok(rows.into_iter().next().unwrap_or(ProductFacet {
            items: Vec::new(),
            total: Vec::new(),
        }))
    }

    /// 建立「商品修订 + SPU 级媒体行」（跨集合多步骤写入）。
    ///
    /// 依次写入 `product_revisions` 与 `product_revision_medias`，保证修订与
    /// 媒体行原子可见（数据模型 §6.3：媒体行随所属修订一并不可变）。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下没有媒体行的修订；Service 必须通过事务会话调用。
    ///
    /// # 参数
    /// * `revision` - 待写入的商品修订
    /// * `medias` - 待写入的修订媒体行
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_product_revision_with_media(
        &self,
        revision: &ProductRevision,
        medias: &[ProductRevisionMedia],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<ProductRevision>(PRODUCT_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ProductRevisionMedia>(PRODUCT_REVISION_MEDIAS),
            medias.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建商品修订排序文档（白名单：`created_at`/`revision_no`）。
fn product_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 商品修订列表投影字段。
fn product_revision_projection() -> Document {
    doc! {
        "id": 1,
        "product_id": 1,
        "revision_no": 1,
        "name": 1,
        "description": 1,
        "specification": 1,
        "category_id": 1,
        "brand_id": 1,
        "status": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}
