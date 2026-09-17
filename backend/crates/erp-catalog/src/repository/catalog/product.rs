use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{ProductId, ProductRevisionId};
use erp_core::money::Amount;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{
    Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter, mongo_ops,
};
use serde::{Deserialize, Serialize};

use super::CatalogRepository;
use super::shared::{PRODUCT_REVISIONS, in_filter, sort_doc};
use crate::entity::catalog::{
    EnableStatus, Product, ProductKind, ProductListingStatus, ProductRevision, ProductRevisionMedia,
    SkuCoverageStatus,
};
use crate::repository::CatalogExt;
use crate::repository::owned::{
    ProductRepository, ProductRevisionMediaRepository, ProductRevisionRepository,
};

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
    /// 当前维护人。
    #[serde(default)]
    pub maintainer_user_id: String,
    /// 当前业务组织。
    #[serde(default)]
    pub business_org_unit_id: String,
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

/// 商品列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductFilter {
    /// 稳定主键集合（`$in` 精确匹配）；`None` 表示不筛选。
    pub ids: Option<Vec<String>>,
    /// 已证明的维护人授权条件；`None` 表示调用方尚未注入范围。
    pub scope: Option<super::scope::CatalogReadScope>,
    /// 维护人筛选，只收窄授权结果。
    pub maintainer_user_ids: Option<Vec<String>>,
    /// 业务组织筛选，只收窄授权结果。
    pub business_org_unit_ids: Option<Vec<String>>,
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

impl Default for ProductFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        Self {
            ids: None,
            scope: None,
            maintainer_user_ids: None,
            business_org_unit_ids: None,
            product_no: None,
            keyword: None,
            product_kind: None,
            category_id: None,
            brand_id: None,
            supplier_id: None,
            status: None,
            listing_status: None,
            supply_coverage: None,
            sales_price_min: None,
            sales_price_max: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
    }
}

impl QueryFilter for ProductFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(ids) = &self.ids {
            filter.extend(in_filter("id", ids.iter().cloned()));
        }
        if let Some(scope) = &self.scope {
            filter = doc! { "$and": [filter, scope.document()] };
        }
        if let Some(ids) = &self.maintainer_user_ids {
            filter.extend(in_filter("maintainer_user_id", ids.iter().cloned()));
        }
        if let Some(ids) = &self.business_org_unit_ids {
            filter.extend(in_filter("business_org_unit_id", ids.iter().cloned()));
        }
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

impl<'a> ProductRepository<'a> {
    /// 按稳定主键批量查询商品。
    ///
    /// # 参数
    /// * `ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除商品实体。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_ids(&self, ids: &[ProductId], executor: &mut dyn Executor) -> Result<Vec<Product>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(in_filter("id", ids.iter().map(ToString::to_string)), executor).await
    }

    /// 列出当前范围内的商品主键，供采购负责人批量解析。
    ///
    /// # 参数
    /// * `scope` - 已证明的对象范围
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 个主键；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仓储不得按登录用户自行推断权限；公司范围应由调用方跳过本方法。
    pub async fn list_authorized_ids(
        &self,
        scope: &super::scope::CatalogReadScope,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        #[derive(serde::Deserialize)]
        struct ProductIdRow {
            id: String,
        }
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<ProductIdRow>(),
            doc! {
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "$and": [scope.document()]
            },
            FindOptions::builder().projection(doc! { "id": 1 }).sort(doc! { "id": 1 }).limit(10001).build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    /// 列出未删除商品主键，供公司范围采购负责人筛选。
    ///
    /// # 参数
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// 最多 10001 个主键；调用方必须整体拒绝超限。
    ///
    /// # 错误
    /// 数据库读取失败时返回仓储错误。
    ///
    /// # 关键业务约束
    /// 仅用于已证明的公司范围；不得替代授权条件。
    pub async fn list_ids(&self, executor: &mut dyn Executor) -> Result<Vec<String>> {
        #[derive(serde::Deserialize)]
        struct ProductIdRow {
            id: String,
        }
        let rows = mongo_ops::find_many(
            &self.collection().clone_with_type::<ProductIdRow>(),
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            FindOptions::builder().projection(doc! { "id": 1 }).sort(doc! { "id": 1 }).limit(10001).build(),
            executor,
        )
        .await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
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
    /// 该修订关联的 SPU 级媒体行；分页查询由 CatalogRepository 批量装配。
    #[serde(default)]
    pub media: Vec<ProductRevisionMedia>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 商品停用事务所需的稳定实体、当前修订、媒体与下一序号快照。
#[derive(Debug, Clone)]
pub struct ProductDisableSnapshot {
    /// 待停用的商品稳定实体。
    pub product: Product,
    /// 按当前指针或最大修订号解析出的当前商品修订。
    pub current_revision: Option<ProductRevision>,
    /// 当前商品修订关联的媒体行。
    pub media: Vec<ProductRevisionMedia>,
    /// 同一商品内的历史最大修订序号；无修订时为空。
    pub latest_revision_no: Option<u32>,
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

impl<'a> ProductRevisionRepository<'a> {
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
            .sort(product_revision_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult { items, total: total as i64 })
    }

    /// 按稳定主键批量查询商品修订。
    ///
    /// # 参数
    /// * `ids` - 商品修订稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除商品修订实体；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_ids(
        &self,
        ids: &[ProductRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevision>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(in_filter("id", ids.iter().map(ToString::to_string)), executor).await
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
        self.find_many(doc! { "product_id": { "$in": ids } }, executor).await
    }
}

impl<'a> ProductRevisionMediaRepository<'a> {
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
            in_filter("product_revision_id", revision_ids.iter().map(|id| id.to_string())),
            executor,
        )
        .await
    }
}

