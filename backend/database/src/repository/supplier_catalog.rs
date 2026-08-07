//! 域 D24 `supplier_catalog` 仓储：supplier_catalog_product(+_revision、_revision_media)、
//! supplier_catalog_sku(+_revision)、supplier_product_mapping、supplier_catalog_intake_batch
//! (+_item)、supplier_offering(+_revision)（数据模型 §6.14，页面 W21）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：`update`/
//! `soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `indexes::supplier_catalog` 导入。
//!
//! 稳定身份（SPU/SKU/供给）可软删除与恢复；修订/图文/映射/入库批次是
//! 不可变或处理类集合，**不提供软删除方法**。

use entities::ids::{
    SkuId, SupplierAccountId, SupplierCatalogIntakeBatchId, SupplierCatalogProductId,
    SupplierCatalogProductRevisionId, SupplierCatalogSkuId, SupplierOfferingId,
};
use entities::supplier_catalog::{
    CatalogItemStatus, CatalogSourceType, IntakeBatchStatus, IntakeItemResult, MappingStatus, OfferingStatus,
    SupplierCatalogIntakeBatch, SupplierCatalogIntakeItem, SupplierCatalogProduct,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionMedia, SupplierCatalogSku,
    SupplierCatalogSkuRevision, SupplierOffering, SupplierOfferingRevision, SupplierProductMapping,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SupplierCatalogExt;
use super::regex_filter::insert_literal_regex_filter;
use crate::executor::Executor;
use crate::repository::{PageResult, Pagination, QueryFilter};
use crate::{mongo_ops, Repository, Result};

/// `supplier_catalog_product` 集合名（单一来源：`SupplierCatalogExt` 关联常量）。
const PRODUCTS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCTS;
/// `supplier_catalog_product_revision` 集合名。
const PRODUCT_REVISIONS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_PRODUCT_REVISIONS;
/// `supplier_catalog_sku` 集合名。
const SKUS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKUS;
/// `supplier_catalog_sku_revision` 集合名。
const SKU_REVISIONS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_SKU_REVISIONS;
/// `supplier_catalog_intake_batch` 集合名。
const INTAKE_BATCHES: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_BATCHES;
/// `supplier_catalog_intake_item` 集合名。
const INTAKE_ITEMS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_CATALOG_INTAKE_ITEMS;
/// `supplier_offering` 集合名。
const OFFERINGS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_OFFERINGS;
/// `supplier_offering_revision` 集合名。
const OFFERING_REVISIONS: &str = <mongodb::Database as SupplierCatalogExt>::SUPPLIER_OFFERING_REVISIONS;

/// `supplier_catalog_product` 列表允许的排序字段白名单。
const PRODUCT_SORT_FIELDS: &[&str] = &["created_at", "supplier_spu_code", "status"];
/// `supplier_catalog_sku` 列表允许的排序字段白名单。
const SKU_SORT_FIELDS: &[&str] = &["created_at", "supplier_sku_code", "status"];
/// `supplier_product_mapping` 列表允许的排序字段白名单。
const MAPPING_SORT_FIELDS: &[&str] = &["created_at", "status"];
/// `supplier_catalog_intake_batch` 列表允许的排序字段白名单。
const INTAKE_SORT_FIELDS: &[&str] = &["created_at", "status"];
/// `supplier_offering` 列表允许的排序字段白名单。
const OFFERING_SORT_FIELDS: &[&str] = &["created_at", "status"];

/// 供应商 SPU 列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogProductRow {
    /// 实体主键。
    pub id: String,
    /// 来源供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// API 连接。
    pub source_connection_id: Option<entities::ids::SupplierApiConnectionId>,
    /// 供应商 SPU 编码。
    pub supplier_spu_code: String,
    /// 条目状态。
    pub status: CatalogItemStatus,
    /// 当前来源修订。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商 SPU 列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCatalogProductFilter {
    /// 来源供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 来源类型；`None` 表示不筛选。
    pub source_type: Option<CatalogSourceType>,
    /// 条目状态；`None` 表示不筛选。
    pub status: Option<CatalogItemStatus>,
    /// SPU 编码模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub supplier_spu_code: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCatalogProductFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_literal_regex_filter(
            &mut filter,
            "supplier_spu_code",
            self.supplier_spu_code.as_deref(),
        );
        filter
    }
}

