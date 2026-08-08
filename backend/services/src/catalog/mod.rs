//! 域 D10 `catalog` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 字典（分类/品牌/单位/规格属性/属性值）单集合 CRUD → `&mut NoTransaction`
//!   （审计日志按 D01 既有写法独立写入）；
//! - 商品创建与规格编辑：跨集合（products + product_revisions + 媒体 +
//!   skus + sku_revisions + 规格属性值 + 审计）→
//!   `database::Transactional::with_transaction`，保证「SPU 身份 + 修订快照 +
//!   SKU 身份 + 规格值」原子可见（数据模型 §6.3）；
//! - 卡券类目原子创建：跨集合（[新建分类] + products + product_revisions +
//!   唯一 SKU + sku_revisions + voucher_category_profile_revisions + 审计）→
//!   `database::Transactional::with_transaction`，与商品创建同构（数据模型
//!   §6.3：卡券类目即 VOUCHER 类型的单 SKU 商品，不再要求预先存在 SKU）。
//!
//! 业务规则来自 entities（`new()`/`update()` 已完成校验与规范化，
//! `specification::compute_specification_signature` 计算签名），Service 只编排：
//! 字典存在性与启用校验、分类-属性适用性、条码冲突、成环检测、规格签名分类
//! （保留/新增/重新启用/移除）与事务写入。跨域只调对方 Repository（D05
//! `file_assets` 校验媒体引用；D02 `audit_logs` 写审计），禁止 Service 依赖 Service。

use std::collections::HashMap;

use database::{AccessControlExt, CatalogExt, FileAssetExt, NoTransaction, Transactional};
use entities::catalog::product::{Product, ProductData};
use entities::catalog::product_brand::{ProductBrand, ProductBrandData, ProductBrandUpdate};
use entities::catalog::product_category::{ProductCategory, ProductCategoryData, ProductCategoryUpdate};
use entities::catalog::product_revision::{ProductRevision, ProductRevisionData};
use entities::catalog::product_revision_media::{MediaRole, ProductRevisionMedia, ProductRevisionMediaData};
use entities::catalog::sku::{Sku, SkuData, SkuUpdate};
use entities::catalog::sku_attribute::{
    AttributeValueType, SkuAttribute, SkuAttributeData, SkuAttributeUpdate,
};
use entities::catalog::sku_attribute_value::{
    SkuAttributeValue, SkuAttributeValueData, SkuAttributeValueUpdate,
};
use entities::catalog::sku_revision::{SkuRevision, SkuRevisionData};
use entities::catalog::sku_revision_attribute_value::{
    SkuRevisionAttributeValue, SkuRevisionAttributeValueData,
};
use entities::catalog::specification::{compute_specification_signature, SpecSignatureEntry};
use entities::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData, UnitOfMeasureUpdate};
use entities::catalog::voucher_category_profile_revision::{
    VoucherCategoryProfileRevision, VoucherCategoryProfileRevisionData,
};
use entities::catalog::{
    EnableStatus, ProductBrandId, ProductCategoryId, ProductId, ProductKind, ProductRevisionId,
    ProductRevisionMediaId, SkuAttributeId, SkuAttributeValueId, SkuId, SkuRevisionAttributeValueId,
    SkuRevisionId, UnitOfMeasureId, VoucherCategoryProfileRevisionId,
};
use entities::common::time::BusinessDate;
use id_generator::next_id;
use mongodb::bson::doc;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::catalog::dto::SortDir;
use crate::errors::{Error, Result};

mod dto;
mod sellable;

pub use self::dto::{
    CreateProductBrandRequest, CreateProductCategoryRequest, CreateProductRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, CreateVoucherCategoryRequest,
    MoveProductCategoryRequest, NewVoucherCategoryInput, PageView, ProductBrandListParams, ProductBrandView,
    ProductCategoryListParams, ProductCategoryView, ProductListParams, ProductMediaInput,
    ProductRevisionListParams, ProductRevisionMediaView, ProductRevisionView, ProductSkuInput, ProductView,
    SkuAttributeListParams, SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView,
    SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView, SpecEntryInput, UnitOfMeasureListParams,
    UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest, UpdateProductRequest,
    UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateUnitOfMeasureRequest,
    UpdateVoucherCategoryRequest, VoucherCategoryProfileListParams, VoucherCategoryProfileView,
    VoucherSkuInput,
};
pub(crate) use self::sellable::sellable_sku_invalid_error;
pub use self::sellable::{SellableSkuListParams, SellableSkuView};

/// 商品分类列表筛选条件类型（经 `CatalogExt` 关联类型跨 crate 可达）。
type ProductCategoryFilter = <mongodb::Database as CatalogExt>::ProductCategoryFilter;
/// 商品品牌列表筛选条件类型。
type ProductBrandFilter = <mongodb::Database as CatalogExt>::ProductBrandFilter;
/// 计量单位列表筛选条件类型。
type UnitOfMeasureFilter = <mongodb::Database as CatalogExt>::UnitOfMeasureFilter;
/// 规格属性列表筛选条件类型。
type SkuAttributeFilter = <mongodb::Database as CatalogExt>::SkuAttributeFilter;
/// 规格属性值列表筛选条件类型。
type SkuAttributeValueFilter = <mongodb::Database as CatalogExt>::SkuAttributeValueFilter;
/// 商品列表筛选条件类型。
type ProductFilter = <mongodb::Database as CatalogExt>::ProductFilter;
/// 商品修订列表筛选条件类型。
type ProductRevisionFilter = <mongodb::Database as CatalogExt>::ProductRevisionFilter;
/// SKU 列表筛选条件类型。
type SkuFilter = <mongodb::Database as CatalogExt>::SkuFilter;
/// SKU 修订列表筛选条件类型。
type SkuRevisionFilter = <mongodb::Database as CatalogExt>::SkuRevisionFilter;
/// 卡券类目扩展修订列表筛选条件类型。
type VoucherProfileFilter = <mongodb::Database as CatalogExt>::VoucherCategoryProfileRevisionFilter;

/// 规格编辑动作（数据模型 §6.3：保留/新增/重新启用/移除）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum SkuEditAction {
    /// 全新签名：分配新 SKU 身份并写首个修订。
    Create,
    /// 签名未变：沿用原 `sku_id` 并追加修订。
    Keep,
    /// 历史停用签名再次出现：复用原 `sku_id`、追加修订并显式重新启用。
    Reactivate,
}

/// 规格编辑计划中的一行（`Create`/`Keep`/`Reactivate` 都伴随一条新 SKU 修订）。
struct SkuEditItem {
    /// 编辑动作。
    action: SkuEditAction,
    /// 待写入的 SKU（新增时为新建；重新启用时为已置 `Active` 的既有实体）。
    sku: Sku,
    /// 待写入的 SKU 修订。
    revision: SkuRevision,
    /// 待写入的修订规格属性值。
    attribute_values: Vec<SkuRevisionAttributeValue>,
}

/// 新 SKU 构建上下文（所属 SPU、名称快照、分类、生效区间、操作人）。
struct NewSkuContext<'a> {
    /// 所属 SPU。
    product_id: &'a ProductId,
    /// 商品名称（作为 SKU 修订名称快照）。
    product_name: &'a str,
    /// ERP 分类（规格适用性校验）。
    category_id: &'a ProductCategoryId,
    /// 生效起始日。
    effective_from: BusinessDate,
    /// 生效截止日。
    effective_to: Option<BusinessDate>,
    /// 操作人 ID。
    created_by: &'a str,
}

/// 一条已解析的规格属性-值（回填字典身份后进入签名计算与行写入）。
struct ResolvedSpecEntry {
    /// 规格属性代码（签名用）。
    attribute_code: String,
    /// 属性值代码（签名用；规范文本属性取文本原值）。
    value_code: String,
    /// 规格属性 ID。
    attribute_id: SkuAttributeId,
    /// 受控枚举属性值 ID（枚举属性使用）。
    attribute_value_id: Option<SkuAttributeValueId>,
    /// 规范文本属性值（文本属性使用）。
    normalized_text_value: Option<String>,
}

/// 商品（SPU）创建草稿（全部 ID 在事务外预生成，事务内只做写入）。
struct ProductDraft {
    /// 写入审计日志的创建原因。
    change_reason: Option<String>,
    /// SPU 稳定身份。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// SPU 级媒体行。
    media: Vec<ProductRevisionMedia>,
    /// SKU 行（action 均为 `Create`）。
    sku_items: Vec<SkuEditItem>,
}

/// 卡券类目原子创建草稿（全部 ID 在事务外预生成，事务内只做写入）。
struct VoucherCategoryDraft {
    /// 内联新建的分类（引用已有分类时为 `None`）。
    new_category: Option<ProductCategory>,
    /// SPU 稳定身份。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// 唯一 SKU 行（action 恒为 `Create`）。
    sku_item: SkuEditItem,
    /// 卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

/// 卡券类目更新草稿（更新 SPU 指针 + 追加商品/SKU/扩展修订）。
struct VoucherCategoryUpdateDraft {
    /// 已更新 `current_revision_id` 的商品。
    product: Product,
    /// 新商品修订。
    product_revision: ProductRevision,
    /// 已更新 `current_revision_id` 的 SKU。
    sku: Sku,
    /// 新 SKU 修订。
    sku_revision: SkuRevision,
    /// 新卡券类目扩展修订。
    voucher_revision: VoucherCategoryProfileRevision,
}

/// 商品规格编辑计划（数据模型 §6.3 全量替换语义）。
struct SpecEditPlan {
    /// 写入审计日志的变更原因。
    change_reason: Option<String>,
    /// 修订后的 SPU。
    product: Product,
    /// 商品修订快照。
    revision: ProductRevision,
    /// SPU 级媒体行。
    media: Vec<ProductRevisionMedia>,
    /// 带新修订的 SKU 行（`Create`/`Keep`/`Reactivate`）。
    sku_items: Vec<SkuEditItem>,
    /// 移除签名的既有 SKU（转为停用，保留全部历史）。
    disable: Vec<Sku>,
}

/// 卡券类目共用根分类稳定代码（所有未指定分类的卡券类目挂在此节点下）。
const VOUCHER_ROOT_CATEGORY_CODE: &str = "VOUCHER";
/// 卡券类目共用根分类名称。
const VOUCHER_ROOT_CATEGORY_NAME: &str = "卡券";
/// 卡券类目默认品牌稳定代码。
const VOUCHER_DEFAULT_BRAND_CODE: &str = "FSY";
/// 卡券类目默认品牌名称（业务固定：福尚云）。
const VOUCHER_DEFAULT_BRAND_NAME: &str = "福尚云";
/// 卡券类目默认基础单位代码/名称/符号（业务固定：张）。
const VOUCHER_DEFAULT_UNIT_CODE: &str = "张";

/// 商品域服务。
///
/// 提供商品字典、SPU/SKU 与卡券类目的创建、查询、更新编排。
pub struct CatalogService {
    db: Database,
}

impl CatalogService {
    /// 创建商品域服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    // ---------- 商品分类（树形字典） ----------

