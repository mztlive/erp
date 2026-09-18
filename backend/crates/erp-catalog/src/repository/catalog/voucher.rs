use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{ProductId, SkuId};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result};
use serde::{Deserialize, Serialize};

use super::CatalogRepository;
use super::shared::{in_filter, max_revision_no, sort_doc, whitelisted_sort};
use crate::dto::catalog::VOUCHER_PROFILE_SORT_FIELDS;
use crate::entity::catalog::{EnableStatus, Product, Sku, VoucherCategoryProfileRevision};
use crate::repository::CatalogExt;

/// 卡券类目扩展修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct VoucherCategoryProfileRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 卡券类目使用的 VOUCHER SKU 稳定身份。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 卡券类目描述。
    pub description: String,
    /// 关联 SKU 编号；关系批量装配失败或 SKU 缺失时为空。
    #[serde(default)]
    pub sku_no: Option<String>,
    /// 关联商品稳定 ID。
    #[serde(default)]
    pub product_id: Option<String>,
    /// 关联商品乐观锁版本。
    #[serde(default)]
    pub product_version: Option<u64>,
    /// 优先取商品当前修订名称，其次取 SKU 当前修订名称，最后回退描述。
    #[serde(default)]
    pub name: Option<String>,
    /// 启停状态。
    pub status: EnableStatus,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl VoucherCategoryProfileRevisionRow {
    /// 以稳定身份构造投影行，关联展示字段保持空条件。
    ///
    /// # 参数
    /// * `id` - 实体主键
    /// * `sku_id` - 卡券类目使用的 VOUCHER SKU 稳定身份
    /// * `description` - 卡券类目描述
    ///
    /// # 返回
    /// 返回待装配关联展示字段的投影行。
    ///
    /// # 错误
    /// 无。
    pub fn new(id: impl Into<String>, sku_id: impl Into<String>, description: impl Into<String>) -> Self {
        Self {
            id: id.into(),
            sku_id: sku_id.into(),
            revision_no: 0,
            description: description.into(),
            sku_no: None,
            product_id: None,
            product_version: None,
            name: None,
            status: EnableStatus::Active,
            version: 0,
            created_at: 0,
        }
    }
}

/// 卡券类目扩展修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct VoucherCategoryProfileRevisionFilter {
    /// 卡券类目 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
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

