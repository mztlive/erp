use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{ProductId, SkuId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::shared::{batch_ids_filter, default_paging, in_filter, sort_doc, whitelisted_sort};
use crate::dto::catalog::SKU_SORT_FIELDS;
use crate::entity::catalog::{EnableStatus, ListingStatus, Sku};

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
    #[serde(default = "crate::entity::catalog::sku::legacy_listing_status")]
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

impl Default for SkuFilter {
    // SKU列表未传筛选时默认第 1 页、每页 20 条，不另加筛选条件。
    fn default() -> Self {
        let (page, page_size, sort_by, sort_ascending) = default_paging();
        Self {
            sku_no: None,
            ids: None,
            product_id: None,
            status: None,
            listing_status: None,
            page,
            page_size,
            sort_by,
            sort_ascending,
        }
    }
}

impl QueryFilter for SkuFilter {
    // 将SKU筛选条件编译为 MongoDB 查询，排除软删除记录。
    //
    // 始终写入未删除的 `deleted_at`。`sku_no` 有值时按字面量正则匹配；`ids` 为 `Some` 时写入 `id` 的 `$in`（空数组也写入）
    // ；`product_id` 有值时精确匹配；`status` 有值时按稳定代码精确匹配。`listing_status` 为 `Listed` 时匹配
    // `listed` 或 `null`，为 `Unlisted` 时只精确匹配 `unlisted`。分页与排序字段不进入条件。
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
    // 读取 SKU 列表的页码与单页条数，不做页码归一或条数限制。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// SKU 集合上的域查询。
#[allow(async_fn_in_trait)]
pub trait SkuRepositoryExt {
    /// 按稳定主键批量查询 SKU。MongoDB 查询或反序列化失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    ///
    /// `ids` 为 SKU 稳定 ID 集合。
    async fn find_by_ids(&self, ids: &[SkuId], executor: &mut dyn Executor) -> Result<Vec<Sku>>;

    /// 批量查询一组商品下的全部 SKU。MongoDB 查询或反序列化失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    ///
    /// `product_ids` 为商品稳定 ID 集合。
    async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>>;

    /// 批量读取采购责任解析或规则展示引用的 SKU。
    ///
    /// 采购责任展示入口的历史名称，语义与 [`Self::find_by_ids`] 完全一致；保留本方法以避免展示层调用方改名，仅委托属主批量查询，不重复实现查询。
    ///
    /// 返回全部匹配且未删除的 SKU；输入为空时返回空集合。MongoDB 查询或反序列化失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    ///
    /// `sku_ids` 为 SKU 稳定 ID 集合。
    async fn list_procurement_responsibility_skus(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>>;

    /// 判断采购责任规则引用的 SKU 是否存在。
    ///
    /// 只封装存在性事实，不判断规则引用合法性。
    ///
    /// 存在且未删除时返回 `true`。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    async fn has_procurement_responsibility_sku(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<bool>;

    /// 分页检索 SKU 列表（投影查询）。
    ///
    /// 只返回 [`SkuRow`] 所需的列表字段；排序字段白名单化（`created_at`/`sku_no`）。
    ///
    /// 返回当前页投影行与满足筛选条件的总数。当 MongoDB 查询、游标读取或计数失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    async fn search_skus(
        &self,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>>;
}

impl SkuRepositoryExt for persistence_core::Repository<'_, Sku> {
    async fn find_by_ids(&self, ids: &[SkuId], executor: &mut dyn Executor) -> Result<Vec<Sku>> {
        let Some(filter) = batch_ids_filter("id", ids) else {
            return Ok(Vec::new());
        };
        self.find_many(filter, executor).await
    }

    async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        let Some(filter) = batch_ids_filter("product_id", product_ids) else {
            return Ok(Vec::new());
        };
        self.find_many(filter, executor).await
    }

    async fn list_procurement_responsibility_skus(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        self.find_by_ids(sku_ids, executor).await
    }

    async fn has_procurement_responsibility_sku(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        Ok(self.find_by_id(sku_id.as_ref(), executor).await?.is_some())
    }

    async fn search_skus(
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
        super::shared::search_projected(&collection, &self.collection(), filter, options, executor).await
    }
}

// 构建 SKU 排序文档（白名单：`created_at`/`sku_no`）。返回排序文档；同一方向同时作用于排序字段与 `id`。
//
// - `sort_by`：请求的排序字段；不在白名单时回退 `created_at`。
// - `sort_ascending`：升序为 `true`，降序为 `false`。
pub(super) fn sku_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(whitelisted_sort(sort_by, SKU_SORT_FIELDS), sort_ascending)
}

// SKU 列表投影字段。返回包含身份、状态、当前修订指针、版本与创建时间的投影文档。
pub(super) fn sku_projection() -> Document {
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