impl Pagination for SupplierCatalogProductFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierCatalogProduct> {
    /// 分页检索供应商 SPU 列表（投影查询）。
    ///
    /// 只返回 [`SupplierCatalogProductRow`] 所需的列表字段，不加载整文档；
    /// 排序字段经白名单校验（`created_at`/`supplier_spu_code`/`status`）。
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
    pub async fn search_supplier_catalog_products(
        &self,
        filter: &SupplierCatalogProductFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierCatalogProductRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                PRODUCT_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_catalog_product_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierCatalogProductRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「供应商 + SPU 编码」查找唯一 SPU 身份。
    ///
    /// 唯一性由 `uk_supplier_catalog_products_supplier_code` 唯一索引保证；
    /// 本方法用于编码查重与身份取回，服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `supplier_id` - 来源供应商
    /// * `supplier_spu_code` - 供应商 SPU 编码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除 SPU；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_supplier_code(
        &self,
        supplier_id: &SupplierAccountId,
        supplier_spu_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCatalogProduct>> {
        self.find_one(
            doc! {
                "supplier_id": supplier_id.to_string(),
                "supplier_spu_code": supplier_spu_code,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierCatalogProductRevision> {
    /// 批量取回多个 SPU 的全部来源修订（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `product_ids` - 供应商 SPU ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的来源修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_revisions_by_product_ids(
        &self,
        product_ids: &[SupplierCatalogProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCatalogProductRevision>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_catalog_product_id",
                product_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierCatalogProductRevisionMedia> {
    /// 批量取回多个来源修订的全部图文（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `revision_ids` - 来源修订 ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的来源图文。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_media_by_revision_ids(
        &self,
        revision_ids: &[SupplierCatalogProductRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCatalogProductRevisionMedia>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_catalog_product_revision_id",
                revision_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

/// 供应商 SKU 列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogSkuRow {
    /// 实体主键。
    pub id: String,
    /// 所属供应商 SPU。
    pub supplier_catalog_product_id: SupplierCatalogProductId,
    /// 供应商 SKU 编码。
    pub supplier_sku_code: String,
    /// 条目状态。
    pub status: CatalogItemStatus,
    /// 当前来源 SKU 修订。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商 SKU 列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCatalogSkuFilter {
    /// 所属供应商 SPU；`None` 表示不筛选。
    pub supplier_catalog_product_id: Option<SupplierCatalogProductId>,
    /// 条目状态；`None` 表示不筛选。
    pub status: Option<CatalogItemStatus>,
    /// SKU 编码模糊匹配（字面量、忽略大小写）；`None` 表示不筛选。
    pub supplier_sku_code: Option<String>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCatalogSkuFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(product_id) = &self.supplier_catalog_product_id {
            filter.insert("supplier_catalog_product_id", product_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        insert_literal_regex_filter(
            &mut filter,
            "supplier_sku_code",
            self.supplier_sku_code.as_deref(),
        );
        filter
    }
}

impl Pagination for SupplierCatalogSkuFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierCatalogSku> {
    /// 分页检索供应商 SKU 列表（投影查询）。
    ///
    /// 只返回 [`SupplierCatalogSkuRow`] 所需的列表字段；排序字段经白名单校验
    /// （`created_at`/`supplier_sku_code`/`status`）。
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
    pub async fn search_supplier_catalog_skus(
        &self,
        filter: &SupplierCatalogSkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierCatalogSkuRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                SKU_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_catalog_sku_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierCatalogSkuRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「所属 SPU + SKU 编码」查找唯一 SKU 身份。
    ///
    /// 唯一性由 `uk_supplier_catalog_skus_product_code` 唯一索引保证。
    ///
    /// # 参数
    /// * `supplier_catalog_product_id` - 所属供应商 SPU
    /// * `supplier_sku_code` - 供应商 SKU 编码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_product_and_code(
        &self,
        supplier_catalog_product_id: &SupplierCatalogProductId,
        supplier_sku_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCatalogSku>> {
        self.find_one(
            doc! {
                "supplier_catalog_product_id": supplier_catalog_product_id.to_string(),
                "supplier_sku_code": supplier_sku_code,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierCatalogSkuRevision> {
    /// 批量取回多个 SKU 的全部来源修订（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `sku_ids` - 供应商 SKU ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的来源 SKU 修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_revisions_by_sku_ids(
        &self,
        sku_ids: &[SupplierCatalogSkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCatalogSkuRevision>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("supplier_catalog_sku_id", sku_ids.iter().map(|id| id.to_string())),
            executor,
        )
        .await
    }
}

/// 供应商 SKU → 公司 SKU 映射列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierProductMappingRow {
    /// 实体主键。
    pub id: String,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 映射状态。
    pub status: MappingStatus,
    /// 审核人。
    pub approved_by: Option<String>,
    /// 审核时间。
    pub approved_at: Option<entities::common::time::Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供应商 SKU → 公司 SKU 映射列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierProductMappingFilter {
    /// 供应商 SKU；`None` 表示不筛选。
    pub supplier_catalog_sku_id: Option<SupplierCatalogSkuId>,
    /// ERP SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 映射状态；`None` 表示不筛选。
    pub status: Option<MappingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierProductMappingFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_catalog_sku_id) = &self.supplier_catalog_sku_id {
            filter.insert("supplier_catalog_sku_id", supplier_catalog_sku_id.to_string());
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierProductMappingFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierProductMapping> {
    /// 分页检索映射列表（投影查询）。
    ///
    /// 只返回 [`SupplierProductMappingRow`] 所需的列表字段；排序字段经白名单
    /// 校验（`created_at`/`status`）。
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
    pub async fn search_supplier_product_mappings(
        &self,
        filter: &SupplierProductMappingFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierProductMappingRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                MAPPING_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_product_mapping_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierProductMappingRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 取回指定供应商 SKU 的生效映射。
    ///
    /// 同一时点同一供应商 SKU 最多一个生效映射（§6.14），由部分唯一索引
    /// `uk_supplier_product_mappings_active_sku` 保证。
    ///
    /// # 参数
    /// * `supplier_catalog_sku_id` - 供应商 SKU
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的生效映射；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_active_by_supplier_sku(
        &self,
        supplier_catalog_sku_id: &SupplierCatalogSkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierProductMapping>> {
        self.find_one(
            doc! {
                "supplier_catalog_sku_id": supplier_catalog_sku_id.to_string(),
                "status": MappingStatus::Active.as_str(),
            },
            executor,
        )
        .await
    }
}

/// 来源入库批次列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierCatalogIntakeBatchRow {
    /// 实体主键。
    pub id: String,
    /// 来源类型。
    pub source_type: CatalogSourceType,
    /// 来源供应商。
    pub supplier_id: SupplierAccountId,
    /// 来源引用。
    pub source_reference: String,
    /// 批次状态。
    pub status: IntakeBatchStatus,
    /// 批次级错误说明。
    pub error_text: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 来源入库批次列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCatalogIntakeBatchFilter {
    /// 来源类型；`None` 表示不筛选。
    pub source_type: Option<CatalogSourceType>,
    /// 来源供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 批次状态；`None` 表示不筛选。
    pub status: Option<IntakeBatchStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCatalogIntakeBatchFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierCatalogIntakeBatchFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierCatalogIntakeBatch> {
    /// 分页检索入库批次列表（投影查询）。
    ///
    /// 只返回 [`SupplierCatalogIntakeBatchRow`] 所需的列表字段；排序字段经
    /// 白名单校验（`created_at`/`status`）。
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
    pub async fn search_supplier_catalog_intake_batches(
        &self,
        filter: &SupplierCatalogIntakeBatchFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierCatalogIntakeBatchRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                INTAKE_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_catalog_intake_batch_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SupplierCatalogIntakeBatchRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「来源类型 + 供应商 + 来源引用」查找唯一批次。
    ///
    /// 唯一性由 `uk_supplier_catalog_intake_batches_source_key` 唯一索引保证；
    /// 用于来源批次幂等（同一来源引用重复同步不产生第二条）。
    ///
    /// # 参数
    /// * `source_type` - 来源类型
    /// * `supplier_id` - 来源供应商
    /// * `source_reference` - 来源引用
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的批次；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_source_key(
        &self,
        source_type: CatalogSourceType,
        supplier_id: &SupplierAccountId,
        source_reference: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierCatalogIntakeBatch>> {
        self.find_one(
            doc! {
                "source_type": source_type.as_str(),
                "supplier_id": supplier_id.to_string(),
                "source_reference": source_reference,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierCatalogIntakeItem> {
    /// 批量取回多个批次的全部入库明细（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `batch_ids` - 入库批次 ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的入库明细。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_items_by_batch_ids(
        &self,
        batch_ids: &[SupplierCatalogIntakeBatchId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCatalogIntakeItem>> {
        if batch_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_catalog_intake_batch_id",
                batch_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }

    /// 统计批次内失败明细数量。
    ///
    /// 用于批次完成后的处理结果摘要；失败明细的 `error_text` 已由实体强制必填。
    ///
    /// # 参数
    /// * `batch_id` - 入库批次
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该批次内 `FAILED` 明细数量。
    ///
    /// # 错误
    /// 当 MongoDB 统计失败时返回错误。
    pub async fn count_failed_items_by_batch_id(
        &self,
        batch_id: &SupplierCatalogIntakeBatchId,
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        mongo_ops::count_documents(
            &self.collection(),
            doc! {
                "supplier_catalog_intake_batch_id": batch_id.to_string(),
                "result": IntakeItemResult::Failed.as_str(),
            },
            executor,
        )
        .await
    }
}

/// 供给稳定身份列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingRow {
    /// 实体主键。
    pub id: String,
    /// ERP SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商 SKU。
    pub supplier_catalog_sku_id: SupplierCatalogSkuId,
    /// 供给状态。
    pub status: OfferingStatus,
    /// 当前供给版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 供给稳定身份列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierOfferingFilter {
    /// ERP SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 供应商；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 供给状态；`None` 表示不筛选。
    pub status: Option<OfferingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单内取值，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierOfferingFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierOfferingFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierOffering> {
    /// 分页检索供给列表（投影查询）。
    ///
    /// 只返回 [`SupplierOfferingRow`] 所需的列表字段；排序字段经白名单校验
    /// （`created_at`/`status`）。
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
    pub async fn search_supplier_offerings(
        &self,
        filter: &SupplierOfferingFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierOfferingRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                OFFERING_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_offering_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierOfferingRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按 ERP SKU 批量取回供给（`$in`，禁止 N+1）。
    ///
    /// §6.14「W14 仅消费按 `sku_id` 查询的关联投影」：一次取回多 SKU 的全部
    /// 有效供给关系；空集合直接返回空结果。
    ///
    /// # 参数
    /// * `sku_ids` - ERP SKU ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的供给身份。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOffering>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("sku_id", sku_ids.iter().map(|id| id.to_string())),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierOfferingRevision> {
    /// 批量取回多个供给的全部供给修订（`$in`，禁止 N+1）。
    ///
    /// # 参数
    /// * `offering_ids` - 供给 ID 集合（空集合直接返回空结果）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的供给修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_revisions_by_offering_ids(
        &self,
        offering_ids: &[SupplierOfferingId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingRevision>> {
        if offering_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_offering_id",
                offering_ids.iter().map(|id| id.to_string()),
            ),
            executor,
        )
        .await
    }
}

/// D24 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierCatalogExt::supplier_catalog()` 访问。
pub struct SupplierCatalogRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierCatalogRepository<'a> {
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

    /// 创建供应商 SPU 稳定身份及首个来源修订。
    ///
    /// 首次创建先写稳定身份，再写不可变修订；稳定身份在写入前指向该首版。
    /// 调用方必须传入事务执行器以保证两笔写入原子提交。
    ///
    /// # 参数
    /// * `product` - 尚未持久化的供应商 SPU
    /// * `revision` - 该 SPU 的首个来源修订
    /// * `executor` - 事务执行器
    ///
    /// # 返回
    /// 两笔写入均成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或底层写入失败时返回错误。
    pub async fn create_product_with_initial_revision(
        &self,
        product: &SupplierCatalogProduct,
        revision: &SupplierCatalogProductRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut product = product.clone();
        product.stable.current_revision_id = Some(revision.base.id.clone());
        Repository::new(self.db, PRODUCTS)
            .create(&product, executor)
            .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierCatalogProductRevision>(PRODUCT_REVISIONS),
            revision,
            executor,
        )
        .await
    }

    /// 为既有供应商 SPU 追加来源修订并推进当前修订指针。
    ///
    /// 先写入 `supplier_catalog_product_revision`，再乐观锁更新
    /// `supplier_catalog_product`（当前修订指针等，§6.14「详情即编辑」保存只
    /// 追加来源修订，必须带期望来源修订号），保证「稳定身份 + 修订」原子可见。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 修订自动提交、稳定表 CAS 失败会留下只有修订没有指针的半成品；Service
    /// 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `product` - 已执行内容更新的 SPU 稳定身份（带期望版本）
    /// * `revision` - 待写入的来源修订
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当修订号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、稳定表版本
    /// 冲突（[`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn append_product_revision(
        &self,
        product: &mut SupplierCatalogProduct,
        revision: &SupplierCatalogProductRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierCatalogProductRevision>(PRODUCT_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Repository::new(self.db, PRODUCTS)
            .update(product, executor)
            .await?;
        Ok(())
    }

    /// 创建供应商 SKU 稳定身份及首个来源修订。
    ///
    /// 首次创建插入稳定身份，不能复用追加修订路径对未持久化实体执行乐观锁
    /// 更新。调用方必须传入事务执行器以保证两笔写入原子提交。
    ///
    /// # 参数
    /// * `sku` - 尚未持久化的供应商 SKU
    /// * `revision` - 该 SKU 的首个来源修订
    /// * `executor` - 事务执行器
    ///
    /// # 返回
    /// 两笔写入均成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或底层写入失败时返回错误。
    pub async fn create_sku_with_initial_revision(
        &self,
        sku: &SupplierCatalogSku,
        revision: &SupplierCatalogSkuRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let mut sku = sku.clone();
        sku.stable.current_revision_id = Some(revision.base.id.clone());
        Repository::new(self.db, SKUS).create(&sku, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<SupplierCatalogSkuRevision>(SKU_REVISIONS),
            revision,
            executor,
        )
        .await
    }

    /// 为既有供应商 SKU 追加来源修订并推进当前修订指针。
    ///
    /// 先写入 `supplier_catalog_sku_revision`，再乐观锁更新 `supplier_catalog_sku`
    /// （当前修订指针），保证「稳定身份 + 修订」原子可见。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下只有修订没有指针的半成品。
    ///
    /// # 参数
    /// * `sku` - 已执行内容更新的 SKU 稳定身份（带期望版本）
    /// * `revision` - 待写入的来源 SKU 修订
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当修订号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、稳定表版本
    /// 冲突（[`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn append_sku_revision(
        &self,
        sku: &mut SupplierCatalogSku,
        revision: &SupplierCatalogSkuRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierCatalogSkuRevision>(SKU_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Repository::new(self.db, SKUS).update(sku, executor).await?;
        Ok(())
    }

    /// 创建来源入库批次及全部明细（跨集合多步骤写入）。
    ///
    /// 依次写入 `supplier_catalog_intake_batch` 与 `supplier_catalog_intake_item`
    /// （批量），保证「批次 + 明细」原子可见（§6.14 处理语义）。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下只有批次没有明细的半成品。
    ///
    /// # 参数
    /// * `batch` - 待写入的入库批次
    /// * `items` - 待写入的入库明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当来源键唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB
    /// 写入失败时返回错误。
    pub async fn create_intake_batch(
        &self,
        batch: &SupplierCatalogIntakeBatch,
        items: &[SupplierCatalogIntakeItem],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierCatalogIntakeBatch>(INTAKE_BATCHES),
            batch,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<SupplierCatalogIntakeItem>(INTAKE_ITEMS),
            items.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 创建供给稳定身份及其首个供给修订（跨集合多步骤写入）。
    ///
    /// 先写入 `supplier_offering_revision`，再乐观锁更新 `supplier_offering`
    /// （当前修订指针），保证「稳定身份 + 修订」原子可见（§6.14）。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下只有修订没有指针的半成品。
    ///
    /// # 参数
    /// * `offering` - 已执行内容更新的供给稳定身份（带期望版本）
    /// * `revision` - 待写入的供给修订
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当修订号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、稳定表版本
    /// 冲突（[`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn create_offering_with_revision(
        &self,
        offering: &mut SupplierOffering,
        revision: &SupplierOfferingRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOfferingRevision>(OFFERING_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Repository::new(self.db, OFFERINGS)
            .update(offering, executor)
            .await?;
        Ok(())
    }
}

/// 构建白名单校验后的排序文档。
///
/// 排序字段必须落在白名单内，未知字段一律回退 `created_at`（§2.3 禁止透传
/// 任意字段名）；`None` 默认 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段
/// * `whitelist` - 允许的排序字段集合
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, whitelist: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| whitelist.contains(field))
        .unwrap_or("created_at");
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 构建 `$in` 批量查询过滤（批量取回，禁止 N+1）。
///
/// # 参数
/// * `field` - 匹配字段名
/// * `values` - 待匹配的 ID 字符串集合
///
/// # 返回
/// 返回批量查询条件文档。
fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values: Vec<mongodb::bson::Bson> = values.into_iter().map(mongodb::bson::Bson::String).collect();
    doc! { field: { "$in": values } }
}

/// 供应商 SPU 列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_catalog_product_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_id": 1,
        "source_type": 1,
        "source_connection_id": 1,
        "supplier_spu_code": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供应商 SKU 列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_catalog_sku_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_catalog_product_id": 1,
        "supplier_sku_code": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 映射列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_product_mapping_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_catalog_sku_id": 1,
        "sku_id": 1,
        "status": 1,
        "approved_by": 1,
        "approved_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 入库批次列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_catalog_intake_batch_projection() -> Document {
    doc! {
        "id": 1,
        "source_type": 1,
        "supplier_id": 1,
        "source_reference": 1,
        "status": 1,
        "error_text": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 供给列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_offering_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "supplier_id": 1,
        "supplier_catalog_sku_id": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{
        sort_doc, QueryFilter, SupplierCatalogIntakeBatchFilter, SupplierCatalogProductFilter,
        SupplierCatalogSkuFilter, PRODUCT_SORT_FIELDS,
    };
    use entities::ids::SupplierAccountId;
    use entities::supplier_catalog::{CatalogItemStatus, CatalogSourceType, IntakeBatchStatus};

    #[test]
    fn product_filter_applies_fields_regex_and_deleted_filter() {
        let filter = SupplierCatalogProductFilter {
            supplier_id: Some(SupplierAccountId::new("sup-1")),
            source_type: Some(CatalogSourceType::Excel),
            status: Some(CatalogItemStatus::Active),
            supplier_spu_code: Some("SPU-001".to_string()),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("supplier_id").unwrap(), "sup-1");
        assert_eq!(document.get_str("source_type").unwrap(), "EXCEL");
        assert_eq!(document.get_str("status").unwrap(), "ACTIVE");
        let regex = document.get_document("supplier_spu_code").unwrap();
        assert_eq!(
            regex.get_str("$regex").unwrap(),
            "SPU\\-001",
            "正则必须转义字面量"
        );
    }

    #[test]
    fn sku_and_batch_filters_omit_absent_fields() {
        let sku = SupplierCatalogSkuFilter {
            supplier_catalog_product_id: None,
            status: Some(CatalogItemStatus::Stopped),
            supplier_sku_code: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        assert_eq!(sku.to_doc(), doc! { "deleted_at": 0i64, "status": "STOPPED" });

        let batch = SupplierCatalogIntakeBatchFilter {
            source_type: None,
            supplier_id: None,
            status: Some(IntakeBatchStatus::Failed),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };
        assert_eq!(batch.to_doc(), doc! { "deleted_at": 0i64, "status": "FAILED" });
    }

    #[test]
    fn sort_doc_respects_whitelist_and_rejects_unknown_fields() {
        assert_eq!(
            sort_doc(None, PRODUCT_SORT_FIELDS, false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("supplier_spu_code"), PRODUCT_SORT_FIELDS, true),
            doc! { "supplier_spu_code": 1 }
        );
        assert_eq!(
            sort_doc(Some("arbitrary_field"), PRODUCT_SORT_FIELDS, true),
            doc! { "created_at": 1 },
            "未知排序字段必须回退 created_at"
        );
    }
}