impl QueryFilter for VoucherCategoryProfileRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for VoucherCategoryProfileRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 卡券类目扩展修订集合上的域查询。
#[allow(async_fn_in_trait)]
pub trait VoucherCategoryProfileRevisionRepositoryExt {
    /// 分页检索卡券类目扩展修订列表（投影查询）。
    ///
    /// 只返回 [`VoucherCategoryProfileRevisionRow`] 所需的列表字段；排序字段
    /// 白名单化（`created_at`/`revision_no`）。
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
    async fn search_voucher_category_profile_revisions(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>>;

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
    ///
    /// # 约束
    /// 仅查询本仓储拥有的卡券类目扩展修订集合，按 SKU 引用过滤，不访问 SKU 集合。
    async fn find_active_by_sku(
        &self,
        sku_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<VoucherCategoryProfileRevision>>;
}

impl VoucherCategoryProfileRevisionRepositoryExt
    for persistence_core::Repository<'_, VoucherCategoryProfileRevision>
{
    async fn search_voucher_category_profile_revisions(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>> {
        let options = FindOptions::builder()
            .sort(voucher_revision_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(voucher_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<VoucherCategoryProfileRevisionRow>();
        super::shared::search_projected(&collection, &self.collection(), filter, options, executor).await
    }

    async fn find_active_by_sku(
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

impl<'a> CatalogRepository<'a> {
    /// 分页查询卡券类目扩展修订并批量装配 SKU、商品和当前名称关系。
    ///
    /// # 参数
    /// * `filter` - 卡券类目修订、状态、分页与排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已补齐关联展示字段的卡券类目修订分页结果。
    ///
    /// # 错误
    /// MongoDB 查询、计数或批量关系装配失败时返回错误。
    pub async fn voucher_profile_page(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>> {
        let mut result = self
            .db
            .voucher_category_profile_revisions()
            .search_voucher_category_profile_revisions(filter, executor)
            .await?;
        self.attach_voucher_profile_context(&mut result.items, executor).await?;
        Ok(result)
    }

    /// 装配单个卡券类目扩展修订的关联展示上下文。
    ///
    /// # 参数
    /// * `revision` - 已写入的卡券类目扩展修订实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已补齐 SKU、商品和当前名称关系的投影行。
    ///
    /// # 错误
    /// MongoDB 批量关系查询或反序列化失败时返回错误。
    pub async fn voucher_profile(
        &self,
        revision: &VoucherCategoryProfileRevision,
        executor: &mut dyn Executor,
    ) -> Result<VoucherCategoryProfileRevisionRow> {
        let mut row = VoucherCategoryProfileRevisionRow::new(
            revision.base.id.clone(),
            revision.sku_id.to_string(),
            revision.description.clone(),
        );
        row.revision_no = revision.revision.revision_no;
        row.status = revision.status;
        row.version = revision.base.version;
        row.created_at = revision.base.created_at;
        let mut rows = vec![row];
        self.attach_voucher_profile_context(&mut rows, executor).await?;
        Ok(rows.remove(0))
    }

    /// 读取指定卡券类目 SKU 的历史最大扩展修订序号。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回历史最大修订号；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn latest_voucher_profile_revision_no(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        max_revision_no(
            &self.db.voucher_category_profile_revisions().collection().clone_with_type(),
            doc! { "sku_id": sku_id.to_string() },
            executor,
        )
        .await
    }

    /// 解析指定卡券类目 SKU 的当前扩展修订。
    ///
    /// 卡券扩展表没有稳定主表指针，因此当前修订按最大修订号确定。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最大修订号对应的扩展修订；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_voucher_profile_revision(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<VoucherCategoryProfileRevision>> {
        Ok(self
            .db
            .voucher_category_profile_revisions()
            .find_many(doc! { "sku_id": sku_id.to_string() }, executor)
            .await?
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no))
    }

    /// 批量装配卡券类目修订的 SKU、商品和当前名称关系。
    ///
    /// # 参数
    /// * `rows` - 当前页卡券类目修订投影
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 成功时原位填充每行关联展示字段；缺失关系保持兼容的空值。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    async fn attach_voucher_profile_context(
        &self,
        rows: &mut [VoucherCategoryProfileRevisionRow],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let skus = self.voucher_profile_skus(rows, executor).await?;
        let products = self.voucher_profile_products(&skus, executor).await?;
        let product_revisions = self.current_product_revisions(&products, executor).await?;
        let sku_revisions = self.current_sku_revisions(&skus, executor).await?;
        let sku_by_id = skus.into_iter().map(|sku| (sku.base.id.clone(), sku)).collect::<HashMap<_, _>>();
        let product_by_id =
            products.into_iter().map(|product| (product.base.id.clone(), product)).collect::<HashMap<_, _>>();
        for row in rows {
            attach_voucher_row(row, &sku_by_id, &product_by_id, &product_revisions, &sku_revisions);
        }
        Ok(())
    }

    /// 批量读取卡券类目修订关联的 SKU。
    ///
    /// # 参数
    /// * `rows` - 当前页卡券类目修订投影
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除 SKU。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn voucher_profile_skus(
        &self,
        rows: &[VoucherCategoryProfileRevisionRow],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        let sku_ids = rows.iter().map(|row| row.sku_id.clone()).collect::<Vec<_>>();
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db.skus().find_many(in_filter("id", sku_ids), executor).await
    }

    /// 批量读取卡券类目 SKU 所属商品。
    ///
    /// # 参数
    /// * `skus` - 当前页关联 SKU
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部命中的未删除商品。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn voucher_profile_products(
        &self,
        skus: &[Sku],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Product>> {
        let product_ids =
            skus.iter().map(|sku| ProductId::new(sku.product_id.to_string())).collect::<Vec<_>>();
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .products()
            .find_many(in_filter("id", product_ids.into_iter().map(|id| id.to_string())), executor)
            .await
    }
}

/// 把批量关系快照装配到单条卡券类目修订投影。
///
/// # 参数
/// * `row` - 待填充的卡券类目修订投影
/// * `sku_by_id` / `product_by_id` - 稳定实体映射
/// * `product_revisions` / `sku_revisions` - 当前修订映射
///
/// # 返回
/// 无返回值；缺失 SKU 时保持全部关联字段为空，命中 SKU 后名称最终回退到描述。
///
/// # 错误
/// 无。
fn attach_voucher_row(
    row: &mut VoucherCategoryProfileRevisionRow,
    sku_by_id: &HashMap<String, Sku>,
    product_by_id: &HashMap<String, Product>,
    product_revisions: &HashMap<String, crate::entity::catalog::ProductRevision>,
    sku_revisions: &HashMap<String, crate::entity::catalog::SkuRevision>,
) {
    let Some(sku) = sku_by_id.get(&row.sku_id) else {
        return;
    };
    row.sku_no = Some(sku.sku_no.clone());
    row.product_id = Some(sku.product_id.to_string());
    if let Some(product) = product_by_id.get(sku.product_id.as_ref()) {
        row.product_version = Some(product.base.version);
        row.name = product_revisions.get(&product.base.id).map(|revision| revision.name.clone());
    }
    if row.name.is_none() {
        row.name = sku_revisions.get(&sku.base.id).map(|revision| revision.name.clone());
    }
    if row.name.is_none() {
        row.name = Some(row.description.clone());
    }
}

/// 构建卡券类目修订排序文档（白名单：`created_at`/`revision_no`）。
fn voucher_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(whitelisted_sort(sort_by, VOUCHER_PROFILE_SORT_FIELDS), sort_ascending)
}

/// 卡券类目扩展修订列表投影字段。
fn voucher_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "description": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::VoucherCategoryProfileRevisionRow;
    use crate::entity::catalog::EnableStatus;

    #[test]
    fn voucher_row_new_carries_identity_fields() {
        let row = VoucherCategoryProfileRevisionRow::new("rev-1", "sku-1", "类目描述");
        assert_eq!(row.id, "rev-1");
        assert_eq!(row.sku_id, "sku-1");
        assert_eq!(row.description, "类目描述");
        assert_eq!(row.sku_no, None);
        assert_eq!(row.status, EnableStatus::Active);
    }

    #[test]
    fn voucher_row_new_boundary_empty_description() {
        let row = VoucherCategoryProfileRevisionRow::new("rev-2", "sku-2", "");
        assert_eq!(row.description, "");
        assert_eq!(row.revision_no, 0);
    }
}
