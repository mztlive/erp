//! 域 D10 `catalog` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 字典（分类/品牌/单位/规格属性/属性值）单集合 CRUD → `&mut NoTransaction`
//!   （审计日志按 D01 既有写法独立写入）；
//! - 商品创建与规格编辑：跨集合（products + product_revisions + 媒体 +
//!   skus + sku_revisions + 审计）→
//!   `database::Transactional::with_transaction`，保证「SPU 身份 + 修订快照 +
//!   SKU 身份 + 规格签名」原子可见（数据模型 §6.3）；
//! - 卡券类目原子创建：跨集合（[新建分类] + products + product_revisions +
//!   唯一 SKU + sku_revisions + voucher_category_profile_revisions + 审计）→
//!   `database::Transactional::with_transaction`，与商品创建同构（数据模型
//!   §6.3：卡券类目即 VOUCHER 类型的单 SKU 商品，不再要求预先存在 SKU）。
//!
//! 业务规则来自 entities（`new()`/`update()` 已完成校验与规范化，
//! `specification::compute_specification_signature` 计算签名），Service 只编排：
//! 字典存在性与启用校验、条码冲突、成环检测、规格签名分类
//! （保留/新增/重新启用/移除）与事务写入。跨域只调对方 Repository（D05
//! `file_assets` 校验媒体引用；D02 `audit_logs` 写审计），禁止 Service 依赖 Service。
use mongodb::Database;

mod attribute;
mod brand;
mod category;
mod dto;
mod listing;
mod product_query;
mod product_workflow;
mod sellable;
mod sku_edit;
mod support;
mod unit;
mod voucher;
mod voucher_defaults;

pub use self::dto::{
    CreateProductBrandRequest, CreateProductCategoryRequest, CreateProductRequest, CreateSkuAttributeRequest,
    CreateSkuAttributeValueRequest, CreateUnitOfMeasureRequest, CreateVoucherCategoryRequest,
    DisableProductRequest, MoveProductCategoryRequest, NewVoucherCategoryInput, PageView,
    ProductBrandListParams, ProductBrandView, ProductCategoryListParams, ProductCategoryParentChange,
    ProductCategoryView, ProductListParams, ProductListingView, ProductMediaInput, ProductRevisionListParams,
    ProductRevisionMediaView, ProductRevisionView, ProductSkuInput, ProductView, SkuAttributeListParams,
    SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView, SkuListParams,
    SkuRevisionListParams, SkuRevisionView, SkuView, SpecEntryInput, UnitOfMeasureListParams,
    UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest, UpdateProductListingRequest,
    UpdateProductRequest, UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateSkuListingRequest,
    UpdateUnitOfMeasureRequest, UpdateVoucherCategoryRequest, VoucherCategoryProfileListParams,
    VoucherCategoryProfileView, VoucherSkuInput,
};
pub(crate) use self::sellable::sellable_sku_invalid_error;
pub use self::sellable::{SellableSkuListParams, SellableSkuSpecificationAttributeView, SellableSkuView};

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
}
