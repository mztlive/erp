use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, Product, Sku, VoucherCategoryProfileRevision};
use entities::ids::{ProductId, SkuId};

use super::super::extensions::CatalogExt;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{in_filter, sort_doc};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

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

impl<'a> Repository<'a, VoucherCategoryProfileRevision> {
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
    pub async fn search_voucher_category_profile_revisions(
        &self,
        filter: &VoucherCategoryProfileRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<VoucherCategoryProfileRevisionRow>> {
        let options = FindOptions::builder()
            .sort(voucher_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(voucher_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<VoucherCategoryProfileRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
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
        self.attach_voucher_profile_context(&mut result.items, executor)
            .await?;
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
        let mut rows = vec![VoucherCategoryProfileRevisionRow {
            id: revision.base.id.clone(),
            sku_id: revision.sku_id.to_string(),
            revision_no: revision.revision.revision_no,
            description: revision.description.clone(),
            sku_no: None,
            product_id: None,
            product_version: None,
            name: None,
            status: revision.status,
            version: revision.base.version,
            created_at: revision.base.created_at,
        }];
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
        let revisions = self
            .db
            .voucher_category_profile_revisions()
            .find_many(doc! { "sku_id": sku_id.to_string() }, executor)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max())
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
        let sku_by_id = skus
            .into_iter()
            .map(|sku| (sku.base.id.clone(), sku))
            .collect::<HashMap<_, _>>();
        let product_by_id = products
            .into_iter()
            .map(|product| (product.base.id.clone(), product))
            .collect::<HashMap<_, _>>();
        for row in rows {
            attach_voucher_row(
                row,
                &sku_by_id,
                &product_by_id,
                &product_revisions,
                &sku_revisions,
            );
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
        let product_ids = skus
            .iter()
            .map(|sku| ProductId::new(sku.product_id.to_string()))
            .collect::<Vec<_>>();
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.db
            .products()
            .find_many(
                in_filter("id", product_ids.into_iter().map(|id| id.to_string())),
                executor,
            )
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
    product_revisions: &HashMap<String, entities::catalog::ProductRevision>,
    sku_revisions: &HashMap<String, entities::catalog::SkuRevision>,
) {
    let Some(sku) = sku_by_id.get(&row.sku_id) else {
        return;
    };
    row.sku_no = Some(sku.sku_no.clone());
    row.product_id = Some(sku.product_id.to_string());
    if let Some(product) = product_by_id.get(sku.product_id.as_ref()) {
        row.product_version = Some(product.base.version);
        row.name = product_revisions
            .get(&product.base.id)
            .map(|revision| revision.name.clone());
    }
    if row.name.is_none() {
        row.name = sku_revisions
            .get(&sku.base.id)
            .map(|revision| revision.name.clone());
    }
    if row.name.is_none() {
        row.name = Some(row.description.clone());
    }
}

/// 构建卡券类目修订排序文档（白名单：`created_at`/`revision_no`）。
fn voucher_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
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
