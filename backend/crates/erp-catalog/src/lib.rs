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
    DisableProductRequest, HandoverCandidateView, HandoverProductRequest, HandoverProductView,
    MAX_PRODUCT_IMPORT_FILE_BYTES, MoveProductCategoryRequest, NewVoucherCategoryInput,
    PRODUCT_IMPORT_DIRECT_PART_BYTES, PRODUCT_IMPORT_DIRECT_PART_URL_TTL_SECS, PRODUCT_IMPORT_HEADERS,
    PRODUCT_IMPORT_NAME_COLUMN, PRODUCT_IMPORT_SHEET_NAME, PRODUCT_IMPORT_UNIT_CODE,
    PRODUCT_IMPORT_UNIT_NAME, PRODUCT_IMPORT_XLSX_MIME, PageView, ProductBrandListParams, ProductBrandView,
    ProductCategoryListParams, ProductCategoryParentChange, ProductCategoryView,
    ProductImportDirectUploadCompleteRequest, ProductImportDirectUploadInitRequest,
    ProductImportDirectUploadInitView, ProductImportDirectUploadPartView, ProductImportDirectUploadedPart,
    ProductImportItemListParams, ProductImportItemView, ProductImportJobListParams, ProductImportJobView,
    ProductListParams, ProductListView, ProductListingView, ProductMediaInput, ProductRevisionListParams,
    ProductRevisionMediaView, ProductRevisionView, ProductSkuInput, ProductView, SkuAttributeListParams,
    SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView, SkuListParams,
    SkuRevisionListParams, SkuRevisionView, SkuView, SpecEntryInput, UnitOfMeasureListParams,
    UnitOfMeasureView, UpdateProductBrandRequest, UpdateProductCategoryRequest, UpdateProductListingRequest,
    UpdateProductRequest, UpdateSkuAttributeRequest, UpdateSkuAttributeValueRequest, UpdateSkuListingRequest,
    UpdateUnitOfMeasureRequest, UpdateVoucherCategoryRequest, VoucherCategoryProfileListParams,
    VoucherCategoryProfileView, VoucherSkuInput, ensure_product_import_headers,
};
pub use entity::catalog::{
    EMPTY_SPEC_SIGNATURE, EnableStatus, ListingStatus, Product, ProductBrand, ProductCategory, ProductKind,
    ProductListingStatus, ProductRevision, Sku, SkuAttribute, SkuAttributeValue, SkuCoverageStatus,
    SkuRevision, SpecSignatureEntry, SpecificationSignatureRead, SpecificationSignatureSet, UnitOfMeasure,
    compute_specification_signature, parse_specification_signature, read_specification_signature,
};
pub use error::{Error, Result};
pub use ports::{
    CatalogAuditPort, CatalogDataScopePort, CatalogResolvedClause, CatalogResolvedScope, CatalogScopeObject,
    EmptyFileAssetFacts, EmptyPendingAttachments, FailClosedAuditPort, FailClosedCatalogDataScopePort,
    FileAssetFact, FileAssetFactsPort, PendingAttachmentBatch, PreparedCatalogAudit,
};
pub use repository::{
    CatalogExt, CatalogReadScope, CatalogRepository, CatalogScopeClause, ProductBrandRepository,
    ProductCategoryRepository, ProductFilter, ProductRepository, ProductRow, SellableSkuFilter,
    SkuRepository, UnitOfMeasureRepository,
};
pub use service::{
    CatalogAccess, CatalogService, SellableSkuListParams, SellableSkuSpecificationAttributeView,
    SellableSkuView, catalog_scope, sellable_sku_invalid_error,
};