impl<'a> CatalogRepository<'a> {
    /// 分页查询商品修订并批量装配关联媒体。
    ///
    /// # 参数
    /// * `filter` - 商品修订、状态、分页与排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已带关联媒体行的商品修订分页投影。
    ///
    /// # 错误
    /// MongoDB 查询、计数或关联媒体批量读取失败时返回错误。
    pub async fn product_revision_page(
        &self,
        filter: &ProductRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductRevisionRow>> {
        let mut result = self.db.product_revisions().search_product_revisions(filter, executor).await?;
        self.attach_product_revision_media(&mut result.items, executor).await?;
        Ok(result)
    }

    /// 读取指定商品的历史最大修订序号。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回历史最大修订号；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn latest_product_revision_no(
        &self,
        product_id: &ProductId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        let revisions = self
            .db
            .product_revisions()
            .find_by_product_ids(std::slice::from_ref(product_id), executor)
            .await?;
        Ok(revisions.iter().map(|revision| revision.revision.revision_no).max())
    }

    /// 读取商品停用事务所需的关系快照。
    ///
    /// # 参数
    /// * `product_id` - 待停用商品稳定 ID
    /// * `executor` - 数据访问执行器；停用流程必须传入事务会话
    ///
    /// # 返回
    /// 商品不存在时返回 `None`；存在时返回商品、当前修订、媒体与历史最大修订号。
    ///
    /// # 错误
    /// MongoDB 查询、关系解析或反序列化失败时返回错误。
    pub async fn product_disable_snapshot(
        &self,
        product_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductDisableSnapshot>> {
        let Some(product) = self.db.products().find_by_id(product_id, executor).await? else {
            return Ok(None);
        };
        let revisions =
            self.db.product_revisions().find_by_product_ids(&[ProductId::new(product_id)], executor).await?;
        let latest_revision_no = revisions.iter().map(|row| row.revision.revision_no).max();
        let current_revision = select_current_product_revision(&product, &revisions).cloned();
        let media = self.media_for_current_product_revision(current_revision.as_ref(), executor).await?;
        Ok(Some(ProductDisableSnapshot { product, current_revision, media, latest_revision_no }))
    }

    /// 批量解析一组商品的当前修订。
    ///
    /// 优先使用稳定主表的当前修订指针；指针缺失或失效时回退到该商品最大修订号。
    ///
    /// # 参数
    /// * `products` - 待解析的商品稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 `product_id -> 当前商品修订` 映射；没有任何修订的商品不出现在映射中。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    pub async fn current_product_revisions(
        &self,
        products: &[Product],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, ProductRevision>> {
        let product_ids =
            products.iter().map(|product| ProductId::new(product.base.id.clone())).collect::<Vec<_>>();
        let revisions = self.db.product_revisions().find_by_product_ids(&product_ids, executor).await?;
        Ok(select_current_product_revisions(products, revisions))
    }

    /// 解析单个商品的当前修订。
    ///
    /// # 参数
    /// * `product` - 已加载的商品稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前指针命中或最大修订号对应的修订；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_product_revision(
        &self,
        product: &Product,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductRevision>> {
        Ok(self
            .current_product_revisions(std::slice::from_ref(product), executor)
            .await?
            .remove(&product.base.id))
    }

    /// 批量装配商品修订投影的媒体关系。
    ///
    /// # 参数
    /// * `rows` - 当前页商品修订投影
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 成功时原位填充每行 `media` 字段。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    async fn attach_product_revision_media(
        &self,
        rows: &mut [ProductRevisionRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let revision_ids = rows.iter().map(|row| ProductRevisionId::new(row.id.clone())).collect::<Vec<_>>();
        let media =
            self.db.product_revision_medias().find_media_by_revision_ids(&revision_ids, executor).await?;
        let mut grouped = group_product_revision_media(media);
        for row in rows {
            row.media = grouped.remove(&row.id).unwrap_or_default();
        }
        Ok(())
    }

    /// 读取可选当前商品修订的关联媒体。
    ///
    /// # 参数
    /// * `revision` - 已解析出的可选当前商品修订
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无当前修订时返回空集合，否则返回该修订全部媒体行。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    async fn media_for_current_product_revision(
        &self,
        revision: Option<&ProductRevision>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let Some(revision) = revision else {
            return Ok(Vec::new());
        };
        self.db
            .product_revision_medias()
            .find_media_by_revision_ids(&[ProductRevisionId::new(revision.base.id.clone())], executor)
            .await
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
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_product_revision_with_media(
        &self,
        revision: &ProductRevision,
        medias: &[ProductRevisionMedia],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<ProductRevision>(PRODUCT_REVISIONS), revision, executor)
            .await?;
        mongo_ops::insert_many(
            &self.db.collection::<ProductRevisionMedia>(PRODUCT_REVISION_MEDIAS),
            medias.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 从同一商品的修订集合解析当前修订。
///
/// # 参数
/// * `product` - 商品稳定实体
/// * `revisions` - 该商品的全部修订
///
/// # 返回
/// 优先返回当前指针命中的修订；否则返回最大修订号；无修订时返回 `None`。
///
/// # 错误
/// 无。
fn select_current_product_revision<'a>(
    product: &Product,
    revisions: &'a [ProductRevision],
) -> Option<&'a ProductRevision> {
    product
        .stable
        .current_revision_id
        .as_deref()
        .and_then(|current_id| revisions.iter().find(|revision| revision.base.id == current_id))
        .or_else(|| revisions.iter().max_by_key(|revision| revision.revision.revision_no))
}

/// 批量解析商品当前修订映射。
///
/// # 参数
/// * `products` - 商品稳定实体集合
/// * `revisions` - 这些商品的全部修订
///
/// # 返回
/// 返回 `product_id -> 当前修订` 映射，没有修订的商品被忽略。
///
/// # 错误
/// 无。
fn select_current_product_revisions(
    products: &[Product],
    revisions: Vec<ProductRevision>,
) -> HashMap<String, ProductRevision> {
    let mut grouped: HashMap<String, Vec<ProductRevision>> = HashMap::new();
    for revision in revisions {
        grouped.entry(revision.product_id.to_string()).or_default().push(revision);
    }
    products
        .iter()
        .filter_map(|product| {
            let revisions = grouped.get(&product.base.id)?;
            select_current_product_revision(product, revisions)
                .cloned()
                .map(|revision| (product.base.id.clone(), revision))
        })
        .collect()
}

/// 按商品修订 ID 分组并排序媒体行。
///
/// # 参数
/// * `rows` - 多个商品修订的媒体关系行
///
/// # 返回
/// 返回 `product_revision_id -> 媒体行` 映射，各组按展示顺序升序排列。
///
/// # 错误
/// 无。
fn group_product_revision_media(
    rows: Vec<ProductRevisionMedia>,
) -> HashMap<String, Vec<ProductRevisionMedia>> {
    let mut grouped: HashMap<String, Vec<ProductRevisionMedia>> = HashMap::new();
    for row in rows {
        grouped.entry(row.product_revision_id.to_string()).or_default().push(row);
    }
    for media in grouped.values_mut() {
        media.sort_by_key(|row| row.sort_order);
    }
    grouped
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

#[cfg(test)]
mod tests {
    use persistence_core::Pagination;

    use super::ProductFilter;

    #[test]
    fn product_filter_default_starts_at_page_one_size_twenty() {
        let filter = ProductFilter::default();
        assert_eq!(filter.page, 1);
        assert_eq!(filter.page_size, 20);
        assert_eq!(filter.ids, None);
        assert_eq!(filter.sales_price_min, None);
        assert!(!filter.sort_ascending);
        assert_eq!(filter.page_and_size(), (1, 20));
    }

    #[test]
    fn product_filter_default_matches_empty_pagination() {
        let filter = ProductFilter::default();
        assert_eq!(filter.skip(), 0);
        assert_eq!(filter.limit(), 20);
    }
}
