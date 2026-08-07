//! 域 D24 `supplier_catalog` 服务编排（页面：W21）。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 供应商商品入库（§8.4 第 1 条）：批次 + 明细 + SPU/SKU 与来源修订原子写入；
//! - 「详情即编辑」保存只追加来源修订（稳定身份指针 + 修订 + 媒体 + SKU 修订）；
//! - 入池确认（映射 `Active` + 双价供给修订）原子写入；
//! - 供给改价/暂停/停止追加新供给修订。
//!
//! 幂等：入库批次以 `(source_type, supplier_id, source_reference)` 唯一索引去重，
//! 重复提交返回首次结果（§8.4 第 1 条「先按批次和来源身份幂等写」）。
//!
//! 跨域协作（只经 DatabaseExt 调对方 Repository，禁止 Service 依赖 Service）：
//! - D10 `catalog`：公司 SKU 存在性校验（映射入池）；
//! - D09 `supplier`：供应商角色存在性校验。

use std::collections::HashMap;
use std::str::FromStr;

use database::{AccessControlExt, CatalogExt, NoTransaction, SupplierCatalogExt, SupplierExt, Transactional};
use entities::common::time::{BusinessDate, Instant};
use entities::ids::{
    SupplierCatalogIntakeBatchId, SupplierCatalogIntakeItemId, SupplierCatalogProductId,
    SupplierCatalogProductRevisionId, SupplierCatalogProductRevisionMediaId, SupplierCatalogSkuId,
    SupplierCatalogSkuRevisionId, SupplierOfferingId, SupplierOfferingRevisionId, SupplierProductMappingId,
};
use entities::money::{round_to_cent, Amount, Quantity, Rate, UnitPrice};
use entities::supplier_catalog::{
    ArchiveStatus, AvailabilityStatus, IntakeItemClassification, IntakeItemResult, MappingStatus,
    OfferingStatus, SupplierCatalogIntakeBatch, SupplierCatalogIntakeBatchData, SupplierCatalogIntakeItem,
    SupplierCatalogIntakeItemData, SupplierCatalogProduct, SupplierCatalogProductData,
    SupplierCatalogProductRevision, SupplierCatalogProductRevisionData, SupplierCatalogProductRevisionMedia,
    SupplierCatalogProductRevisionMediaData, SupplierCatalogSku, SupplierCatalogSkuData,
    SupplierCatalogSkuRevision, SupplierCatalogSkuRevisionData, SupplierOffering, SupplierOfferingData,
    SupplierOfferingRevision, SupplierOfferingRevisionData, SupplierProductMapping,
    SupplierProductMappingData,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::query::{normalized_text, page_or_default, page_size_or_default};

mod dto;

use self::dto::SortDir;
pub use self::dto::{
    ApproveSupplierProductMappingRequest, ApproveSupplierProductMappingResult,
    CreateSupplierCatalogProductRequest, CreateSupplierCatalogProductResult,
    CreateSupplierProductMappingRequest, CreateSupplierProductMappingResult, PageView,
    ReviseSupplierCatalogProductRequest, ReviseSupplierCatalogProductResult, ReviseSupplierOfferingRequest,
    ReviseSupplierOfferingResult, SupplierCatalogIntakeBatchListParams, SupplierCatalogIntakeBatchView,
    SupplierCatalogMediaView, SupplierCatalogMediaWrite, SupplierCatalogProductDetailView,
    SupplierCatalogProductListParams, SupplierCatalogProductRevisionView, SupplierCatalogProductView,
    SupplierCatalogSkuDetailView, SupplierCatalogSkuListParams, SupplierCatalogSkuRevisionView,
    SupplierCatalogSkuView, SupplierCatalogSkuWrite, SupplierOfferingListParams, SupplierOfferingView,
    SupplierProductMappingListParams, SupplierProductMappingView,
};

/// 供应商 SPU 列表筛选条件类型（经 `SupplierCatalogExt` 关联类型跨 crate 可达）。
type SupplierCatalogProductFilter = <mongodb::Database as SupplierCatalogExt>::SupplierCatalogProductFilter;
/// 供应商 SKU 列表筛选条件类型。
type SupplierCatalogSkuFilter = <mongodb::Database as SupplierCatalogExt>::SupplierCatalogSkuFilter;
/// 映射列表筛选条件类型。
type SupplierProductMappingFilter = <mongodb::Database as SupplierCatalogExt>::SupplierProductMappingFilter;
/// 供给列表筛选条件类型。
type SupplierOfferingFilter = <mongodb::Database as SupplierCatalogExt>::SupplierOfferingFilter;
/// 入库批次列表筛选条件类型。
type SupplierCatalogIntakeBatchFilter =
    <mongodb::Database as SupplierCatalogExt>::SupplierCatalogIntakeBatchFilter;

/// 供应商商品库服务。
///
/// 提供供应商 SPU/SKU 来源修订、映射、供给与入库批次的编排。
pub struct SupplierCatalogService {
    db: Database,
}

impl SupplierCatalogService {
    /// 创建供应商商品库服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 分页查询供应商 SPU 列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_list(
        &self,
        params: &SupplierCatalogProductListParams,
    ) -> Result<PageView<SupplierCatalogProductView>> {
        params.validate()?;
        let (sort_by, sort_dir) = self::dto::normalize_sort(
            &params.sort_by,
            &params.sort_dir,
            self::dto::SUPPLIER_PRODUCT_SORT_FIELDS,
        )?;
        let filter = SupplierCatalogProductFilter {
            supplier_id: params
                .supplier_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SupplierAccountId::new(value.to_string())),
            source_type: params.source_type,
            status: params.status,
            supplier_spu_code: normalized_text(params.q.as_deref()),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_catalog_products()
            .search_supplier_catalog_products(&filter, &mut NoTransaction)
            .await?;
        let product_ids: Vec<SupplierCatalogProductId> =
            page.items.iter().map(|row| row.id.clone().into()).collect();
        let revisions = self
            .db
            .supplier_catalog_product_revisions()
            .find_revisions_by_product_ids(&product_ids, &mut NoTransaction)
            .await?;
        let mut by_product: HashMap<String, SupplierCatalogProductRevision> = HashMap::new();
        for revision in revisions {
            let entry = by_product
                .entry(revision.supplier_catalog_product_id.to_string())
                .or_insert_with(|| revision.clone());
            if revision.revision.revision_no > entry.revision.revision_no {
                *entry = revision;
            }
        }
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let current = by_product.get(&row.id);
                SupplierCatalogProductView {
                    id: row.id.clone(),
                    supplier_id: row.supplier_id.to_string(),
                    source_type: row.source_type,
                    supplier_spu_code: row.supplier_spu_code,
                    status: row.status,
                    current_revision_id: row.current_revision_id,
                    current_revision_no: current.map(|revision| revision.revision.revision_no),
                    name: current.map(|revision| revision.name.clone()),
                    source_category: current.and_then(|revision| revision.source_category.clone()),
                    source_brand: current.and_then(|revision| revision.source_brand.clone()),
                    source_updated_at: current.map(|revision| revision.source_updated_at.unix_secs() as u64),
                    version: row.version,
                    created_at: row.created_at,
                }
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询供应商 SPU 详情（修订历史、媒体、SKU 与映射）。
    ///
    /// # 参数
    /// * `id` - 供应商 SPU ID
    ///
    /// # 返回
    /// 返回详情视图。
    ///
    /// # 错误
    /// * `NotFound` - SPU 不存在
    pub async fn product_detail(&self, id: &str) -> Result<SupplierCatalogProductDetailView> {
        let product = self
            .db
            .supplier_catalog_products()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;
        let product_id = entities::ids::SupplierCatalogProductId::new(id.to_string());
        let mut revisions = self
            .db
            .supplier_catalog_product_revisions()
            .find_revisions_by_product_ids(std::slice::from_ref(&product_id), &mut NoTransaction)
            .await?;
        revisions.sort_by_key(|revision| std::cmp::Reverse(revision.revision.revision_no));
        let current_revision = revisions.first().cloned();

        let media = match &current_revision {
            Some(revision) => {
                let media = self
                    .db
                    .supplier_catalog_product_revision_media()
                    .find_media_by_revision_ids(&[revision.base.id.clone().into()], &mut NoTransaction)
                    .await?;
                media
                    .into_iter()
                    .map(|media| SupplierCatalogMediaView {
                        id: media.base.id,
                        usage: media.media_usage,
                        url: media.source_url_snapshot,
                        file_asset_id: media.file_asset_id.map(|id| id.to_string()),
                        archive_status: media.archive_status,
                        sort_order: media.sort_order,
                    })
                    .collect()
            }
            None => Vec::new(),
        };

        let skus = self
            .db
            .supplier_catalog_skus()
            .find_many(
                mongodb::bson::doc! { "supplier_catalog_product_id": product_id.to_string() },
                &mut NoTransaction,
            )
            .await?;
        let sku_ids: Vec<SupplierCatalogSkuId> = skus.iter().map(|sku| sku.base.id.clone().into()).collect();
        let sku_revisions = self
            .db
            .supplier_catalog_sku_revisions()
            .find_many(
                mongodb::bson::doc! { "supplier_catalog_sku_id": { "$in": sku_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?;
        let mut revisions_by_sku: HashMap<String, Vec<SupplierCatalogSkuRevision>> = HashMap::new();
        for revision in sku_revisions {
            revisions_by_sku
                .entry(revision.supplier_catalog_sku_id.to_string())
                .or_default()
                .push(revision);
        }
        for revisions in revisions_by_sku.values_mut() {
            revisions.sort_by_key(|revision| std::cmp::Reverse(revision.revision.revision_no));
        }
        let sku_views = skus
            .into_iter()
            .map(|sku| {
                let revisions = revisions_by_sku.remove(&sku.base.id).unwrap_or_default();
                SupplierCatalogSkuDetailView {
                    sku: sku_view(&sku, revisions.first()),
                    revisions: revisions.iter().map(sku_revision_view).collect(),
                }
            })
            .collect();

        let mappings = self
            .db
            .supplier_product_mappings()
            .search_supplier_product_mappings(
                &SupplierProductMappingFilter {
                    supplier_catalog_sku_id: None,
                    sku_id: None,
                    status: None,
                    page: 1,
                    page_size: 100,
                    sort_by: Some("created_at".to_string()),
                    sort_ascending: false,
                },
                &mut NoTransaction,
            )
            .await?
            .items
            .into_iter()
            .filter(|row| row.supplier_catalog_sku_id.to_string() == product.base.id)
            .map(|row| SupplierProductMappingView {
                id: row.id,
                supplier_catalog_sku_id: row.supplier_catalog_sku_id.to_string(),
                sku_id: row.sku_id.to_string(),
                status: row.status,
                approved_by: row.approved_by,
                approved_at: row.approved_at.map(|instant| instant.unix_secs() as u64),
                reason: None,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(SupplierCatalogProductDetailView {
            product: product_view(&product, &current_revision),
            revisions: revisions.iter().map(product_revision_view).collect(),
            media,
            skus: sku_views,
            mappings,
        })
    }

    /// 分页查询供应商 SKU 列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_list(
        &self,
        params: &SupplierCatalogSkuListParams,
    ) -> Result<PageView<SupplierCatalogSkuView>> {
        params.validate()?;
        let (sort_by, sort_dir) = self::dto::normalize_sort(
            &params.sort_by,
            &params.sort_dir,
            self::dto::SUPPLIER_SKU_SORT_FIELDS,
        )?;
        let filter = SupplierCatalogSkuFilter {
            supplier_catalog_product_id: params
                .supplier_catalog_product_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SupplierCatalogProductId::new(value.to_string())),
            status: None,
            supplier_sku_code: normalized_text(params.q.as_deref()),
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_catalog_skus()
            .search_supplier_catalog_skus(&filter, &mut NoTransaction)
            .await?;
        let sku_ids: Vec<SupplierCatalogSkuId> = page.items.iter().map(|row| row.id.clone().into()).collect();
        let current_revisions = self.current_sku_revisions(&sku_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let current = current_revisions.get(&row.id).cloned().flatten();
                SupplierCatalogSkuView {
                    id: row.id.clone(),
                    supplier_catalog_product_id: row.supplier_catalog_product_id.to_string(),
                    supplier_sku_code: row.supplier_sku_code,
                    status: row.status,
                    current_revision_id: row.current_revision_id,
                    current_revision_no: current.as_ref().map(|revision| revision.revision.revision_no),
                    name: current.as_ref().map(|revision| revision.name.clone()),
                    specification: current.as_ref().map(|revision| revision.specification.clone()),
                    barcode: current.as_ref().and_then(|revision| revision.barcode.clone()),
                    dropship_floor_price_gross: current.as_ref().and_then(|revision| {
                        revision.dropship_floor_price_gross.map(|value| value.to_string())
                    }),
                    bulk_floor_price_gross: current
                        .as_ref()
                        .and_then(|revision| revision.bulk_floor_price_gross.map(|value| value.to_string())),
                    bulk_minimum_order_quantity: current.as_ref().and_then(|revision| {
                        revision
                            .bulk_minimum_order_quantity
                            .map(|value| value.to_string())
                    }),
                    availability_status: current.as_ref().map(|revision| revision.availability_status),
                    version: row.version,
                    created_at: row.created_at,
                }
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询供应商 SKU → 公司 SKU 映射列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn mapping_list(
        &self,
        params: &SupplierProductMappingListParams,
    ) -> Result<PageView<SupplierProductMappingView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            self::dto::normalize_sort(&params.sort_by, &params.sort_dir, self::dto::MAPPING_SORT_FIELDS)?;
        let filter = SupplierProductMappingFilter {
            supplier_catalog_sku_id: params
                .supplier_catalog_sku_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SupplierCatalogSkuId::new(value.to_string())),
            sku_id: params
                .sku_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SkuId::new(value.to_string())),
            status: params.status,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_product_mappings()
            .search_supplier_product_mappings(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierProductMappingView {
                id: row.id,
                supplier_catalog_sku_id: row.supplier_catalog_sku_id.to_string(),
                sku_id: row.sku_id.to_string(),
                status: row.status,
                approved_by: row.approved_by,
                approved_at: row.approved_at.map(|instant| instant.unix_secs() as u64),
                reason: None,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询供给列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn offering_list(
        &self,
        params: &SupplierOfferingListParams,
    ) -> Result<PageView<SupplierOfferingView>> {
        params.validate()?;
        let (sort_by, sort_dir) =
            self::dto::normalize_sort(&params.sort_by, &params.sort_dir, self::dto::OFFERING_SORT_FIELDS)?;
        let filter = SupplierOfferingFilter {
            sku_id: params
                .sku_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SkuId::new(value.to_string())),
            supplier_id: params
                .supplier_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SupplierAccountId::new(value.to_string())),
            status: None,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_offerings()
            .search_supplier_offerings(&filter, &mut NoTransaction)
            .await?;
        let offering_ids: Vec<SupplierOfferingId> =
            page.items.iter().map(|row| row.id.clone().into()).collect();
        let current_revisions = self.current_offering_revisions(&offering_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| {
                let current = current_revisions.get(&row.id).cloned().flatten();
                SupplierOfferingView {
                    id: row.id.clone(),
                    sku_id: row.sku_id.to_string(),
                    supplier_id: row.supplier_id.to_string(),
                    supplier_catalog_sku_id: row.supplier_catalog_sku_id.to_string(),
                    status: row.status,
                    current_revision_id: row.current_revision_id,
                    current_revision_no: current.as_ref().map(|revision| revision.revision.revision_no),
                    dropship_supply_price_gross: current
                        .as_ref()
                        .map(|revision| revision.dropship_supply_price_gross.to_string()),
                    dropship_supply_price_net: current
                        .as_ref()
                        .map(|revision| revision.dropship_supply_price_net.to_string()),
                    bulk_supply_price_gross: current
                        .as_ref()
                        .map(|revision| revision.bulk_supply_price_gross.to_string()),
                    bulk_supply_price_net: current
                        .as_ref()
                        .map(|revision| revision.bulk_supply_price_net.to_string()),
                    input_tax_rate: current
                        .as_ref()
                        .map(|revision| revision.input_tax_rate.to_string()),
                    bulk_minimum_order_quantity: current
                        .as_ref()
                        .map(|revision| revision.bulk_minimum_order_quantity.to_string()),
                    supply_region: current
                        .as_ref()
                        .map(|revision| revision.supply_region.clone())
                        .unwrap_or_default(),
                    availability_status: current.as_ref().map(|revision| revision.availability_status),
                    valid_from: current.as_ref().map(|revision| revision.valid_from.to_string()),
                    valid_to: current
                        .as_ref()
                        .and_then(|revision| revision.valid_to.map(|date| date.to_string())),
                    version: row.version,
                    created_at: row.created_at,
                }
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询入库批次列表。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn intake_batch_list(
        &self,
        params: &SupplierCatalogIntakeBatchListParams,
    ) -> Result<PageView<SupplierCatalogIntakeBatchView>> {
        params.validate()?;
        let (sort_by, sort_dir) = self::dto::normalize_sort(
            &params.sort_by,
            &params.sort_dir,
            self::dto::INTAKE_BATCH_SORT_FIELDS,
        )?;
        let filter = SupplierCatalogIntakeBatchFilter {
            supplier_id: params
                .supplier_id
                .as_deref()
                .map(str::trim)
                .filter(|value| !value.is_empty())
                .map(|value| entities::ids::SupplierAccountId::new(value.to_string())),
            source_type: params.source_type,
            status: params.status,
            page: page_or_default(params.page),
            page_size: page_size_or_default(params.page_size),
            sort_by: Some(sort_by.to_string()),
            sort_ascending: matches!(sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_catalog_intake_batches()
            .search_supplier_catalog_intake_batches(&filter, &mut NoTransaction)
            .await?;
        let batch_ids: Vec<SupplierCatalogIntakeBatchId> =
            page.items.iter().map(|row| row.id.clone().into()).collect();
        let counts = self.intake_item_counts(&batch_ids).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierCatalogIntakeBatchView {
                id: row.id.clone(),
                source_type: row.source_type,
                supplier_id: row.supplier_id.to_string(),
                source_reference: row.source_reference,
                status: row.status,
                error_text: row.error_text,
                item_count: counts.get(&row.id).copied().unwrap_or(0),
                created_at: row.created_at,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建供应商商品（Excel/API/手工共用命令；§8.4 第 1 条幂等入库）。
    ///
    /// 单事务写入：入库批次与明细 → SPU 与首个来源修订（含媒体）→ 各 SKU 与
    /// 来源修订 → 明细回填 SKU 指针。批次 `(source_type, supplier_id,
    /// source_reference)` 唯一索引去重，重复提交返回首次结果。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回创建（或重放）结果。
    ///
    /// # 错误
    /// * `NotFound` - 供应商不存在
    /// * `ConflictError` - 同供应商 SPU 编码已存在
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_product(
        &self,
        req: CreateSupplierCatalogProductRequest,
        actor: &AuditActor,
    ) -> Result<CreateSupplierCatalogProductResult> {
        req.validate()?;
        let supplier_id = entities::ids::SupplierAccountId::new(req.supplier_id.trim().to_string());
        let source_reference = req
            .source_reference
            .clone()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or_else(|| req.idempotency_key.clone());

        // 幂等：批次来源键去重（§8.4 第 1 条）。
        if let Some(batch) = self
            .db
            .supplier_catalog_intake_batches()
            .find_by_source_key(
                req.source_type,
                &supplier_id,
                &source_reference,
                &mut NoTransaction,
            )
            .await?
        {
            let item = self
                .db
                .supplier_catalog_intake_items()
                .find_items_by_batch_ids(&[batch.base.id.clone().into()], &mut NoTransaction)
                .await?
                .into_iter()
                .next();
            let (product_id, intake_item_id) = match item {
                Some(item) => {
                    let intake_item_id = item.base.id.clone();
                    let product_id = match &item.supplier_catalog_sku_id {
                        Some(sku_id) => self
                            .db
                            .supplier_catalog_skus()
                            .find_by_id(sku_id, &mut NoTransaction)
                            .await
                            .ok()
                            .flatten()
                            .map(|sku| sku.supplier_catalog_product_id.to_string())
                            .unwrap_or_default(),
                        None => String::new(),
                    };
                    (product_id, intake_item_id)
                }
                None => (String::new(), String::new()),
            };
            return Ok(CreateSupplierCatalogProductResult {
                product_id,
                sku_ids: Vec::new(),
                intake_batch_id: batch.base.id,
                intake_item_id,
                replayed: true,
                reference: format!("SC-{}", source_reference),
                recorded_at: batch.base.created_at,
            });
        }

        self.db
            .supplier_accounts()
            .find_by_id(&supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商不存在".to_string()))?;

        let product = SupplierCatalogProduct::new(
            SupplierCatalogProductId::new(next_id()),
            SupplierCatalogProductData {
                supplier_id: supplier_id.clone(),
                source_type: req.source_type,
                source_connection_id: None,
                supplier_spu_code: req.supplier_spu_code.clone(),
            },
            actor.id(),
        )?;
        let product_id = product.base.id.clone();
        let revision = self.build_product_revision(&product_id, &req, 1)?;
        let media = self.build_revision_media(&revision.base.id, &req.media)?;

        let mut skus = Vec::with_capacity(req.skus.len());
        let mut sku_revisions = Vec::with_capacity(req.skus.len());
        for sku_write in &req.skus {
            let sku = SupplierCatalogSku::new(
                SupplierCatalogSkuId::new(next_id()),
                SupplierCatalogSkuData {
                    supplier_catalog_product_id: entities::ids::SupplierCatalogProductId::new(
                        product_id.clone(),
                    ),
                    supplier_sku_code: sku_write.supplier_sku_code.clone(),
                },
                actor.id(),
            )?;
            let sku_revision = self.build_sku_revision(&sku, sku_write, 1)?;
            sku_revisions.push(sku_revision);
            skus.push(sku);
        }

        let batch = SupplierCatalogIntakeBatch::new(
            SupplierCatalogIntakeBatchId::new(next_id()),
            SupplierCatalogIntakeBatchData {
                source_type: req.source_type,
                supplier_id: supplier_id.clone(),
                source_reference: source_reference.clone(),
                source_connection_id: None,
                file_asset_id: None,
            },
        )?;
        let mut intake_items = Vec::with_capacity(skus.len());
        for (index, sku) in skus.iter().enumerate() {
            intake_items.push(SupplierCatalogIntakeItem::new(
                SupplierCatalogIntakeItemId::new(next_id()),
                SupplierCatalogIntakeItemData {
                    supplier_catalog_intake_batch_id: batch.base.id.clone().into(),
                    row_no: (index + 1) as u32,
                    supplier_sku_code: sku.supplier_sku_code.clone(),
                    source_revision_token: req.source_revision_token.clone(),
                    classification: IntakeItemClassification::New,
                    result: IntakeItemResult::Success,
                    error_text: None,
                    supplier_catalog_sku_id: None,
                },
            )?);
        }

        let audit = actor.clone().resource_log(
            "supplier_catalog_product.create",
            "supplier_catalog_product",
            product_id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let product_for_tx = product.clone();
        let revision_for_tx = revision.clone();
        let batch_for_tx = batch.clone();
        let skus_for_tx = skus.clone();
        let sku_revisions_for_tx = sku_revisions.clone();
        let intake_items_for_tx = intake_items.clone();
        let media_for_tx = media.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_catalog()
                        .create_intake_batch(&batch_for_tx, &intake_items_for_tx, session)
                        .await?;
                    db.supplier_catalog()
                        .create_product_with_initial_revision(&product_for_tx, &revision_for_tx, session)
                        .await?;
                    for media in &media_for_tx {
                        db.supplier_catalog_product_revision_media()
                            .create(media, session)
                            .await?;
                    }
                    for (sku, sku_revision) in skus_for_tx.iter().zip(sku_revisions_for_tx.iter()) {
                        db.supplier_catalog()
                            .create_sku_with_initial_revision(sku, sku_revision, session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(CreateSupplierCatalogProductResult {
            product_id,
            sku_ids: skus.iter().map(|sku| sku.base.id.clone()).collect(),
            intake_batch_id: batch.base.id.clone(),
            intake_item_id: intake_items[0].base.id.clone(),
            replayed: false,
            reference: format!("SC-{}", source_reference),
            recorded_at: Instant::now().unix_secs() as u64,
        })
    }

    /// 供应商商品中心保存（形成新的来源修订，§8.4 第 1 条「详情即编辑」）。
    ///
    /// 单事务：SPU 稳定身份指针 + 新来源修订 + 媒体；既有 SKU 追加新修订并
    /// 推进指针，新 SKU 编码创建新 SKU 与首版。
    ///
    /// # 参数
    /// * `id` - 供应商 SPU ID
    /// * `req` - 保存请求（携带期望来源修订号做乐观并发校验）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新修订号。
    ///
    /// # 错误
    /// * `NotFound` - SPU 不存在
    /// * `ConflictError` - 期望修订号与当前不一致
    pub async fn revise_product(
        &self,
        id: &str,
        req: ReviseSupplierCatalogProductRequest,
        actor: &AuditActor,
    ) -> Result<ReviseSupplierCatalogProductResult> {
        req.validate()?;
        let product = self
            .db
            .supplier_catalog_products()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;
        let current_no = self
            .db
            .supplier_catalog_product_revisions()
            .find_revisions_by_product_ids(&[product.base.id.clone().into()], &mut NoTransaction)
            .await?
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0);
        if current_no != req.expected_revision_no {
            return Err(Error::ConflictError(
                "供应商商品来源版本已经变化，请刷新后重新保存".to_string(),
            ));
        }
        let next_no = current_no + 1;
        let revision = self.build_product_revision_from_revise(&product, &req, next_no)?;
        let media = self.build_revision_media(&revision.base.id, &req.media)?;

        let skus = self
            .db
            .supplier_catalog_skus()
            .find_many(
                mongodb::bson::doc! { "supplier_catalog_product_id": product.base.id.clone() },
                &mut NoTransaction,
            )
            .await?;
        let mut sku_ops: Vec<(SupplierCatalogSku, SupplierCatalogSkuRevision)> = Vec::new();
        let mut new_skus: Vec<(SupplierCatalogSku, SupplierCatalogSkuRevision)> = Vec::new();
        for sku_write in &req.skus {
            let existing = skus
                .iter()
                .find(|sku| sku.supplier_sku_code == sku_write.supplier_sku_code.trim());
            match existing {
                Some(sku) => {
                    let current = self
                        .db
                        .supplier_catalog_sku_revisions()
                        .find_many(
                            mongodb::bson::doc! { "supplier_catalog_sku_id": sku.base.id.clone() },
                            &mut NoTransaction,
                        )
                        .await?
                        .iter()
                        .map(|revision| revision.revision.revision_no)
                        .max()
                        .unwrap_or(0);
                    let mut sku_mut = sku.clone();
                    let sku_revision = self.build_sku_revision(&sku_mut, sku_write, current + 1)?;
                    sku_mut.stable.current_revision_id = Some(sku_revision.base.id.clone());
                    sku_ops.push((sku_mut, sku_revision));
                }
                None => {
                    let sku = SupplierCatalogSku::new(
                        SupplierCatalogSkuId::new(next_id()),
                        SupplierCatalogSkuData {
                            supplier_catalog_product_id: product.base.id.clone().into(),
                            supplier_sku_code: sku_write.supplier_sku_code.clone(),
                        },
                        actor.id(),
                    )?;
                    let sku_revision = self.build_sku_revision(&sku, sku_write, 1)?;
                    new_skus.push((sku, sku_revision));
                }
            }
        }

        let audit = actor.clone().resource_log(
            "supplier_catalog_product.update",
            "supplier_catalog_product",
            product.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let product_for_tx = product.clone();
        let revision_for_tx = revision.clone();
        let media_for_tx = media.clone();
        let sku_ops_for_tx = sku_ops.clone();
        let new_skus_for_tx = new_skus.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut product_mut = product_for_tx.clone();
                    product_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    db.supplier_catalog()
                        .append_product_revision(&mut product_mut, &revision_for_tx, session)
                        .await?;
                    for media in &media_for_tx {
                        db.supplier_catalog_product_revision_media()
                            .create(media, session)
                            .await?;
                    }
                    for (sku, sku_revision) in &sku_ops_for_tx {
                        db.supplier_catalog()
                            .append_sku_revision(&mut sku.clone(), sku_revision, session)
                            .await?;
                    }
                    for (sku, sku_revision) in &new_skus_for_tx {
                        db.supplier_catalog()
                            .create_sku_with_initial_revision(sku, sku_revision, session)
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReviseSupplierCatalogProductResult {
            product_id: product.base.id.clone(),
            revision_no: next_no,
            reference: format!("SC-REV-V{next_no}"),
            recorded_at: Instant::now().unix_secs() as u64,
        })
    }

    /// 创建供应商 SKU → 公司 SKU 映射（初始 `PENDING`）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回映射结果。
    ///
    /// # 错误
    /// * `NotFound` - 供应商 SKU 或公司 SKU 不存在
    /// * `ConflictError` - 同一供应商 SKU 已有映射
    pub async fn create_mapping(
        &self,
        req: CreateSupplierProductMappingRequest,
        actor: &AuditActor,
    ) -> Result<CreateSupplierProductMappingResult> {
        req.validate()?;
        let supplier_sku_id = entities::ids::SupplierCatalogSkuId::new(req.supplier_catalog_sku_id.clone());
        self.db
            .supplier_catalog_skus()
            .find_by_id(&supplier_sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商 SKU 不存在".to_string()))?;
        let sku_id = entities::ids::SkuId::new(req.sku_id.clone());
        self.db
            .skus()
            .find_by_id(&sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("公司 SKU 不存在".to_string()))?;
        let mapping = SupplierProductMapping::new(
            SupplierProductMappingId::new(next_id()),
            SupplierProductMappingData {
                supplier_catalog_sku_id: supplier_sku_id,
                sku_id,
                status: MappingStatus::Pending,
                approved_by: None,
                approved_at: None,
                reason: req.reason,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_product_mapping.create",
            "supplier_product_mapping",
            mapping.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mapping_for_tx = mapping.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_product_mappings()
                        .create(&mapping_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
        Ok(CreateSupplierProductMappingResult {
            mapping_id: mapping.base.id.clone(),
            status: mapping.status,
            version: mapping.base.version,
            reference: format!("MAP-{}", &mapping.base.id[..8]),
        })
    }

    /// 确认映射并登记双价供给（入池，§8.4 第 1 条）。
    ///
    /// 单事务：映射置 `Active`（审核人/时间） + 供给稳定身份与首个双价供给修订
    /// 原子写入；供给唯一索引 `(sku, supplier, supplier_sku)` 去重。
    ///
    /// # 参数
    /// * `id` - 映射 ID
    /// * `req` - 确认请求（双价与进项税率）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回供给结果。
    ///
    /// # 错误
    /// * `NotFound` - 映射不存在
    /// * `ConflictError` - 期望版本不一致或供给已存在
    pub async fn approve_mapping(
        &self,
        id: &str,
        req: ApproveSupplierProductMappingRequest,
        actor: &AuditActor,
    ) -> Result<ApproveSupplierProductMappingResult> {
        req.validate()?;
        let mut mapping = self
            .db
            .supplier_product_mappings()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("映射不存在".to_string()))?;
        if mapping.base.version != req.expected_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if mapping.status != MappingStatus::Pending {
            return Err(Error::ConflictError("映射已处理，请勿重复确认".to_string()));
        }
        let offering = SupplierOffering::new(
            SupplierOfferingId::new(next_id()),
            SupplierOfferingData {
                sku_id: mapping.sku_id.clone(),
                supplier_id: self.supplier_of_sku(&mapping.supplier_catalog_sku_id).await?,
                supplier_catalog_sku_id: mapping.supplier_catalog_sku_id.clone(),
            },
            actor.id(),
        )?;
        let offering_revision = self.build_offering_revision(
            &offering,
            1,
            req.dropship_supply_price_gross.as_str(),
            req.bulk_supply_price_gross.as_str(),
            req.input_tax_rate.as_str(),
            req.bulk_minimum_order_quantity.as_str(),
            &req.supply_region,
            req.valid_from.as_str(),
            req.valid_to.as_deref(),
            req.dropship_express.as_deref(),
            req.freight_amount.as_deref(),
            req.service_fee_amount.as_deref(),
            req.available_quantity.as_deref(),
            AvailabilityStatus::Available,
        )?;
        mapping.update(
            MappingStatus::Active,
            Some(actor.id().to_string()),
            Some(Instant::now()),
        )?;

        let audit = actor.clone().resource_log(
            "supplier_product_mapping.approve",
            "supplier_product_mapping",
            mapping.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let mapping_for_tx = mapping.clone();
        let offering_for_tx = offering.clone();
        let offering_revision_for_tx = offering_revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut offering_mut = offering_for_tx.clone();
                    offering_mut.stable.current_revision_id = Some(offering_revision_for_tx.base.id.clone());
                    db.supplier_catalog()
                        .create_offering_with_revision(&mut offering_mut, &offering_revision_for_tx, session)
                        .await?;
                    db.supplier_product_mappings()
                        .update(&mut mapping_for_tx.clone(), session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ApproveSupplierProductMappingResult {
            mapping_id: mapping.base.id.clone(),
            status: mapping.status,
            offering_id: offering.base.id.clone(),
            offering_revision_no: 1,
            version: mapping.base.version,
            reference: format!("OFF-{}", &offering.base.id[..8]),
        })
    }

    /// 供给改价/暂停/停止（形成新的不可变供给修订）。
    ///
    /// # 参数
    /// * `id` - 供给稳定身份 ID
    /// * `req` - 修订请求（携带期望修订号）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新修订号与状态。
    ///
    /// # 错误
    /// * `NotFound` - 供给不存在
    /// * `ConflictError` - 期望修订号与当前不一致
    pub async fn revise_offering(
        &self,
        id: &str,
        req: ReviseSupplierOfferingRequest,
        actor: &AuditActor,
    ) -> Result<ReviseSupplierOfferingResult> {
        req.validate()?;
        let mut offering = self
            .db
            .supplier_offerings()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供给不存在".to_string()))?;
        let current_no = self
            .db
            .supplier_offering_revisions()
            .find_revisions_by_offering_ids(&[offering.base.id.clone().into()], &mut NoTransaction)
            .await?
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0);
        if current_no != req.expected_revision_no {
            return Err(Error::ConflictError(
                "供给版本已经变化，请刷新后重新保存".to_string(),
            ));
        }
        let next_no = current_no + 1;
        let availability = if req.status == Some(OfferingStatus::Stopped) {
            AvailabilityStatus::Stopped
        } else {
            AvailabilityStatus::Available
        };
        let revision = self.build_offering_revision(
            &offering,
            next_no,
            req.dropship_supply_price_gross.as_str(),
            req.bulk_supply_price_gross.as_str(),
            req.input_tax_rate.as_str(),
            req.bulk_minimum_order_quantity.as_str(),
            &req.supply_region,
            req.valid_from.as_str(),
            req.valid_to.as_deref(),
            req.dropship_express.as_deref(),
            req.freight_amount.as_deref(),
            req.service_fee_amount.as_deref(),
            req.available_quantity.as_deref(),
            availability,
        )?;
        if let Some(status) = req.status {
            offering.stable.status = status;
        }

        let audit = actor.clone().resource_log(
            "supplier_offering.update",
            "supplier_offering",
            offering.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let offering_for_tx = offering.clone();
        let revision_for_tx = revision.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    let mut offering_mut = offering_for_tx.clone();
                    offering_mut.stable.current_revision_id = Some(revision_for_tx.base.id.clone());
                    db.supplier_catalog()
                        .create_offering_with_revision(&mut offering_mut, &revision_for_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(ReviseSupplierOfferingResult {
            offering_id: offering.base.id.clone(),
            revision_no: next_no,
            status: offering.stable.status,
            version: offering.base.version,
            reference: format!("OFF-REV-V{next_no}"),
        })
    }
}

// ---------------------------------------------------------------------------
// 私有编排辅助
// ---------------------------------------------------------------------------

impl SupplierCatalogService {
    /// 从供应商 SKU 解析所属供应商。
    async fn supplier_of_sku(
        &self,
        sku_id: &SupplierCatalogSkuId,
    ) -> Result<entities::ids::SupplierAccountId> {
        let sku = self
            .db
            .supplier_catalog_skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商 SKU 不存在".to_string()))?;
        let product = self
            .db
            .supplier_catalog_products()
            .find_by_id(&sku.supplier_catalog_product_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商商品不存在".to_string()))?;
        Ok(product.supplier_id)
    }

    /// 批量取回 SKU 的当前来源修订。
    async fn current_sku_revisions(
        &self,
        sku_ids: &[SupplierCatalogSkuId],
    ) -> Result<HashMap<String, Option<SupplierCatalogSkuRevision>>> {
        if sku_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .db
            .supplier_catalog_sku_revisions()
            .find_many(
                mongodb::bson::doc! { "supplier_catalog_sku_id": { "$in": sku_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?;
        let mut map: HashMap<String, Option<SupplierCatalogSkuRevision>> =
            sku_ids.iter().map(|id| (id.to_string(), None)).collect();
        for revision in revisions {
            let entry = map
                .entry(revision.supplier_catalog_sku_id.to_string())
                .or_default();
            let replace = entry
                .as_ref()
                .map(|current| revision.revision.revision_no > current.revision.revision_no)
                .unwrap_or(true);
            if replace {
                *entry = Some(revision);
            }
        }
        Ok(map)
    }

    /// 批量取回供给的当前修订。
    async fn current_offering_revisions(
        &self,
        offering_ids: &[SupplierOfferingId],
    ) -> Result<HashMap<String, Option<SupplierOfferingRevision>>> {
        if offering_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let revisions = self
            .db
            .supplier_offering_revisions()
            .find_many(
                mongodb::bson::doc! { "supplier_offering_id": { "$in": offering_ids.iter().map(ToString::to_string).collect::<Vec<_>>() } },
                &mut NoTransaction,
            )
            .await?;
        let mut map: HashMap<String, Option<SupplierOfferingRevision>> =
            offering_ids.iter().map(|id| (id.to_string(), None)).collect();
        for revision in revisions {
            let entry = map.entry(revision.supplier_offering_id.to_string()).or_default();
            let replace = entry
                .as_ref()
                .map(|current| revision.revision.revision_no > current.revision.revision_no)
                .unwrap_or(true);
            if replace {
                *entry = Some(revision);
            }
        }
        Ok(map)
    }

    /// 统计批次的入库明细条数。
    async fn intake_item_counts(
        &self,
        batch_ids: &[SupplierCatalogIntakeBatchId],
    ) -> Result<HashMap<String, i64>> {
        let mut counts = HashMap::new();
        for batch_id in batch_ids {
            let items = self
                .db
                .supplier_catalog_intake_items()
                .find_items_by_batch_ids(std::slice::from_ref(batch_id), &mut NoTransaction)
                .await?;
            counts.insert(batch_id.to_string(), items.len() as i64);
        }
        Ok(counts)
    }

    /// 构建 SPU 来源修订（入库场景）。
    fn build_product_revision(
        &self,
        product_id: &str,
        req: &CreateSupplierCatalogProductRequest,
        revision_no: u32,
    ) -> Result<SupplierCatalogProductRevision> {
        let payload_hmac = content_fingerprint(&[
            req.name.as_str(),
            req.description.as_deref().unwrap_or(""),
            req.supplier_spu_code.as_str(),
        ]);
        SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new(next_id()),
            SupplierCatalogProductRevisionData {
                supplier_catalog_product_id: entities::ids::SupplierCatalogProductId::new(
                    product_id.to_string(),
                ),
                revision_no,
                name: req.name.clone(),
                description: req.description.clone(),
                source_product_kind: req.source_product_kind.clone(),
                source_category: req.source_category.clone(),
                source_brand: req.source_brand.clone(),
                structured_attributes: req.structured_attributes.clone(),
                source_revision_token: req.source_revision_token.clone(),
                source_updated_at: Instant::now(),
                payload_hmac,
                valid_from: req.valid_from.as_deref().map(parse_business_date).transpose()?,
                valid_to: req.valid_to.as_deref().map(parse_business_date).transpose()?,
            },
            req.source_type,
        )
        .map_err(Into::into)
    }

    /// 构建 SPU 来源修订（保存场景）。
    fn build_product_revision_from_revise(
        &self,
        product: &SupplierCatalogProduct,
        req: &ReviseSupplierCatalogProductRequest,
        revision_no: u32,
    ) -> Result<SupplierCatalogProductRevision> {
        let payload_hmac = content_fingerprint(&[
            req.name.as_str(),
            req.description.as_deref().unwrap_or(""),
            req.supplier_spu_code.as_str(),
        ]);
        SupplierCatalogProductRevision::new(
            SupplierCatalogProductRevisionId::new(next_id()),
            SupplierCatalogProductRevisionData {
                supplier_catalog_product_id: product.base.id.clone().into(),
                revision_no,
                name: req.name.clone(),
                description: req.description.clone(),
                source_product_kind: req.source_product_kind.clone(),
                source_category: req.source_category.clone(),
                source_brand: req.source_brand.clone(),
                structured_attributes: req.structured_attributes.clone(),
                source_revision_token: req.source_revision_token.clone(),
                source_updated_at: Instant::now(),
                payload_hmac,
                valid_from: req.valid_from.as_deref().map(parse_business_date).transpose()?,
                valid_to: req.valid_to.as_deref().map(parse_business_date).transpose()?,
            },
            product.source_type,
        )
        .map_err(Into::into)
    }

    /// 构建来源媒体。
    fn build_revision_media(
        &self,
        revision_id: &str,
        media: &[SupplierCatalogMediaWrite],
    ) -> Result<Vec<SupplierCatalogProductRevisionMedia>> {
        let mut result = Vec::with_capacity(media.len());
        let mut sort_by_usage: HashMap<String, u32> = HashMap::new();
        for write in media {
            let sort_order = {
                let counter = sort_by_usage.entry(write.usage.as_str().to_string()).or_insert(0);
                *counter += 1;
                *counter
            };
            result.push(SupplierCatalogProductRevisionMedia::new(
                SupplierCatalogProductRevisionMediaId::new(next_id()),
                SupplierCatalogProductRevisionMediaData {
                    supplier_catalog_product_revision_id:
                        entities::ids::SupplierCatalogProductRevisionId::new(revision_id.to_string()),
                    media_usage: write.usage,
                    file_asset_id: write.file_asset_id.clone().map(entities::ids::FileAssetId::new),
                    source_url_snapshot: Some(write.url.clone()),
                    archive_status: ArchiveStatus::PendingImport,
                    sort_order,
                },
            )?);
        }
        Ok(result)
    }

    /// 构建 SKU 来源修订。
    fn build_sku_revision(
        &self,
        sku: &SupplierCatalogSku,
        write: &SupplierCatalogSkuWrite,
        revision_no: u32,
    ) -> Result<SupplierCatalogSkuRevision> {
        SupplierCatalogSkuRevision::new(
            SupplierCatalogSkuRevisionId::new(next_id()),
            SupplierCatalogSkuRevisionData {
                supplier_catalog_sku_id: sku.base.id.clone().into(),
                revision_no,
                source_revision_token: Some(content_fingerprint(&[write.supplier_sku_code.as_str()])),
                name: write.name.clone(),
                specification: write.specification.clone(),
                source_base_unit: write.source_base_unit.clone(),
                barcode: write.barcode.clone(),
                structured_attributes: write.structured_attributes.clone(),
                source_main_image_asset_id: write
                    .source_main_image_asset_id
                    .clone()
                    .map(entities::ids::FileAssetId::new),
                source_main_image_url_snapshot: write
                    .source_main_image_url
                    .as_ref()
                    .map(|value| value.trim().to_string())
                    .filter(|value| !value.is_empty()),
                main_image_archive_status: write
                    .source_main_image_url
                    .as_ref()
                    .map(|value| value.trim())
                    .filter(|value| !value.is_empty())
                    .map(|_| ArchiveStatus::PendingImport),
                dropship_floor_price_gross: self.parse_amount(write.dropship_floor_price_gross.as_deref())?,
                bulk_floor_price_gross: self.parse_amount(write.bulk_floor_price_gross.as_deref())?,
                bulk_minimum_order_quantity: self
                    .parse_quantity(write.bulk_minimum_order_quantity.as_deref())?,
                available_quantity: self.parse_quantity(write.available_quantity.as_deref())?,
                availability_status: write.availability_status,
                source_updated_at: Instant::now(),
                received_at: Instant::now(),
                source_payload_hmac: Some(content_fingerprint(&[
                    write.name.as_str(),
                    write.specification.as_str(),
                    write.supplier_sku_code.as_str(),
                ])),
            },
        )
        .map_err(Into::into)
    }

    /// 构建双价供给修订（含税/不含税按统一定点规则换算）。
    #[allow(clippy::too_many_arguments)]
    fn build_offering_revision(
        &self,
        offering: &SupplierOffering,
        revision_no: u32,
        dropship_gross: &str,
        bulk_gross: &str,
        input_tax_rate: &str,
        bulk_minimum_order_quantity: &str,
        supply_region: &[String],
        valid_from: &str,
        valid_to: Option<&str>,
        dropship_express: Option<&str>,
        freight_amount: Option<&str>,
        service_fee_amount: Option<&str>,
        available_quantity: Option<&str>,
        availability_status: AvailabilityStatus,
    ) -> Result<SupplierOfferingRevision> {
        let rate = Rate::from_str(input_tax_rate.trim())
            .map_err(|_| Error::ValidationError(format!("非法进项税率: {input_tax_rate}")))?;
        let dropship_gross_price = UnitPrice::from_str(dropship_gross.trim())
            .map_err(|_| Error::ValidationError(format!("非法一件代发供给价: {dropship_gross}")))?;
        let bulk_gross_price = UnitPrice::from_str(bulk_gross.trim())
            .map_err(|_| Error::ValidationError(format!("非法集采供给价: {bulk_gross}")))?;
        let dropship_net_price = price_net(dropship_gross_price, rate);
        let bulk_net_price = price_net(bulk_gross_price, rate);
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new(next_id()),
            SupplierOfferingRevisionData {
                supplier_offering_id: offering.base.id.clone().into(),
                revision_no,
                dropship_supply_price_gross: dropship_gross_price,
                dropship_supply_price_net: dropship_net_price,
                bulk_supply_price_gross: bulk_gross_price,
                bulk_supply_price_net: bulk_net_price,
                input_tax_rate: rate,
                dropship_express: dropship_express.map(str::to_string),
                freight_amount: self.parse_amount(freight_amount)?,
                service_fee_amount: self.parse_amount(service_fee_amount)?,
                bulk_minimum_order_quantity: Quantity::from_str(bulk_minimum_order_quantity.trim()).map_err(
                    |_| Error::ValidationError(format!("非法集采起订量: {bulk_minimum_order_quantity}")),
                )?,
                supply_region: supply_region.to_vec(),
                availability_status,
                available_quantity: self.parse_quantity(available_quantity)?,
                product_capabilities: Vec::new(),
                valid_from: parse_business_date(valid_from)?,
                valid_to: valid_to.map(parse_business_date).transpose()?,
                prefill_source_refs: entities::supplier_catalog::PrefillSourceRefs::default(),
            },
        )
        .map_err(Into::into)
    }

    /// 解析金额。
    fn parse_amount(&self, value: Option<&str>) -> Result<Option<Amount>> {
        match value {
            Some(value) if !value.trim().is_empty() => Amount::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法金额: {value}"))),
            _ => Ok(None),
        }
    }

    /// 解析数量。
    fn parse_quantity(&self, value: Option<&str>) -> Result<Option<Quantity>> {
        match value {
            Some(value) if !value.trim().is_empty() => Quantity::from_str(value.trim())
                .map(Some)
                .map_err(|_| Error::ValidationError(format!("非法数量: {value}"))),
            _ => Ok(None),
        }
    }
}

/// 从 SPU 实体与当前修订构造视图。
fn product_view(
    product: &SupplierCatalogProduct,
    current: &Option<SupplierCatalogProductRevision>,
) -> SupplierCatalogProductView {
    SupplierCatalogProductView {
        id: product.base.id.clone(),
        supplier_id: product.supplier_id.to_string(),
        source_type: product.source_type,
        supplier_spu_code: product.supplier_spu_code.clone(),
        status: product.stable.status,
        current_revision_id: product.stable.current_revision_id.clone(),
        current_revision_no: current.as_ref().map(|revision| revision.revision.revision_no),
        name: current.as_ref().map(|revision| revision.name.clone()),
        source_category: current
            .as_ref()
            .and_then(|revision| revision.source_category.clone()),
        source_brand: current
            .as_ref()
            .and_then(|revision| revision.source_brand.clone()),
        source_updated_at: current
            .as_ref()
            .map(|revision| revision.source_updated_at.unix_secs() as u64),
        version: product.base.version,
        created_at: product.base.created_at,
    }
}

/// 从 SKU 实体与当前修订构造视图。
fn sku_view(
    sku: &SupplierCatalogSku,
    current: Option<&SupplierCatalogSkuRevision>,
) -> SupplierCatalogSkuView {
    SupplierCatalogSkuView {
        id: sku.base.id.clone(),
        supplier_catalog_product_id: sku.supplier_catalog_product_id.to_string(),
        supplier_sku_code: sku.supplier_sku_code.clone(),
        status: sku.stable.status,
        current_revision_id: sku.stable.current_revision_id.clone(),
        current_revision_no: current.map(|revision| revision.revision.revision_no),
        name: current.map(|revision| revision.name.clone()),
        specification: current.map(|revision| revision.specification.clone()),
        barcode: current.and_then(|revision| revision.barcode.clone()),
        dropship_floor_price_gross: current
            .and_then(|revision| revision.dropship_floor_price_gross.map(|v| v.to_string())),
        bulk_floor_price_gross: current
            .and_then(|revision| revision.bulk_floor_price_gross.map(|v| v.to_string())),
        bulk_minimum_order_quantity: current
            .and_then(|revision| revision.bulk_minimum_order_quantity.map(|v| v.to_string())),
        availability_status: current.map(|revision| revision.availability_status),
        version: sku.base.version,
        created_at: sku.base.created_at,
    }
}

/// 从 SPU 来源修订构造视图。
fn product_revision_view(revision: &SupplierCatalogProductRevision) -> SupplierCatalogProductRevisionView {
    SupplierCatalogProductRevisionView {
        id: revision.base.id.clone(),
        revision_no: revision.revision.revision_no,
        name: revision.name.clone(),
        description: revision.description.clone(),
        source_product_kind: revision.source_product_kind.clone(),
        source_category: revision.source_category.clone(),
        source_brand: revision.source_brand.clone(),
        structured_attributes: revision.structured_attributes.clone(),
        source_revision_token: revision.source_revision_token.clone(),
        source_updated_at: revision.source_updated_at.unix_secs() as u64,
        valid_from: revision.valid_from.map(|date| date.to_string()),
        valid_to: revision.valid_to.map(|date| date.to_string()),
    }
}

/// 从 SKU 来源修订构造视图。
fn sku_revision_view(revision: &SupplierCatalogSkuRevision) -> SupplierCatalogSkuRevisionView {
    SupplierCatalogSkuRevisionView {
        id: revision.base.id.clone(),
        revision_no: revision.revision.revision_no,
        name: revision.name.clone(),
        specification: revision.specification.clone(),
        source_base_unit: revision.source_base_unit.clone(),
        barcode: revision.barcode.clone(),
        structured_attributes: revision.structured_attributes.clone(),
        source_main_image_url: revision.source_main_image_url_snapshot.clone(),
        source_main_image_asset_id: revision
            .source_main_image_asset_id
            .as_ref()
            .map(|id| id.to_string()),
        dropship_floor_price_gross: revision.dropship_floor_price_gross.map(|value| value.to_string()),
        bulk_floor_price_gross: revision.bulk_floor_price_gross.map(|value| value.to_string()),
        bulk_minimum_order_quantity: revision
            .bulk_minimum_order_quantity
            .map(|value| value.to_string()),
        available_quantity: revision.available_quantity.map(|value| value.to_string()),
        availability_status: revision.availability_status,
        source_updated_at: revision.source_updated_at.unix_secs() as u64,
    }
}

/// 解析业务日期字符串。
fn parse_business_date(value: &str) -> Result<BusinessDate> {
    BusinessDate::from_str(value.trim()).map_err(|_| Error::ValidationError(format!("非法业务日期: {value}")))
}

/// 单价不含税换算：`net = gross − round_to_cent(gross × rate)`（§4.2 铁律 4 单价形态）。
fn price_net(gross: UnitPrice, rate: Rate) -> UnitPrice {
    let net = gross.to_decimal() - round_to_cent(gross.to_decimal() * rate.to_decimal());
    UnitPrice::try_from(net).expect("单价换算后小数位不超过 4 位")
}

/// 内容指纹（SipHash 十六进制；同二进制内稳定，用于来源幂等键与内容比对）。
fn content_fingerprint(parts: &[&str]) -> String {
    use std::collections::hash_map::DefaultHasher;
    use std::hash::{Hash, Hasher};
    let mut hasher = DefaultHasher::new();
    for part in parts {
        part.hash(&mut hasher);
    }
    format!("{:016x}", hasher.finish())
}
