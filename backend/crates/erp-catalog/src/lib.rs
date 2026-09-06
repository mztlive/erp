//! Catalog domain: dictionaries, products, SKUs, sellable query and specification signature.

pub mod dto;
pub mod entity;
mod error;
pub mod indexes;
pub mod ports;
pub mod repository;
pub mod service;

pub use dto::{
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
pub use entity::catalog::{
    compute_specification_signature, parse_specification_signature, read_specification_signature,
    EnableStatus, ListingStatus, Product, ProductBrand, ProductCategory, ProductKind, ProductListingStatus,
    ProductRevision, Sku, SkuAttribute, SkuAttributeValue, SkuCoverageStatus, SkuRevision,
    SpecSignatureEntry, SpecificationSignatureRead, SpecificationSignatureSet, UnitOfMeasure,
    EMPTY_SPEC_SIGNATURE,
};
pub use error::{Error, Result};
pub use ports::{
    CatalogAuditPort, EmptyFileAssetFacts, EmptyPendingAttachments, FailClosedAuditPort, FileAssetFact,
    FileAssetFactsPort, PendingAttachmentBatch, PreparedCatalogAudit,
};
pub use repository::{
    CatalogExt, CatalogRepository, ProductBrandRepository, ProductCategoryRepository, ProductRepository,
    SellableSkuFilter, SkuRepository, UnitOfMeasureRepository,
};
pub use service::{
    sellable_sku_invalid_error, CatalogService, SellableSkuListParams, SellableSkuSpecificationAttributeView,
    SellableSkuView,
};