    /// 分页查询商品分类列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`category_code`/`name`/`parent_category_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn product_category_list(
        &self,
        params: &ProductCategoryListParams,
    ) -> Result<PageView<ProductCategoryView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductCategoryFilter {
            category_code: query.category_code,
            name: query.name,
            parent_category_id: query.parent_category_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_categories()
            .search_product_categories(&filter, &mut NoTransaction)
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结），按字段映射为响应视图。
        let items = page
            .items
            .into_iter()
            .map(|row| ProductCategoryView {
                id: row.id,
                category_code: row.category_code,
                parent_category_id: row.parent_category_id,
                name: row.name,
                product_kind: row.product_kind,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建商品分类（单集合写入，无事务）。
    ///
    /// 新建分类时校验父分类存在且不会形成环（沿祖先链上溯，命中自身即环）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建分类的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 父分类不存在
    /// * `BusinessLogicError` - 父子关系将形成环
    /// * `ConflictError` - category_code 重复（唯一索引透出）
    pub async fn product_category_create(
        &self,
        req: CreateProductCategoryRequest,
        actor: &AuditActor,
    ) -> Result<ProductCategoryView> {
        req.validate()?;
        let parent_id = req.parent_category_id.clone();
        let id = ProductCategoryId::new(next_id());
        self.ensure_parent_chain_ok(&id, parent_id.as_ref()).await?;
        let category = ProductCategory::new(
            id.clone(),
            ProductCategoryData {
                category_code: req.category_code,
                parent_category_id: parent_id,
                name: req.name,
                product_kind: req.product_kind,
                status: req.status.unwrap_or(EnableStatus::Active),
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("product_category.create", "product_category", id.to_string())?;
        self.db
            .product_categories()
            .create(&category, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(category.into())
    }

    /// 更新商品分类（乐观锁语义，`category_code`/`parent_category_id` 不可改）。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后分类的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 分类不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn product_category_update(
        &self,
        id: &str,
        req: UpdateProductCategoryRequest,
        actor: &AuditActor,
    ) -> Result<ProductCategoryView> {
        req.validate()?;
        let mut category = self.load_category(id).await?;
        ensure_version(category.base.version, req.version)?;
        category.update(
            ProductCategoryUpdate {
                name: req.name,
                product_kind: req.product_kind,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "product_category.update",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_categories().update(&mut category, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProductCategory, crate::errors::Error>(category)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 移动商品分类到新父分类（树形维护；成环检测在服务层完成）。
    ///
    /// 沿新父分类的祖先链上溯，命中本节点即拒绝；`None` 表示提升为根分类。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `req` - 移动请求（含期望版本与新父分类）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回移动后分类的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 分类或新父分类不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `BusinessLogicError` - 移动将形成环
    pub async fn product_category_move(
        &self,
        id: &str,
        req: MoveProductCategoryRequest,
        actor: &AuditActor,
    ) -> Result<ProductCategoryView> {
        req.validate()?;
        let mut category = self.load_category(id).await?;
        ensure_version(category.base.version, req.version)?;
        self.ensure_parent_chain_ok(&category.base.id, req.parent_category_id.as_ref())
            .await?;
        category.set_parent(req.parent_category_id, actor.id())?;
        let audit = actor.clone().resource_log(
            "product_category.move",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_categories().update(&mut category, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProductCategory, crate::errors::Error>(category)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除商品分类（软删除，乐观锁语义）。
    ///
    /// 存在子分类时拒绝删除（数据模型 §6.3：树形维护页不允许留下孤儿子树）。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 分类不存在
    /// * `BusinessLogicError` - 分类下存在子分类
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn product_category_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut category = self.load_category(id).await?;
        let children = self
            .db
            .product_categories()
            .find_children(Some(id), &mut NoTransaction)
            .await?;
        if !children.is_empty() {
            return Err(Error::BusinessLogicError(
                "分类下存在子分类，不能删除".to_string(),
            ));
        }
        let audit = actor.clone().resource_log(
            "product_category.delete",
            "product_category",
            category.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_categories()
                        .soft_delete(&mut category, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    // ---------- 商品品牌 ----------

    /// 分页查询商品品牌列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`brand_code`/`name`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_brand_list(
        &self,
        params: &ProductBrandListParams,
    ) -> Result<PageView<ProductBrandView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductBrandFilter {
            brand_code: query.brand_code,
            name: query.name,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_brands()
            .search_product_brands(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductBrandView {
                id: row.id,
                brand_code: row.brand_code,
                name: row.name,
                logo_asset_id: row.logo_asset_id,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建商品品牌（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建品牌的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - brand_code 重复（唯一索引透出）
    pub async fn product_brand_create(
        &self,
        req: CreateProductBrandRequest,
        actor: &AuditActor,
    ) -> Result<ProductBrandView> {
        req.validate()?;
        let id = ProductBrandId::new(next_id());
        let brand = ProductBrand::new(
            id.clone(),
            ProductBrandData {
                brand_code: req.brand_code,
                name: req.name,
                status: req.status.unwrap_or(EnableStatus::Active),
                logo_file_asset_id: req.logo_file_asset_id,
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("product_brand.create", "product_brand", id.to_string())?;
        self.db
            .product_brands()
            .create(&brand, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(brand.into())
    }

    /// 更新商品品牌（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 品牌 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后品牌的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 品牌不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn product_brand_update(
        &self,
        id: &str,
        req: UpdateProductBrandRequest,
        actor: &AuditActor,
    ) -> Result<ProductBrandView> {
        req.validate()?;
        let mut brand = self.load_brand(id).await?;
        ensure_version(brand.base.version, req.version)?;
        brand.update(
            ProductBrandUpdate {
                name: req.name,
                status: req.status,
                logo_file_asset_id: req.logo_file_asset_id,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("product_brand.update", "product_brand", brand.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_brands().update(&mut brand, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProductBrand, crate::errors::Error>(brand)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除商品品牌（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 品牌 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 品牌不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn product_brand_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut brand = self.load_brand(id).await?;
        let audit =
            actor
                .clone()
                .resource_log("product_brand.delete", "product_brand", brand.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.product_brands().soft_delete(&mut brand, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    // ---------- 计量单位 ----------

    /// 分页查询计量单位列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`unit_code`/`name`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn unit_of_measure_list(
        &self,
        params: &UnitOfMeasureListParams,
    ) -> Result<PageView<UnitOfMeasureView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = UnitOfMeasureFilter {
            unit_code: query.unit_code,
            name: query.name,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .unit_of_measures()
            .search_unit_of_measures(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| UnitOfMeasureView {
                id: row.id,
                unit_code: row.unit_code,
                name: row.name,
                symbol: row.symbol,
                quantity_scale: row.quantity_scale,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建计量单位（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建单位的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - unit_code 重复（唯一索引透出）
    pub async fn unit_of_measure_create(
        &self,
        req: CreateUnitOfMeasureRequest,
        actor: &AuditActor,
    ) -> Result<UnitOfMeasureView> {
        req.validate()?;
        let id = UnitOfMeasureId::new(next_id());
        let unit = UnitOfMeasure::new(
            id.clone(),
            UnitOfMeasureData {
                unit_code: req.unit_code,
                name: req.name,
                symbol: req.symbol,
                quantity_scale: req.quantity_scale,
                status: req.status.unwrap_or(EnableStatus::Active),
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.create", "unit_of_measure", id.to_string())?;
        self.db
            .unit_of_measures()
            .create(&unit, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(unit.into())
    }

    /// 更新计量单位（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后单位的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn unit_of_measure_update(
        &self,
        id: &str,
        req: UpdateUnitOfMeasureRequest,
        actor: &AuditActor,
    ) -> Result<UnitOfMeasureView> {
        req.validate()?;
        let mut unit = self.load_unit(id).await?;
        ensure_version(unit.base.version, req.version)?;
        unit.update(
            UnitOfMeasureUpdate {
                name: req.name,
                symbol: req.symbol,
                quantity_scale: req.quantity_scale,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.update", "unit_of_measure", unit.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.unit_of_measures().update(&mut unit, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<UnitOfMeasure, crate::errors::Error>(unit)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除计量单位（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 单位不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn unit_of_measure_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut unit = self.load_unit(id).await?;
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.delete", "unit_of_measure", unit.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.unit_of_measures().soft_delete(&mut unit, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    // ---------- 规格属性 ----------

    /// 分页查询规格属性列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`attribute_code`/`name`/`value_type`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_attribute_list(
        &self,
        params: &SkuAttributeListParams,
    ) -> Result<PageView<SkuAttributeView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuAttributeFilter {
            attribute_code: query.attribute_code,
            name: query.name,
            value_type: query.value_type,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sku_attributes()
            .search_sku_attributes(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuAttributeView {
                id: row.id,
                attribute_code: row.attribute_code,
                name: row.name,
                value_type: row.value_type,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建规格属性（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建属性的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `ConflictError` - attribute_code 重复（唯一索引透出）
    pub async fn sku_attribute_create(
        &self,
        req: CreateSkuAttributeRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeView> {
        req.validate()?;
        let id = SkuAttributeId::new(next_id());
        let attribute = SkuAttribute::new(
            id.clone(),
            SkuAttributeData {
                attribute_code: req.attribute_code,
                name: req.name,
                value_type: req.value_type,
                status: req.status.unwrap_or(EnableStatus::Active),
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("sku_attribute.create", "sku_attribute", id.to_string())?;
        self.db
            .sku_attributes()
            .create(&attribute, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(attribute.into())
    }

    /// 更新规格属性（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后属性的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn sku_attribute_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeView> {
        req.validate()?;
        let mut attribute = self.load_attribute(id).await?;
        ensure_version(attribute.base.version, req.version)?;
        attribute.update(
            SkuAttributeUpdate {
                name: req.name,
                value_type: req.value_type,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("sku_attribute.update", "sku_attribute", attribute.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attributes().update(&mut attribute, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SkuAttribute, crate::errors::Error>(attribute)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除规格属性（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 属性不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut attribute = self.load_attribute(id).await?;
        let audit =
            actor
                .clone()
                .resource_log("sku_attribute.delete", "sku_attribute", attribute.base.id.clone())?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attributes().soft_delete(&mut attribute, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    // ---------- 规格属性值 ----------

    /// 分页查询规格属性值列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`attribute_id`/`value_code`/`display_value`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_attribute_value_list(
        &self,
        params: &SkuAttributeValueListParams,
    ) -> Result<PageView<SkuAttributeValueView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuAttributeValueFilter {
            attribute_id: query.attribute_id,
            value_code: query.value_code,
            display_value: query.display_value,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sku_attribute_values()
            .search_sku_attribute_values(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuAttributeValueView {
                id: row.id,
                attribute_id: row.attribute_id,
                value_code: row.value_code,
                display_value: row.display_value,
                sort_order: row.sort_order,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建规格属性值（单集合写入，无事务）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建属性值的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 所属规格属性不存在
    /// * `ConflictError` - 同一属性下 value_code 重复（唯一索引透出）
    pub async fn sku_attribute_value_create(
        &self,
        req: CreateSkuAttributeValueRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeValueView> {
        req.validate()?;
        self.load_attribute(req.attribute_id.as_ref()).await?;
        let id = SkuAttributeValueId::new(next_id());
        let value = SkuAttributeValue::new(
            id.clone(),
            SkuAttributeValueData {
                attribute_id: req.attribute_id,
                value_code: req.value_code,
                display_value: req.display_value,
                sort_order: req.sort_order,
                status: req.status.unwrap_or(EnableStatus::Active),
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "sku_attribute_value.create",
            "sku_attribute_value",
            id.to_string(),
        )?;
        self.db
            .sku_attribute_values()
            .create(&value, &mut NoTransaction)
            .await?;
        self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
        Ok(value.into())
    }

    /// 更新规格属性值（乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后属性值的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    pub async fn sku_attribute_value_update(
        &self,
        id: &str,
        req: UpdateSkuAttributeValueRequest,
        actor: &AuditActor,
    ) -> Result<SkuAttributeValueView> {
        req.validate()?;
        let mut value = self.load_attribute_value(id).await?;
        ensure_version(value.base.version, req.version)?;
        value.update(
            SkuAttributeValueUpdate {
                display_value: req.display_value,
                sort_order: req.sort_order,
                status: req.status,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "sku_attribute_value.update",
            "sku_attribute_value",
            value.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attribute_values().update(&mut value, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SkuAttributeValue, crate::errors::Error>(value)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 删除规格属性值（软删除，乐观锁语义）。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回删除结果。
    ///
    /// # 错误
    /// * `NotFound` - 属性值不存在
    /// * `ConflictError` - 并发修改（CAS 冲突）
    pub async fn sku_attribute_value_delete(&self, id: &str, actor: &AuditActor) -> Result<()> {
        let mut value = self.load_attribute_value(id).await?;
        let audit = actor.clone().resource_log(
            "sku_attribute_value.delete",
            "sku_attribute_value",
            value.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.sku_attribute_values().soft_delete(&mut value, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await
    }

    // ---------- 商品（SPU + SKU） ----------

    /// 分页查询商品列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`product_no`/`product_kind`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_list(&self, params: &ProductListParams) -> Result<PageView<ProductView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductFilter {
            product_no: query.product_no,
            product_kind: query.product_kind,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .products()
            .search_products(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| ProductView {
                id: row.id,
                product_no: row.product_no,
                product_kind: row.product_kind,
                status: row.status,
                current_revision_id: row.current_revision_id,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 创建商品（SPU + 首个商品修订 + 媒体 + 全部 SKU 行，跨集合事务）。
    ///
    /// 数据模型 §6.3：`product_no`/`sku_no`/`(product_id, specification_signature)`
    /// 唯一由唯一索引兜底（`DuplicateKey` → 409）；新签名分配新 `sku_id`；
    /// 条码冲突阻断；分类必须允许商品类型；规格属性必须适用于所选分类。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建商品的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - 分类/品牌/基础单位/媒体文件不存在
    /// * `BusinessLogicError` - 分类不允许商品类型、规格不适用于分类、条码冲突等
    /// * `ConflictError` - 唯一约束冲突或并发事务冲突
    pub async fn product_create(&self, req: CreateProductRequest, actor: &AuditActor) -> Result<ProductView> {
        req.validate()?;
        let draft = self.build_product_draft(req, actor).await?;
        let product = self.write_product_draft(draft, actor).await?;
        Ok(product.into())
    }

    /// 规格编辑商品（数据模型 §6.3 全量替换语义，跨集合事务）。
    ///
    /// 按规范化签名把提交前后的签名集合分类为「保留/新增/重新启用/移除」：
    /// 签名未变沿用原 `sku_id` 并追加修订；从未存在的新签名分配新 `sku_id`；
    /// 历史停用签名复用原 `sku_id` 并显式重新启用；移除签名的旧 SKU 保留
    /// 全部历史并转为停用。任一校验失败或并发冲突整体回滚。
    ///
    /// # 参数
    /// * `id` - 商品 ID
    /// * `req` - 规格编辑请求（含期望版本与修订后全部 SKU 行）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回编辑后商品的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 商品/分类/品牌/基础单位/媒体文件不存在
    /// * `ConflictError` - 期望版本与当前版本不一致，或并发事务冲突
    /// * `BusinessLogicError` - 分类不允许商品类型、规格不适用于分类、条码冲突等
    pub async fn product_update(
        &self,
        id: &str,
        req: UpdateProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductView> {
        req.validate()?;
        let mut product = self.load_product(id).await?;
        ensure_version(product.base.version, req.version)?;
        let plan = self.build_spec_edit_plan(&mut product, req, actor).await?;
        let product = self.write_spec_edit_plan(plan, actor).await?;
        Ok(product.into())
    }

    /// 分页查询商品修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`product_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn product_revision_list(
        &self,
        params: &ProductRevisionListParams,
    ) -> Result<PageView<ProductRevisionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = ProductRevisionFilter {
            product_id: query.product_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .product_revisions()
            .search_product_revisions(&filter, &mut NoTransaction)
            .await?;
        let media_by_revision = self
            .media_by_revision_ids(
                &page
                    .items
                    .iter()
                    .map(|row| ProductRevisionId::new(row.id.clone()))
                    .collect::<Vec<_>>(),
            )
            .await?;
        // 投影行类型属于仓储私有子树（`repository/mod.rs` 冻结），按字段映射为响应视图。
        let items = page
            .items
            .into_iter()
            .map(|row| ProductRevisionView {
                id: row.id.clone(),
                product_id: row.product_id,
                revision_no: row.revision_no,
                name: row.name,
                description: row.description,
                specification: row.specification,
                category_id: row.category_id,
                brand_id: row.brand_id,
                status: row.status,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                media: media_by_revision.get(&row.id).cloned().unwrap_or_default(),
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询 SKU 列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_no`/`product_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_list(&self, params: &SkuListParams) -> Result<PageView<SkuView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuFilter {
            sku_no: query.sku_no,
            product_id: query.product_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self.db.skus().search_skus(&filter, &mut NoTransaction).await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuView {
                id: row.id,
                sku_no: row.sku_no,
                product_id: row.product_id,
                base_unit_id: row.base_unit_id,
                specification_signature: row.specification_signature,
                status: row.status,
                current_revision_id: row.current_revision_id,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 分页查询 SKU 修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_id`/`name`/`barcode`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn sku_revision_list(
        &self,
        params: &SkuRevisionListParams,
    ) -> Result<PageView<SkuRevisionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SkuRevisionFilter {
            sku_id: query.sku_id,
            name: query.name,
            barcode: query.barcode,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .sku_revisions()
            .search_sku_revisions(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SkuRevisionView {
                id: row.id,
                sku_id: row.sku_id,
                revision_no: row.revision_no,
                name: row.name,
                description: row.description,
                specification: row.specification,
                barcode: row.barcode,
                source_main_image_asset_id: row.source_main_image_asset_id,
                weight_kg: row.weight_kg,
                volume_m3: row.volume_m3,
                status: row.status,
                sales_visible_price_gross: row.sales_visible_price_gross,
                market_price: row.market_price,
                effective_from: row.effective_from,
                effective_to: row.effective_to,
                created_at: row.created_at,
                version: row.version,
            })
            .collect();
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    // ---------- 卡券类目扩展 ----------

    /// 分页查询卡券类目扩展修订列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`sku_id`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn voucher_category_profile_list(
        &self,
        params: &VoucherCategoryProfileListParams,
    ) -> Result<PageView<VoucherCategoryProfileView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = VoucherProfileFilter {
            sku_id: query.sku_id,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .voucher_category_profile_revisions()
            .search_voucher_category_profile_revisions(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let mut view = VoucherCategoryProfileView {
                id: row.id,
                sku_id: row.sku_id,
                sku_no: None,
                product_id: None,
                product_version: None,
                name: None,
                revision_no: row.revision_no,
                description: row.description,
                status: row.status,
                created_at: row.created_at,
                version: row.version,
            };
            self.enrich_voucher_category_view(&mut view).await?;
            items.push(view);
        }
        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 更新卡券类目名称与描述（按 SKU 稳定身份定位，追加商品/SKU/扩展修订）。
    ///
    /// 分类、品牌、基础单位与编号保持不变；乐观锁使用所属商品 `product.version`。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目对应的 VOUCHER SKU 稳定 ID
    /// * `req` - 更新请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建的卡券类目扩展修订视图（含名称与商品版本）。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败
    /// * `NotFound` - SKU/商品/当前修订不存在，或 SKU 非 VOUCHER 商品
    /// * `ConflictError` - 商品乐观锁冲突或并发写入冲突
    pub async fn voucher_category_update(
        &self,
        sku_id: &str,
        req: UpdateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileView> {
        req.validate()?;
        let draft = self
            .build_voucher_category_update_draft(sku_id, req, actor)
            .await?;
        let revision = self.write_voucher_category_update_draft(draft, actor).await?;
        let mut view = VoucherCategoryProfileView::from(revision);
        self.enrich_voucher_category_view(&mut view).await?;
        Ok(view)
    }

    /// 原子创建卡券类目（商品 + 首个修订 + 唯一 SKU + [可选内联新建分类] +
    /// 卡券类目扩展修订，同一事务写入）。
    ///
    /// 业务上一个卡券类目即一个 VOUCHER 类型的 SKU：`voucher_no` 同时作为
    /// `product_no` 与 `sku_no`。分类 / 品牌 / 基础单位可省略：
    /// - 分类：缺省挂到共用卡券根分类（代码 `VOUCHER`）；
    /// - 品牌：缺省「福尚云」；
    /// - 基础单位：缺省「张」。
    /// 上述默认字典不存在时由本方法自动创建（单集合写入，位于事务外）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建卡券类目扩展修订的响应视图。
    ///
    /// # 错误
    /// * `ValidationError` - 请求体校验失败，或分类与内联新建同时给出
    /// * `NotFound` - 显式引用的分类/品牌/基础单位不存在
    /// * `BusinessLogicError` - 分类不允许 VOUCHER 类型、父子关系成环、基础单位已停用
    /// * `ConflictError` - `product_no`/`sku_no`/`category_code`/条码 唯一约束冲突
    pub async fn voucher_category_create(
        &self,
        req: CreateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileView> {
        req.validate()?;
        let draft = self.build_voucher_category_draft(req, actor).await?;
        let revision = self.write_voucher_category_draft(draft, actor).await?;
        let mut view = VoucherCategoryProfileView::from(revision);
        self.enrich_voucher_category_view(&mut view).await?;
        Ok(view)
    }

    // ---------- 私有加载与写入辅助 ----------

    /// 按 ID 加载未删除分类。
    ///
    /// # 参数
    /// * `id` - 分类 ID
    ///
    /// # 返回
    /// 返回分类实体。
    ///
    /// # 错误
    /// 分类不存在时返回 `NotFound`。
    async fn load_category(&self, id: &str) -> Result<ProductCategory> {
        self.db
            .product_categories()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品分类不存在".to_string()))
    }

    /// 按 ID 加载未删除品牌。
    ///
    /// # 参数
    /// * `id` - 品牌 ID
    ///
    /// # 返回
    /// 返回品牌实体。
    ///
    /// # 错误
    /// 品牌不存在时返回 `NotFound`。
    async fn load_brand(&self, id: &str) -> Result<ProductBrand> {
        self.db
            .product_brands()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品品牌不存在".to_string()))
    }

    /// 按 ID 加载未删除计量单位。
    ///
    /// # 参数
    /// * `id` - 单位 ID
    ///
    /// # 返回
    /// 返回单位实体。
    ///
    /// # 错误
    /// 单位不存在时返回 `NotFound`。
    async fn load_unit(&self, id: &str) -> Result<UnitOfMeasure> {
        self.db
            .unit_of_measures()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("计量单位不存在".to_string()))
    }

    /// 按 ID 加载未删除规格属性。
    ///
    /// # 参数
    /// * `id` - 属性 ID
    ///
    /// # 返回
    /// 返回属性实体。
    ///
    /// # 错误
    /// 属性不存在时返回 `NotFound`。
    async fn load_attribute(&self, id: &str) -> Result<SkuAttribute> {
        self.db
            .sku_attributes()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性不存在".to_string()))
    }

    /// 按 ID 加载未删除规格属性值。
    ///
    /// # 参数
    /// * `id` - 属性值 ID
    ///
    /// # 返回
    /// 返回属性值实体。
    ///
    /// # 错误
    /// 属性值不存在时返回 `NotFound`。
    async fn load_attribute_value(&self, id: &str) -> Result<SkuAttributeValue> {
        self.db
            .sku_attribute_values()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("规格属性值不存在".to_string()))
    }

    /// 按 ID 加载未删除商品。
    ///
    /// # 参数
    /// * `id` - 商品 ID
    ///
    /// # 返回
    /// 返回商品实体。
    ///
    /// # 错误
    /// 商品不存在时返回 `NotFound`。
    async fn load_product(&self, id: &str) -> Result<Product> {
        self.db
            .products()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("商品不存在".to_string()))
    }

    /// 校验新父分类的祖先链不包含本节点（成环检测）。
    ///
    /// # 参数
    /// * `id` - 本节点 ID
    /// * `parent_id` - 新父分类（`None` 为根）
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 父分类不存在或沿祖先链命中本节点时返回错误。
    async fn ensure_parent_chain_ok(&self, id: &str, parent_id: Option<&ProductCategoryId>) -> Result<()> {
        let mut cursor = parent_id.cloned();
        while let Some(current) = cursor {
            if current.as_ref() == id {
                return Err(Error::BusinessLogicError("父子关系不能形成环".to_string()));
            }
            let parent = self
                .db
                .product_categories()
                .find_by_id(current.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("父分类不存在".to_string()))?;
            cursor = parent.parent_category_id;
        }
        Ok(())
    }

    /// 构造商品创建草稿（全部 ID 预生成，事务外完成全部业务校验）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的草稿。
    ///
    /// # 错误
    /// 字典/媒体/规格校验失败时返回对应错误。
    async fn build_product_draft(
        &self,
        req: CreateProductRequest,
        actor: &AuditActor,
    ) -> Result<ProductDraft> {
        self.ensure_product_dictionaries(&req.category_id, &req.brand_id, &req.skus, req.product_kind)
            .await?;
        let product_id = ProductId::new(next_id());
        let revision_id = ProductRevisionId::new(next_id());
        let status = req.status.unwrap_or(EnableStatus::Active);
        let product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: req.product_no,
                product_kind: req.product_kind,
                status,
            },
            actor.id(),
        )?;
        let media = self
            .build_media_rows(&revision_id, &req.carousel_media, MediaRole::Carousel)
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail)
                    .await?,
            )
            .collect::<Vec<_>>();
        let mut sku_items = Vec::with_capacity(req.skus.len());
        for sku_input in req.skus {
            if sku_input.sku_id.is_some()
                || sku_input.expected_sku_revision_id.is_some()
                || sku_input.reenable
            {
                return Err(Error::ValidationError(
                    "新建商品的 SKU 不得指定既有身份或重新启用意图".to_string(),
                ));
            }
            let item = self
                .build_new_sku_item(
                    NewSkuContext {
                        product_id: &product_id,
                        product_name: &req.name,
                        category_id: &req.category_id,
                        effective_from: req.effective_from,
                        effective_to: req.effective_to,
                        created_by: actor.id(),
                    },
                    sku_input,
                )
                .await?;
            sku_items.push(item);
        }
        let mut product = product;
        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id,
                revision_no: 1,
                name: req.name,
                description: req.description,
                specification: req.specification,
                category_id: req.category_id,
                brand_id: req.brand_id,
                status,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(ProductDraft {
            change_reason: req.change_reason,
            product,
            revision,
            media,
            sku_items,
        })
    }

    /// 构造卡券类目原子创建草稿（分类二选一 + 品牌/单位校验 + 商品/SKU/卡券
    /// 修订预生成，全部 ID 在事务外预生成，事务内只做写入）。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的草稿。
    ///
    /// # 错误
    /// * `ValidationError` - `category_id` 与 `new_category` 同时给出
    /// * `NotFound` - 显式引用的分类/父分类/品牌/基础单位不存在
    /// * `BusinessLogicError` - 分类不允许 VOUCHER 类型、父子关系成环、基础单位已停用、条码冲突
    async fn build_voucher_category_draft(
        &self,
        req: CreateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryDraft> {
        let CreateVoucherCategoryRequest {
            voucher_no,
            name,
            description,
            specification,
            category_id,
            new_category,
            brand_id,
            sku,
            status,
            effective_from,
            effective_to,
        } = req;

        ensure_category_selection_exclusive(&category_id, &new_category)?;
        let (category_id, new_category) = match (category_id, new_category) {
            (Some(category_id), None) => {
                let category = self.load_category(category_id.as_ref()).await?;
                if category.product_kind != ProductKind::Voucher {
                    return Err(Error::BusinessLogicError(
                        "所选分类不允许 VOUCHER 类型".to_string(),
                    ));
                }
                (category_id, None)
            }
            (None, Some(new_category_input)) => {
                let new_category_id = ProductCategoryId::new(next_id());
                self.ensure_parent_chain_ok(
                    new_category_id.as_ref(),
                    new_category_input.parent_category_id.as_ref(),
                )
                .await?;
                let category = ProductCategory::new(
                    new_category_id.clone(),
                    ProductCategoryData {
                        category_code: new_category_input.category_code,
                        parent_category_id: new_category_input.parent_category_id,
                        name: new_category_input.name,
                        product_kind: ProductKind::Voucher,
                        status: EnableStatus::Active,
                    },
                    actor.id(),
                )?;
                (new_category_id, Some(category))
            }
            (None, None) => {
                let root_id = self.ensure_voucher_root_category(actor).await?;
                (root_id, None)
            }
            (Some(_), Some(_)) => {
                unreachable!("ensure_category_selection_exclusive 已拒绝同时给出")
            }
        };

        let brand_id = match brand_id {
            Some(brand_id) => {
                self.load_brand(brand_id.as_ref()).await?;
                brand_id
            }
            None => self.ensure_voucher_default_brand(actor).await?,
        };

        let sku = match sku {
            Some(sku) => sku,
            None => VoucherSkuInput {
                base_unit_id: self.ensure_voucher_default_unit(actor).await?,
                barcode: None,
                weight_kg: None,
                volume_m3: None,
                sales_visible_price_gross: None,
                market_price: None,
            },
        };

        self.ensure_brand_and_unit_ok(&brand_id, std::iter::once(&sku.base_unit_id))
            .await?;

        let effective_from = effective_from.unwrap_or_else(BusinessDate::today);
        let product_id = ProductId::new(next_id());
        let revision_id = ProductRevisionId::new(next_id());
        let product_status = status.unwrap_or(EnableStatus::Active);
        let mut product = Product::new(
            product_id.clone(),
            ProductData {
                product_no: voucher_no.clone(),
                product_kind: ProductKind::Voucher,
                status: product_status,
            },
            actor.id(),
        )?;

        let sku_item = self
            .build_new_sku_item(
                NewSkuContext {
                    product_id: &product_id,
                    product_name: &name,
                    category_id: &category_id,
                    effective_from,
                    effective_to,
                    created_by: actor.id(),
                },
                ProductSkuInput {
                    sku_id: None,
                    expected_sku_revision_id: None,
                    reenable: false,
                    sku_no: voucher_no,
                    base_unit_id: sku.base_unit_id,
                    barcode: sku.barcode,
                    main_image_asset_id: None,
                    weight_kg: sku.weight_kg,
                    volume_m3: sku.volume_m3,
                    sales_visible_price_gross: sku.sales_visible_price_gross,
                    market_price: sku.market_price,
                    spec_entries: Vec::new(),
                },
            )
            .await?;

        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id,
                revision_no: 1,
                name,
                description: Some(description.clone()),
                specification,
                category_id,
                brand_id,
                status: product_status,
                effective_from,
                effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());

        let voucher_revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new(next_id()),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new(sku_item.sku.base.id.clone()),
                revision_no: 1,
                description,
                status: product_status,
            },
        )?;

        Ok(VoucherCategoryDraft {
            new_category,
            product,
            revision,
            sku_item,
            voucher_revision,
        })
    }

    /// 确保共用卡券根分类存在（代码 `VOUCHER` / 名称「卡券」/ `product_kind=VOUCHER`）。
    ///
    /// 已存在则直接返回其 ID；不存在则创建为根分类。并发创建冲突时重新查询一次。
    ///
    /// # 参数
    /// * `actor` - 审计操作人（仅在需要新建时写入 `created_by`）
    ///
    /// # 返回
    /// 返回卡券根分类稳定 ID。
    ///
    /// # 错误
    /// * `BusinessLogicError` - 已有同代码分类但 `product_kind` 不是 VOUCHER
    /// * `ConflictError` / `RepositoryError` - 持久化失败
    async fn ensure_voucher_root_category(&self, actor: &AuditActor) -> Result<ProductCategoryId> {
        if let Some(existing) = self
            .db
            .product_categories()
            .find_one_by_field(
                "category_code",
                VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            if existing.product_kind != ProductKind::Voucher {
                return Err(Error::BusinessLogicError(format!(
                    "系统卡券根分类（代码 {VOUCHER_ROOT_CATEGORY_CODE}）的商品类型不是卡券，请修正后再创建卡券类目"
                )));
            }
            return Ok(ProductCategoryId::new(existing.base.id));
        }

        let id = ProductCategoryId::new(next_id());
        let category = ProductCategory::new(
            id.clone(),
            ProductCategoryData {
                category_code: VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                parent_category_id: None,
                name: VOUCHER_ROOT_CATEGORY_NAME.to_string(),
                product_kind: ProductKind::Voucher,
                status: EnableStatus::Active,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("product_category.create", "product_category", id.to_string())?;
        match self
            .db
            .product_categories()
            .create(&category, &mut NoTransaction)
            .await
        {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .product_categories()
                        .find_one_by_field(
                            "category_code",
                            VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| Error::ConflictError("卡券根分类并发创建冲突，请重试".to_string()))?;
                    Ok(ProductCategoryId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }

    /// 确保卡券默认品牌「福尚云」存在（代码 `FSY`）。
    ///
    /// 优先按 `brand_code=FSY` 查找；若无则按名称「福尚云」查找；仍无则创建。
    ///
    /// # 参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回品牌稳定 ID。
    ///
    /// # 错误
    /// 持久化失败时返回对应错误；并发冲突时重新查询。
    async fn ensure_voucher_default_brand(&self, actor: &AuditActor) -> Result<ProductBrandId> {
        if let Some(existing) = self
            .db
            .product_brands()
            .find_one_by_field(
                "brand_code",
                VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            return Ok(ProductBrandId::new(existing.base.id));
        }
        if let Some(existing) = self
            .db
            .product_brands()
            .find_one_by_field("name", VOUCHER_DEFAULT_BRAND_NAME.to_string(), &mut NoTransaction)
            .await?
        {
            return Ok(ProductBrandId::new(existing.base.id));
        }

        let id = ProductBrandId::new(next_id());
        let brand = ProductBrand::new(
            id.clone(),
            ProductBrandData {
                brand_code: VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                name: VOUCHER_DEFAULT_BRAND_NAME.to_string(),
                status: EnableStatus::Active,
                logo_file_asset_id: None,
            },
            actor.id(),
        )?;
        let audit = actor
            .clone()
            .resource_log("product_brand.create", "product_brand", id.to_string())?;
        match self.db.product_brands().create(&brand, &mut NoTransaction).await {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .product_brands()
                        .find_one_by_field(
                            "brand_code",
                            VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("默认品牌福尚云并发创建冲突，请重试".to_string())
                        })?;
                    Ok(ProductBrandId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }

    /// 确保卡券默认基础单位「张」存在（代码/名称/符号均为「张」，整数数量）。
    ///
    /// # 参数
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回计量单位稳定 ID。
    ///
    /// # 错误
    /// 持久化失败时返回对应错误；并发冲突时重新查询。
    async fn ensure_voucher_default_unit(&self, actor: &AuditActor) -> Result<UnitOfMeasureId> {
        if let Some(existing) = self
            .db
            .unit_of_measures()
            .find_one_by_field(
                "unit_code",
                VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                &mut NoTransaction,
            )
            .await?
        {
            if !existing.is_active() {
                return Err(Error::BusinessLogicError(
                    "默认单位「张」已停用，请启用后再创建卡券类目".to_string(),
                ));
            }
            return Ok(UnitOfMeasureId::new(existing.base.id));
        }

        let id = UnitOfMeasureId::new(next_id());
        let unit = UnitOfMeasure::new(
            id.clone(),
            UnitOfMeasureData {
                unit_code: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                name: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                symbol: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                quantity_scale: 0,
                status: EnableStatus::Active,
            },
            actor.id(),
        )?;
        let audit =
            actor
                .clone()
                .resource_log("unit_of_measure.create", "unit_of_measure", id.to_string())?;
        match self.db.unit_of_measures().create(&unit, &mut NoTransaction).await {
            Ok(()) => {
                self.db.audit_logs().create(&audit, &mut NoTransaction).await?;
                Ok(id)
            }
            Err(err) => match Error::from(err) {
                Error::ConflictError(_) => {
                    let existing = self
                        .db
                        .unit_of_measures()
                        .find_one_by_field(
                            "unit_code",
                            VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| {
                            Error::ConflictError("默认单位「张」并发创建冲突，请重试".to_string())
                        })?;
                    Ok(UnitOfMeasureId::new(existing.base.id))
                }
                other => Err(other),
            },
        }
    }

    /// 校验商品创建/编辑引用的字典（分类/品牌/基础单位）与分类-商品类型兼容性。
    ///
    /// # 参数
    /// * `category_id` - ERP 分类
    /// * `brand_id` - ERP 品牌
    /// * `skus` - SKU 行（校验每个基础单位存在且启用）
    /// * `product_kind` - 商品业务类型
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 字典不存在/停用或分类不允许商品类型时返回错误。
    async fn ensure_product_dictionaries(
        &self,
        category_id: &ProductCategoryId,
        brand_id: &ProductBrandId,
        skus: &[ProductSkuInput],
        product_kind: ProductKind,
    ) -> Result<()> {
        let category = self.load_category(category_id.as_ref()).await?;
        if category.product_kind != product_kind {
            return Err(Error::BusinessLogicError("所选分类不允许该商品类型".to_string()));
        }
        self.ensure_brand_and_unit_ok(brand_id, skus.iter().map(|sku| &sku.base_unit_id))
            .await
    }

    /// 校验品牌存在、每个基础单位存在且启用（不要求分类已落库，供卡券类目
    /// 原子创建复用——此时分类可能是本次事务内才新建的草稿）。
    ///
    /// # 参数
    /// * `brand_id` - ERP 品牌
    /// * `base_unit_ids` - 待校验的基础单位 ID 集合
    ///
    /// # 返回
    /// 合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 品牌/单位不存在或单位已停用时返回错误。
    async fn ensure_brand_and_unit_ok<'a>(
        &self,
        brand_id: &ProductBrandId,
        base_unit_ids: impl Iterator<Item = &'a UnitOfMeasureId>,
    ) -> Result<()> {
        self.load_brand(brand_id.as_ref()).await?;
        for unit_id in base_unit_ids {
            let unit = self.load_unit(unit_id.as_ref()).await?;
            if !unit.is_active() {
                return Err(Error::BusinessLogicError("基础单位已停用".to_string()));
            }
        }
        Ok(())
    }

    /// 构造媒体行（校验 `file_asset` 引用存在，媒体角色与顺序落位）。
    ///
    /// # 参数
    /// * `revision_id` - 所属商品修订
    /// * `inputs` - 媒体输入
    /// * `role` - 媒体用途
    ///
    /// # 返回
    /// 返回媒体实体集合。
    ///
    /// # 错误
    /// 媒体文件不存在或同用途顺序重复时返回错误。
    async fn build_media_rows(
        &self,
        revision_id: &ProductRevisionId,
        inputs: &[ProductMediaInput],
        role: MediaRole,
    ) -> Result<Vec<ProductRevisionMedia>> {
        let mut rows = Vec::with_capacity(inputs.len());
        for input in inputs {
            let asset = self
                .db
                .file_assets()
                .find_by_id(input.file_asset_id.as_ref(), &mut NoTransaction)
                .await?
                .ok_or_else(|| Error::NotFound("媒体文件不存在".to_string()))?;
            if !asset.is_usable_for_business(entities::common::time::Instant::now()) {
                return Err(Error::BusinessLogicError("媒体文件不可用于业务".to_string()));
            }
            let row = ProductRevisionMedia::new(
                ProductRevisionMediaId::new(next_id()),
                ProductRevisionMediaData {
                    product_revision_id: revision_id.clone(),
                    file_asset_id: input.file_asset_id.clone(),
                    media_role: role,
                    sort_order: input.sort_order,
                    alt_text: input.alt_text.clone(),
                },
            )?;
            rows.push(row);
        }
        ensure_unique_sort_orders(rows.iter().map(|row| row.sort_order), "媒体展示顺序")?;
        Ok(rows)
    }

    /// 按修订 ID 批量读取媒体行并映射为响应视图（按修订分组）。
    ///
    /// # 参数
    /// * `revision_ids` - 商品修订 ID 集合
    ///
    /// # 返回
    /// 返回 `修订 ID → 媒体视图列表` 的分组映射。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn media_by_revision_ids(
        &self,
        revision_ids: &[ProductRevisionId],
    ) -> Result<HashMap<String, Vec<ProductRevisionMediaView>>> {
        if revision_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let rows = self
            .db
            .product_revision_medias()
            .find_media_by_revision_ids(revision_ids, &mut NoTransaction)
            .await?;
        let mut by_revision: HashMap<String, Vec<ProductRevisionMediaView>> = HashMap::new();
        for row in rows {
            by_revision
                .entry(row.product_revision_id.to_string())
                .or_default()
                .push(ProductRevisionMediaView {
                    id: row.base.id,
                    file_asset_id: row.file_asset_id.to_string(),
                    media_role: row.media_role,
                    sort_order: row.sort_order,
                    alt_text: row.alt_text,
                });
        }
        for views in by_revision.values_mut() {
            views.sort_by_key(|view| view.sort_order);
        }
        Ok(by_revision)
    }

    /// 构造新 SKU 行（解析规格 → 计算签名 → 生成 SKU 身份 + 首个修订 + 规格值）。
    ///
    /// # 参数
    /// * `ctx` - 新 SKU 上下文（所属 SPU、名称快照、分类、生效区间、操作人）
    /// * `input` - SKU 输入行
    ///
    /// # 返回
    /// 返回 `Create` 动作的规格编辑行。
    ///
    /// # 错误
    /// 规格属性/值不存在、签名冲突、条码冲突时返回错误。
    async fn build_new_sku_item(
        &self,
        ctx: NewSkuContext<'_>,
        input: ProductSkuInput,
    ) -> Result<SkuEditItem> {
        let (signature, resolved) = self
            .resolve_spec_entries(ctx.category_id, &input.spec_entries)
            .await?;
        self.ensure_barcode_available(&input.barcode, None).await?;
        let sku_id = SkuId::new(next_id());
        let revision_id = SkuRevisionId::new(next_id());
        let revision = SkuRevision::new(
            revision_id.clone(),
            SkuRevisionData {
                sku_id: sku_id.clone(),
                revision_no: 1,
                name: ctx.product_name.to_string(),
                description: None,
                specification: None,
                barcode: input.barcode,
                source_main_image_asset_id: input.main_image_asset_id.clone(),
                weight_kg: input.weight_kg,
                volume_m3: input.volume_m3,
                sales_visible_price_gross: input.sales_visible_price_gross,
                market_price: input.market_price,
                status: EnableStatus::Active,
                effective_from: ctx.effective_from,
                effective_to: ctx.effective_to,
            },
        )?;
        let attribute_values = build_attribute_value_rows(&revision_id, &resolved)?;
        let mut sku = Sku::new(
            sku_id,
            SkuData {
                sku_no: input.sku_no,
                product_id: ctx.product_id.clone(),
                base_unit_id: input.base_unit_id,
                specification_signature: signature,
                status: EnableStatus::Active,
            },
            ctx.created_by,
        )?;
        sku.stable.current_revision_id = Some(revision.base.id.clone());
        Ok(SkuEditItem {
            action: SkuEditAction::Create,
            sku,
            revision,
            attribute_values,
        })
    }

    /// 解析规格属性-值对为字典身份并计算规范化签名。
    ///
    /// 枚举属性校验属性值存在且启用；文本属性以文本原值作为签名值；
    /// 分类定义了适用属性时逐条校验归属；身份排序位按属性代码排序后落位。
    ///
    /// # 参数
    /// * `category_id` - ERP 分类
    /// * `entries` - 规格输入
    ///
    /// # 返回
    /// 返回 `(签名输入, 解析后的属性值行)`。
    ///
    /// # 错误
    /// 属性/值不存在、属性停用、不适用于分类或签名冲突时返回错误。
    async fn resolve_spec_entries(
        &self,
        category_id: &ProductCategoryId,
        entries: &[SpecEntryInput],
    ) -> Result<(String, Vec<ResolvedSpecEntry>)> {
        let applicable = self.applicable_attribute_ids(category_id).await?;
        let mut resolved = Vec::with_capacity(entries.len());
        for entry in entries {
            let attribute = self
                .db
                .sku_attributes()
                .find_one_by_field(
                    "attribute_code",
                    entry.attribute_code.trim().to_string(),
                    &mut NoTransaction,
                )
                .await?
                .ok_or_else(|| Error::NotFound(format!("规格属性不存在: {}", entry.attribute_code)))?;
            if !attribute.is_active() {
                return Err(Error::BusinessLogicError(format!(
                    "规格属性已停用: {}",
                    entry.attribute_code
                )));
            }
            if !applicable.is_empty() && !applicable.contains(&attribute.base.id) {
                return Err(Error::BusinessLogicError(format!(
                    "规格属性不适用于该商品分类: {}",
                    entry.attribute_code
                )));
            }
            let value_code = entry.attribute_value_code.trim().to_string();
            let resolved_entry = match attribute.value_type {
                AttributeValueType::Enum => {
                    let value = self
                        .db
                        .sku_attribute_values()
                        .find_one(
                            doc! { "attribute_id": attribute.base.id.clone(), "value_code": &value_code },
                            &mut NoTransaction,
                        )
                        .await?
                        .ok_or_else(|| Error::NotFound(format!("属性值不存在: {}", value_code)))?;
                    ResolvedSpecEntry {
                        attribute_code: attribute.attribute_code.clone(),
                        value_code: value.value_code.clone(),
                        attribute_id: attribute.base.id.clone().into(),
                        attribute_value_id: Some(value.base.id.clone().into()),
                        normalized_text_value: None,
                    }
                }
                AttributeValueType::Text => ResolvedSpecEntry {
                    attribute_code: attribute.attribute_code.clone(),
                    value_code: value_code.clone(),
                    attribute_id: attribute.base.id.clone().into(),
                    attribute_value_id: None,
                    normalized_text_value: Some(value_code.clone()),
                },
            };
            resolved.push(resolved_entry);
        }
        let signature_entries: Vec<SpecSignatureEntry> = resolved
            .iter()
            .map(|entry| SpecSignatureEntry {
                attribute_code: entry.attribute_code.clone(),
                value_code: entry.value_code.clone(),
            })
            .collect();
        let signature = compute_specification_signature(&signature_entries)?;
        resolved.sort_by(|left, right| left.attribute_code.cmp(&right.attribute_code));
        Ok((signature, resolved))
    }

    /// 查询分类定义的适用属性 ID 集合（未定义时返回空集合表示不限制）。
    ///
    /// # 参数
    /// * `category_id` - ERP 分类
    ///
    /// # 返回
    /// 返回适用属性 ID 集合。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn applicable_attribute_ids(&self, category_id: &ProductCategoryId) -> Result<Vec<String>> {
        let relations = self
            .db
            .product_category_attributes()
            .find_by_category_ids(std::slice::from_ref(category_id), &mut NoTransaction)
            .await?;
        Ok(relations
            .into_iter()
            .map(|relation| relation.attribute_id.to_string())
            .collect())
    }

    /// 校验条码未被其他在用 SKU 使用（数据模型 §6.3：冲突转人工，不自动合并）。
    ///
    /// # 参数
    /// * `barcode` - 条码原值
    /// * `current_sku_id` - 本次写入归属的 SKU（同 SKU 自身修订不视为冲突）
    ///
    /// # 返回
    /// 可用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 条码已被其他在用 SKU 使用时返回 `BusinessLogicError`。
    async fn ensure_barcode_available(
        &self,
        barcode: &Option<String>,
        current_sku_id: Option<&str>,
    ) -> Result<()> {
        let Some(barcode) = barcode
            .as_deref()
            .map(str::trim)
            .filter(|value| !value.is_empty())
        else {
            return Ok(());
        };
        let active = self
            .db
            .sku_revisions()
            .find_active_by_barcode(barcode, &mut NoTransaction)
            .await?;
        if active
            .iter()
            .any(|revision| revision.sku_id.as_ref() != current_sku_id.unwrap_or_default())
        {
            return Err(Error::BusinessLogicError(format!(
                "条码已被其他在用SKU使用: {barcode}"
            )));
        }
        Ok(())
    }

    /// 计算某 SKU 已有修订的最大序号 + 1（唯一索引兜底并发）。
    ///
    /// # 参数
    /// * `sku_id` - SKU
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_sku_revision_no(&self, sku_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .sku_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// 在单个事务内写入商品创建草稿并返回 SPU。
    ///
    /// 依次写入 `products`、`product_revisions` + 媒体、每个 SKU 的
    /// `skus` + `sku_revisions` + 规格属性值，以及审计日志。
    ///
    /// # 参数
    /// * `draft` - 创建草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的 SPU 实体。
    ///
    /// # 错误
    /// 唯一索引冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_product_draft(&self, draft: ProductDraft, actor: &AuditActor) -> Result<Product> {
        let ProductDraft {
            change_reason,
            product,
            revision,
            media,
            sku_items,
        } = draft;
        let audit = actor.clone().resource_log_with_message(
            "product.create",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().create(&product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &media, session)
                        .await?;
                    for item in &sku_items {
                        db.catalog()
                            .create_sku_with_revision(
                                &item.sku,
                                &item.revision,
                                &item.attribute_values,
                                session,
                            )
                            .await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Product, crate::errors::Error>(product)
                })
            })
            .await
    }

    /// 在单个事务内写入卡券类目创建草稿并返回卡券类目扩展修订。
    ///
    /// 依次写入 `[新建分类]`、`products`、`product_revisions`（无媒体）、
    /// 唯一 SKU 的 `skus` + `sku_revisions`、`voucher_category_profile_revisions`，
    /// 以及审计日志。
    ///
    /// # 参数
    /// * `draft` - 创建草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回写入后的卡券类目扩展修订实体。
    ///
    /// # 错误
    /// 唯一索引冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_voucher_category_draft(
        &self,
        draft: VoucherCategoryDraft,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileRevision> {
        let audit = actor.clone().resource_log(
            "voucher_category.create",
            "voucher_category_profile",
            draft.voucher_revision.base.id.clone(),
        )?;
        let VoucherCategoryDraft {
            new_category,
            product,
            revision,
            sku_item,
            voucher_revision,
        } = draft;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    if let Some(category) = &new_category {
                        db.product_categories().create(category, session).await?;
                    }
                    db.products().create(&product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &[], session)
                        .await?;
                    db.catalog()
                        .create_sku_with_revision(
                            &sku_item.sku,
                            &sku_item.revision,
                            &sku_item.attribute_values,
                            session,
                        )
                        .await?;
                    db.voucher_category_profile_revisions()
                        .create(&voucher_revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<VoucherCategoryProfileRevision, crate::errors::Error>(voucher_revision)
                })
            })
            .await
    }

    /// 构造卡券类目更新草稿：沿用分类/品牌/单位与价格，仅改名称与描述。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU 稳定 ID
    /// * `req` - 更新请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回待写入的更新草稿。
    ///
    /// # 错误
    /// SKU/商品/当前修订不存在、非 VOUCHER 商品或乐观锁失败时返回错误。
    async fn build_voucher_category_update_draft(
        &self,
        sku_id: &str,
        req: UpdateVoucherCategoryRequest,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryUpdateDraft> {
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("卡券类目 SKU 不存在".to_string()))?;
        let mut product = self.load_product(sku.product_id.as_ref()).await?;
        if product.product_kind != ProductKind::Voucher {
            return Err(Error::BusinessLogicError(
                "目标 SKU 不属于卡券类目商品".to_string(),
            ));
        }
        ensure_version(product.base.version, req.version)?;

        let current_product_revision = self.load_current_product_revision(&product).await?;
        let current_sku_revision = self.load_current_sku_revision(sku_id).await?;

        let effective_from = req.effective_from.unwrap_or_else(BusinessDate::today);
        let effective_to = req.effective_to;
        let product_status = product.stable.status;
        let next_product_revision_no = self.next_product_revision_no(product.base.id.as_str()).await?;
        let next_sku_revision_no = self.next_sku_revision_no(sku_id).await?;
        let next_voucher_revision_no = self.next_voucher_profile_revision_no(sku_id).await?;

        let product_revision = ProductRevision::new(
            ProductRevisionId::new(next_id()),
            ProductRevisionData {
                product_id: ProductId::new(product.base.id.clone()),
                revision_no: next_product_revision_no,
                name: req.name.clone(),
                description: Some(req.description.clone()),
                specification: current_product_revision.specification.clone(),
                category_id: current_product_revision.category_id.clone(),
                brand_id: current_product_revision.brand_id.clone(),
                status: product_status,
                effective_from,
                effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(product_revision.base.id.clone());
        product.stable.touch(actor.id());

        let sku_revision = SkuRevision::new(
            SkuRevisionId::new(next_id()),
            SkuRevisionData {
                sku_id: SkuId::new(sku_id.to_string()),
                revision_no: next_sku_revision_no,
                name: req.name,
                description: Some(req.description.clone()),
                specification: current_sku_revision.specification.clone(),
                barcode: current_sku_revision.barcode.clone(),
                source_main_image_asset_id: current_sku_revision.source_main_image_asset_id.clone(),
                weight_kg: current_sku_revision.weight_kg,
                volume_m3: current_sku_revision.volume_m3,
                sales_visible_price_gross: current_sku_revision.sales_visible_price_gross,
                market_price: current_sku_revision.market_price,
                status: current_sku_revision.status,
                effective_from,
                effective_to,
            },
        )?;
        let mut sku = sku;
        sku.stable.current_revision_id = Some(sku_revision.base.id.clone());
        sku.stable.touch(actor.id());

        let voucher_revision = VoucherCategoryProfileRevision::new(
            VoucherCategoryProfileRevisionId::new(next_id()),
            VoucherCategoryProfileRevisionData {
                sku_id: SkuId::new(sku_id.to_string()),
                revision_no: next_voucher_revision_no,
                description: req.description,
                status: product_status,
            },
        )?;

        Ok(VoucherCategoryUpdateDraft {
            product,
            product_revision,
            sku,
            sku_revision,
            voucher_revision,
        })
    }

    /// 在事务内写入卡券类目更新草稿。
    ///
    /// # 参数
    /// * `draft` - 更新草稿
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回新建的卡券类目扩展修订。
    ///
    /// # 错误
    /// 乐观锁冲突或事务失败时返回错误并整体回滚。
    async fn write_voucher_category_update_draft(
        &self,
        draft: VoucherCategoryUpdateDraft,
        actor: &AuditActor,
    ) -> Result<VoucherCategoryProfileRevision> {
        let audit = actor.clone().resource_log(
            "voucher_category.update",
            "voucher_category_profile",
            draft.voucher_revision.base.id.clone(),
        )?;
        let VoucherCategoryUpdateDraft {
            mut product,
            product_revision,
            mut sku,
            sku_revision,
            voucher_revision,
        } = draft;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().update(&mut product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&product_revision, &[], session)
                        .await?;
                    db.skus().update(&mut sku, session).await?;
                    db.sku_revisions().create(&sku_revision, session).await?;
                    db.voucher_category_profile_revisions()
                        .create(&voucher_revision, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<VoucherCategoryProfileRevision, crate::errors::Error>(voucher_revision)
                })
            })
            .await
    }

    /// 为卡券类目视图补齐 SKU 编号、商品版本与展示名称。
    ///
    /// # 参数
    /// * `view` - 待补齐的响应视图（`sku_id` 必须已填）
    ///
    /// # 返回
    /// 成功时返回 `Ok(())`；关联数据缺失时静默跳过补齐字段。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn enrich_voucher_category_view(&self, view: &mut VoucherCategoryProfileView) -> Result<()> {
        let Some(sku) = self
            .db
            .skus()
            .find_by_id(&view.sku_id, &mut NoTransaction)
            .await?
        else {
            return Ok(());
        };
        view.sku_no = Some(sku.sku_no.clone());
        view.product_id = Some(sku.product_id.to_string());
        if let Ok(product) = self.load_product(sku.product_id.as_ref()).await {
            view.product_version = Some(product.base.version);
            if let Ok(revision) = self.load_current_product_revision(&product).await {
                view.name = Some(revision.name);
            }
        }
        if view.name.is_none() {
            if let Ok(sku_revision) = self.load_current_sku_revision(&view.sku_id).await {
                view.name = Some(sku_revision.name);
            }
        }
        if view.name.is_none() {
            view.name = Some(view.description.clone());
        }
        Ok(())
    }

    /// 加载商品当前修订（优先 `current_revision_id`，否则取最大修订号）。
    ///
    /// # 参数
    /// * `product` - 已加载的商品
    ///
    /// # 返回
    /// 返回当前商品修订。
    ///
    /// # 错误
    /// 不存在任何修订时返回 `NotFound`。
    async fn load_current_product_revision(&self, product: &Product) -> Result<ProductRevision> {
        if let Some(revision_id) = product.stable.current_revision_id.as_ref() {
            if let Some(revision) = self
                .db
                .product_revisions()
                .find_by_id(revision_id, &mut NoTransaction)
                .await?
            {
                return Ok(revision);
            }
        }
        let revisions = self
            .db
            .product_revisions()
            .find_many(doc! { "product_id": product.base.id.clone() }, &mut NoTransaction)
            .await?;
        revisions
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no)
            .ok_or_else(|| Error::NotFound("商品修订不存在".to_string()))
    }

    /// 加载 SKU 当前修订（优先 `current_revision_id`，否则取最大修订号）。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    ///
    /// # 返回
    /// 返回当前 SKU 修订。
    ///
    /// # 错误
    /// SKU 或修订不存在时返回 `NotFound`。
    async fn load_current_sku_revision(&self, sku_id: &str) -> Result<SkuRevision> {
        if let Some(sku) = self.db.skus().find_by_id(sku_id, &mut NoTransaction).await? {
            if let Some(revision_id) = sku.stable.current_revision_id.as_ref() {
                if let Some(revision) = self
                    .db
                    .sku_revisions()
                    .find_by_id(revision_id, &mut NoTransaction)
                    .await?
                {
                    return Ok(revision);
                }
            }
        }
        let revisions = self
            .db
            .sku_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        revisions
            .into_iter()
            .max_by_key(|revision| revision.revision.revision_no)
            .ok_or_else(|| Error::NotFound("SKU 修订不存在".to_string()))
    }

    /// 计算某卡券类目 SKU 已有扩展修订的最大序号 + 1。
    ///
    /// # 参数
    /// * `sku_id` - 卡券类目 SKU
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_voucher_profile_revision_no(&self, sku_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .voucher_category_profile_revisions()
            .find_many(doc! { "sku_id": sku_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }

    /// 构造规格编辑计划（分类保留/新增/重新启用/移除 + 新商品修订与媒体）。
    ///
    /// # 参数
    /// * `product` - 已加载并完成版本校验的 SPU（可变，计划构建中更新状态）
    /// * `req` - 规格编辑请求
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回编辑计划。
    ///
    /// # 错误
    /// 字典/媒体/规格/条码校验失败时返回对应错误。
    async fn build_spec_edit_plan(
        &self,
        product: &mut Product,
        req: UpdateProductRequest,
        actor: &AuditActor,
    ) -> Result<SpecEditPlan> {
        self.ensure_product_dictionaries(&req.category_id, &req.brand_id, &req.skus, product.product_kind)
            .await?;
        let product_id = ProductId::new(product.base.id.clone());
        let existing = self
            .db
            .skus()
            .find_many(doc! { "product_id": product_id.to_string() }, &mut NoTransaction)
            .await?;
        let current_by_signature: HashMap<String, Sku> = existing
            .into_iter()
            .map(|sku| (sku.specification_signature.clone(), sku))
            .collect();
        let next_product_revision_no = self.next_product_revision_no(&product_id).await?;
        let revision_id = ProductRevisionId::new(next_id());
        let media = self
            .build_media_rows(&revision_id, &req.carousel_media, MediaRole::Carousel)
            .await?
            .into_iter()
            .chain(
                self.build_media_rows(&revision_id, &req.detail_media, MediaRole::Detail)
                    .await?,
            )
            .collect::<Vec<_>>();

        let mut sku_items = Vec::with_capacity(req.skus.len());
        let mut seen_signatures = std::collections::HashSet::new();
        let mut disable = Vec::new();
        let change_reason = req.change_reason.as_deref().map(str::trim);
        let audit_message = change_reason
            .filter(|reason| !reason.is_empty())
            .map(str::to_string);
        for sku_input in req.skus {
            let (signature, resolved) = self
                .resolve_spec_entries(&req.category_id, &sku_input.spec_entries)
                .await?;
            if !seen_signatures.insert(signature.clone()) {
                return Err(Error::BusinessLogicError("规格集合中存在重复签名".to_string()));
            }
            if let Some(mut existing_sku) = current_by_signature.get(&signature).cloned() {
                // 保留/重新启用：沿用原 sku_id，追加修订；重新启用显式置 Active。
                let reactivating = !existing_sku.is_active();
                ensure_existing_sku_identity(&existing_sku, &sku_input, reactivating, change_reason)?;
                self.ensure_barcode_available(&sku_input.barcode, Some(existing_sku.base.id.as_str()))
                    .await?;
                let revision_no = self.next_sku_revision_no(&existing_sku.base.id).await?;
                let revision = self.build_sku_revision(
                    &existing_sku.base.id,
                    revision_no,
                    &req.name,
                    req.effective_from,
                    req.effective_to,
                    &sku_input,
                )?;
                let revision_id = SkuRevisionId::new(revision.base.id.clone());
                let attribute_values = build_attribute_value_rows(&revision_id, &resolved)?;
                if reactivating {
                    existing_sku.update(
                        SkuUpdate {
                            status: Some(EnableStatus::Active),
                        },
                        actor.id(),
                    )?;
                }
                sku_items.push(SkuEditItem {
                    action: if reactivating {
                        SkuEditAction::Reactivate
                    } else {
                        SkuEditAction::Keep
                    },
                    sku: existing_sku,
                    revision,
                    attribute_values,
                });
            } else {
                // 全新签名：分配新 SKU 身份。
                if sku_input.sku_id.is_some()
                    || sku_input.expected_sku_revision_id.is_some()
                    || sku_input.reenable
                {
                    return Err(Error::ValidationError(
                        "新增规格签名不得指定或猜测既有 SKU 身份".to_string(),
                    ));
                }
                let item = self
                    .build_new_sku_item(
                        NewSkuContext {
                            product_id: &product_id,
                            product_name: &req.name,
                            category_id: &req.category_id,
                            effective_from: req.effective_from,
                            effective_to: req.effective_to,
                            created_by: actor.id(),
                        },
                        sku_input,
                    )
                    .await?;
                sku_items.push(item);
            }
        }
        for (signature, sku) in &current_by_signature {
            if sku.is_active() && !seen_signatures.contains(signature) {
                let mut sku = sku.clone();
                sku.update(
                    SkuUpdate {
                        status: Some(EnableStatus::Disabled),
                    },
                    actor.id(),
                )?;
                disable.push(sku);
            }
        }

        let revision = ProductRevision::new(
            revision_id.clone(),
            ProductRevisionData {
                product_id: product_id.clone(),
                revision_no: next_product_revision_no,
                name: req.name,
                description: req.description,
                specification: req.specification,
                category_id: req.category_id,
                brand_id: req.brand_id,
                status: req.status,
                effective_from: req.effective_from,
                effective_to: req.effective_to,
            },
        )?;
        product.stable.current_revision_id = Some(revision.base.id.clone());
        product.stable.status = req.status;
        product.stable.touch(actor.id());
        Ok(SpecEditPlan {
            change_reason: audit_message,
            product: product.clone(),
            revision,
            media,
            sku_items,
            disable,
        })
    }

    /// 在单个事务内写入规格编辑计划。
    ///
    /// 写新商品修订与媒体、按动作写 SKU 修订/规格值/状态，更新 SPU，
    /// 移除签名的既有 SKU 转为停用，最后写审计日志；任一步失败整体回滚。
    ///
    /// # 参数
    /// * `plan` - 规格编辑计划
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回编辑后的 SPU 实体。
    ///
    /// # 错误
    /// 并发冲突（409）或事务失败时返回错误并整体回滚。
    async fn write_spec_edit_plan(&self, plan: SpecEditPlan, actor: &AuditActor) -> Result<Product> {
        let SpecEditPlan {
            change_reason,
            mut product,
            revision,
            media,
            sku_items,
            mut disable,
        } = plan;
        let audit = actor.clone().resource_log_with_message(
            "product.update",
            "product",
            product.base.id.clone(),
            change_reason,
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.products().update(&mut product, session).await?;
                    db.catalog()
                        .create_product_revision_with_media(&revision, &media, session)
                        .await?;
                    for item in &sku_items {
                        match item.action {
                            SkuEditAction::Create => {
                                db.catalog()
                                    .create_sku_with_revision(
                                        &item.sku,
                                        &item.revision,
                                        &item.attribute_values,
                                        session,
                                    )
                                    .await?;
                            }
                            SkuEditAction::Keep | SkuEditAction::Reactivate => {
                                db.sku_revisions().create(&item.revision, session).await?;
                                for row in &item.attribute_values {
                                    db.sku_revision_attribute_values().create(row, session).await?;
                                }
                                if item.action == SkuEditAction::Reactivate {
                                    let mut sku = item.sku.clone();
                                    db.skus().update(&mut sku, session).await?;
                                }
                            }
                        }
                    }
                    for sku in &mut disable {
                        db.skus().update(sku, session).await?;
                    }
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<Product, crate::errors::Error>(product)
                })
            })
            .await
    }

    /// 构造既有 SKU 的追加修订（Keep/Reactivate 动作）。
    ///
    /// # 参数
    /// * `sku_id` - 既有 SKU
    /// * `revision_no` - 下一个修订序号
    /// * `product_name` - 商品名称（修订名称快照）
    /// * `effective_from` / `effective_to` - 生效区间
    /// * `input` - SKU 输入行
    ///
    /// # 返回
    /// 返回 SKU 修订实体。
    ///
    /// # 错误
    /// 实体校验失败时返回错误。
    fn build_sku_revision(
        &self,
        sku_id: &str,
        revision_no: u32,
        product_name: &str,
        effective_from: BusinessDate,
        effective_to: Option<BusinessDate>,
        input: &ProductSkuInput,
    ) -> Result<SkuRevision> {
        Ok(SkuRevision::new(
            SkuRevisionId::new(next_id()),
            SkuRevisionData {
                sku_id: sku_id.to_string().into(),
                revision_no,
                name: product_name.to_string(),
                description: None,
                specification: None,
                barcode: input.barcode.clone(),
                source_main_image_asset_id: input.main_image_asset_id.clone(),
                weight_kg: input.weight_kg,
                volume_m3: input.volume_m3,
                sales_visible_price_gross: input.sales_visible_price_gross,
                market_price: input.market_price,
                status: EnableStatus::Active,
                effective_from,
                effective_to,
            },
        )?)
    }

    /// 计算某商品已有修订的最大序号 + 1。
    ///
    /// # 参数
    /// * `product_id` - 商品 ID
    ///
    /// # 返回
    /// 返回下一个修订序号。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn next_product_revision_no(&self, product_id: &str) -> Result<u32> {
        let revisions = self
            .db
            .product_revisions()
            .find_many(doc! { "product_id": product_id }, &mut NoTransaction)
            .await?;
        Ok(revisions
            .iter()
            .map(|revision| revision.revision.revision_no)
            .max()
            .unwrap_or(0)
            + 1)
    }
}

// ---------- 自由函数辅助 ----------

/// 校验卡券类目创建请求的分类选择：`category_id` 与 `new_category` 不可同时给出；
/// 两者都缺省时由服务端解析共用卡券根分类。
///
/// # 参数
/// * `category_id` - 引用已有分类
/// * `new_category` - 内联新建分类
///
/// # 返回
/// 合法时返回 `Ok(())`。
///
/// # 错误
/// 两者同时给出时返回 `ValidationError`。
fn ensure_category_selection_exclusive(
    category_id: &Option<ProductCategoryId>,
    new_category: &Option<NewVoucherCategoryInput>,
) -> Result<()> {
    match (category_id, new_category) {
        (Some(_), Some(_)) => Err(Error::ValidationError(
            "分类只能二选一：引用已有分类或新建分类".to_string(),
        )),
        (Some(_), None) | (None, Some(_)) | (None, None) => Ok(()),
    }
}

/// 校验期望版本与当前版本一致（乐观锁语义）。
///
/// # 参数
/// * `current` - 当前版本
/// * `expected` - 期望版本
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 不一致时返回 `ConflictError`（HTTP 409）。
fn ensure_version(current: u64, expected: u64) -> Result<()> {
    if current != expected {
        return Err(Error::ConflictError(
            "数据已被其他请求修改，请刷新后重试".to_string(),
        ));
    }
    Ok(())
}

/// 构造 SKU 修订规格属性值行（身份排序位 = 属性代码排序后的序号）。
///
/// # 参数
/// * `revision_id` - 所属 SKU 修订
/// * `resolved` - 已按属性代码排序的解析条目
///
/// # 返回
/// 返回规格属性值行集合。
///
/// # 错误
/// 实体校验失败时返回错误。
fn build_attribute_value_rows(
    revision_id: &SkuRevisionId,
    resolved: &[ResolvedSpecEntry],
) -> Result<Vec<SkuRevisionAttributeValue>> {
    let mut rows = Vec::with_capacity(resolved.len());
    for (position, entry) in resolved.iter().enumerate() {
        let row = SkuRevisionAttributeValue::new(
            SkuRevisionAttributeValueId::new(next_id()),
            SkuRevisionAttributeValueData {
                sku_revision_id: revision_id.clone(),
                sku_attribute_id: entry.attribute_id.clone(),
                sku_attribute_value_id: entry.attribute_value_id.clone(),
                normalized_text_value: entry.normalized_text_value.clone(),
                identity_position: position as u32,
            },
        )?;
        rows.push(row);
    }
    Ok(rows)
}

/// 校验既有规格行的稳定身份、期望修订与显式重新启用意图。
///
/// # 参数
/// * `existing` - 规范化签名命中的既有稳定 SKU
/// * `input` - 客户端提交的 SKU 行
/// * `reactivating` - 该 SKU 当前是否处于停用状态
/// * `change_reason` - 请求级变更原因
///
/// # 返回
/// 身份、修订与重新启用意图一致时返回 `Ok(())`。
///
/// # 错误
/// 身份缺失/不匹配、期望修订过期、稳定字段被改动，或停用 SKU 未明确重新
/// 启用并填写原因时返回验证或冲突错误。
fn ensure_existing_sku_identity(
    existing: &Sku,
    input: &ProductSkuInput,
    reactivating: bool,
    change_reason: Option<&str>,
) -> Result<()> {
    if input.sku_id.as_ref().map(ToString::to_string).as_deref() != Some(existing.base.id.as_str()) {
        return Err(Error::ValidationError(
            "既有规格行必须携带匹配的稳定 sku_id".to_string(),
        ));
    }
    let expected_revision = input.expected_sku_revision_id.as_ref().map(ToString::to_string);
    if expected_revision.as_deref() != existing.stable.current_revision_id.as_deref() {
        return Err(Error::ConflictError(
            "SKU 修订已变化，请刷新商品后重试".to_string(),
        ));
    }
    if input.sku_no.trim() != existing.sku_no || input.base_unit_id != existing.base_unit_id {
        return Err(Error::ValidationError(
            "SKU 编码和基础单位为稳定身份字段，编辑时不得修改".to_string(),
        ));
    }
    if reactivating && (!input.reenable || change_reason.is_none_or(str::is_empty)) {
        return Err(Error::ValidationError(
            "重新启用历史停用 SKU 必须明确 reenable=true 并填写 change_reason".to_string(),
        ));
    }
    if !reactivating && input.reenable {
        return Err(Error::ValidationError(
            "当前启用 SKU 不得提交重新启用意图".to_string(),
        ));
    }
    Ok(())
}

/// 校验一组整数内无重复（用于媒体展示顺序等组合唯一前置检查）。
///
/// # 参数
/// * `values` - 待检查值
/// * `label` - 字段说明
///
/// # 返回
/// 无重复时返回 `Ok(())`。
///
/// # 错误
/// 存在重复时返回 `BusinessLogicError`。
fn ensure_unique_sort_orders(values: impl Iterator<Item = i32>, label: &str) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    for value in values {
        if !seen.insert(value) {
            return Err(Error::BusinessLogicError(format!("{label}不能重复")));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        ensure_category_selection_exclusive, ensure_existing_sku_identity, EnableStatus,
        NewVoucherCategoryInput, ProductCategoryId, ProductId, ProductSkuInput, Sku, SkuData, SkuId,
        SkuRevisionId, UnitOfMeasureId,
    };

    fn new_category_input() -> NewVoucherCategoryInput {
        NewVoucherCategoryInput {
            category_code: "VC-CAT".to_string(),
            parent_category_id: None,
            name: "卡券分类".to_string(),
        }
    }

    fn existing_sku(status: EnableStatus) -> Sku {
        let mut sku = Sku::new(
            SkuId::new("sku-1"),
            SkuData {
                sku_no: "SKU-001".to_string(),
                product_id: ProductId::new("product-1"),
                base_unit_id: UnitOfMeasureId::new("unit-1"),
                specification_signature: "color=red".to_string(),
                status,
            },
            "tester",
        )
        .unwrap();
        sku.stable.current_revision_id = Some("sku-rev-2".to_string());
        sku
    }

    fn existing_input(reenable: bool) -> ProductSkuInput {
        ProductSkuInput {
            sku_id: Some(SkuId::new("sku-1")),
            expected_sku_revision_id: Some(SkuRevisionId::new("sku-rev-2")),
            reenable,
            sku_no: "SKU-001".to_string(),
            base_unit_id: UnitOfMeasureId::new("unit-1"),
            barcode: None,
            main_image_asset_id: None,
            weight_kg: None,
            volume_m3: None,
            sales_visible_price_gross: None,
            market_price: None,
            spec_entries: Vec::new(),
        }
    }

    #[test]
    fn ensure_category_selection_exclusive_accepts_exactly_one() {
        assert!(ensure_category_selection_exclusive(&Some(ProductCategoryId::new("cat-1")), &None).is_ok());
        assert!(ensure_category_selection_exclusive(&None, &Some(new_category_input())).is_ok());
    }

    #[test]
    fn ensure_category_selection_exclusive_rejects_both_given() {
        assert!(ensure_category_selection_exclusive(
            &Some(ProductCategoryId::new("cat-1")),
            &Some(new_category_input())
        )
        .is_err());
    }

    #[test]
    fn ensure_category_selection_exclusive_allows_neither_for_default_root() {
        assert!(
            ensure_category_selection_exclusive(&None, &None).is_ok(),
            "两者都缺省时走共用卡券根分类"
        );
    }

    #[test]
    fn existing_sku_requires_exact_current_revision() {
        let sku = existing_sku(EnableStatus::Active);
        let mut input = existing_input(false);
        input.expected_sku_revision_id = Some(SkuRevisionId::new("stale-revision"));

        assert!(ensure_existing_sku_identity(&sku, &input, false, None).is_err());
    }

    #[test]
    fn disabled_sku_requires_explicit_reenable_and_reason() {
        let sku = existing_sku(EnableStatus::Disabled);

        assert!(ensure_existing_sku_identity(&sku, &existing_input(false), true, None).is_err());
        assert!(ensure_existing_sku_identity(&sku, &existing_input(true), true, Some("恢复销售")).is_ok());
    }
}
