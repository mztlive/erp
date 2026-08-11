# `backend/services/src/catalog/dto.rs` 拆分分析

## 文件信息

| 项目 | 内容 |
|---|---|
| 源文件 | `backend/services/src/catalog/dto.rs` |
| 扫描行数 | 1961 |
| 分析状态 | 已完成深入分析 |
| 拆分结论 | split |
| 预估工作量 | M |
| 风险 | medium |
| 生成来源 | workflow `analyze-large-files` |
| 生成日期 | 2026-08-11 |

## 拆分方案

- 结论：**split**（工作量 M，风险 medium）
- 摘要：建议拆分。目标文件包含通用分页基础设施、分类、品牌/单位、规格属性、商品聚合和卡券类目六个明确内聚簇。保留 catalog/mod.rs 作为领域模块根及公共 re-export 入口，同时保留 catalog/dto.rs 作为内部 facade，声明并重新导出六个 dto 子文件；现有测试继续留在 dto.rs。这样无需改变 services::catalog::* 公共 API，也无需批量修改 crate::catalog::dto::* 内部引用。拆分后 dto.rs 约 230 行，最大 product.rs 约 750 行，其余约 130-280 行，均可控制在约 800 行以内。主要风险是 non_blank 跨子模块可见性、validator 派生宏名称解析以及 pub/pub(crate) re-export 边界。
- 拆分建议：
  - **backend/services/src/catalog/dto/common.rs**：放置通用分页、排序和验证基础设施：SortDir、PageParams、PageView<T>、normalize_sort、non_blank。
    - 依赖/注意：依赖 crate::errors::{Error, Result} 和 serde::Serialize。PageView、SortDir、PageParams 应由父级 dto.rs 按原可见性重新导出；normalize_sort 保持 pub(crate)。non_blank 从私有函数调整为 pub(super)，并由需要自定义 validator 校验的子模块显式导入。该文件不得反向依赖具体 DTO 子模块，以避免循环依赖。
  - **backend/services/src/catalog/dto/category.rs**：放置商品分类 DTO 簇：PRODUCT_CATEGORY_SORT_FIELDS、CreateProductCategoryRequest、UpdateProductCategoryRequest、MoveProductCategoryRequest、ProductCategoryView、impl From<ProductCategory> for ProductCategoryView、ProductCategoryListParams、ProductCategoryListQuery、impl ProductCategoryListParams::normalized。
    - 依赖/注意：依赖 entities::catalog::{EnableStatus, ProductCategory, ProductKind}、ProductCategoryId、serde、validator、crate::errors::Result、crate::query::{normalized_text, page_or_default, page_size_or_default}，并从 super::common 引入 non_blank、normalize_sort、PageParams。ProductCategoryListQuery 与 normalized 继续保持 pub(crate)；parent_category_id 的 root 特判必须原样迁移。
  - **backend/services/src/catalog/dto/brand_unit.rs**：放置品牌与计量单位两个稳定字典簇：PRODUCT_BRAND_SORT_FIELDS、CreateProductBrandRequest、UpdateProductBrandRequest、ProductBrandView、ProductBrand 的 From impl、ProductBrandListParams、ProductBrandListQuery 及 normalized；UNIT_OF_MEASURE_SORT_FIELDS、CreateUnitOfMeasureRequest、UpdateUnitOfMeasureRequest、UnitOfMeasureView、UnitOfMeasure 的 From impl、UnitOfMeasureListParams、UnitOfMeasureListQuery 及 normalized。
    - 依赖/注意：依赖 ProductBrand、UnitOfMeasure、EnableStatus、FileAssetId、serde、validator、crate::errors::Result 和 crate::query helper，并从 super::common 引入 non_blank、normalize_sort、PageParams。两个字典没有相互类型依赖，但体量较小且生命周期一致，合并后仍约 270 行。内部 ListQuery 保持 pub(crate)，公开 DTO 经 dto.rs 和 catalog/mod.rs 两级 re-export。
  - **backend/services/src/catalog/dto/attribute.rs**：放置规格属性及属性值 DTO 簇：SKU_ATTRIBUTE_SORT_FIELDS、SKU_ATTRIBUTE_VALUE_SORT_FIELDS、CreateSkuAttributeRequest、UpdateSkuAttributeRequest、SkuAttributeView、SkuAttribute 的 From impl、SkuAttributeListParams、SkuAttributeListQuery 及 normalized；CreateSkuAttributeValueRequest、UpdateSkuAttributeValueRequest、SkuAttributeValueView、SkuAttributeValue 的 From impl、SkuAttributeValueListParams、SkuAttributeValueListQuery 及 normalized。
    - 依赖/注意：依赖 SkuAttribute、SkuAttributeValue、EnableStatus、AttributeValueType、SkuAttributeId、serde、validator、crate::errors::Result、crate::query helper，并从 super::common 引入 non_blank、normalize_sort、PageParams。属性值直接引用 SkuAttributeId，因此属性与属性值应保留在同一文件。ListQuery 类型和 normalized 方法保持 pub(crate)。
  - **backend/services/src/catalog/dto/product.rs**：放置商品聚合 DTO：PRODUCT_SORT_FIELDS、PRODUCT_REVISION_SORT_FIELDS、SKU_SORT_FIELDS、SKU_REVISION_SORT_FIELDS；ProductMediaInput、SpecEntryInput、ProductSkuInput、CreateProductRequest、UpdateProductRequest、ProductView、UpdateProductListingRequest、UpdateSkuListingRequest、ProductListingView、ProductListParams、ProductListQuery、ProductListParams::normalized、validate_sales_price_range；ProductRevisionMediaView、ProductRevisionView、ProductRevision 的 From impl、ProductRevisionListParams、ProductRevisionListQuery 及 normalized；SkuView、Sku 的 From impl、SkuListParams、SkuListQuery 及 normalized；SkuRevisionView、SkuRevision 的 From impl、SkuRevisionListParams、SkuRevisionListQuery 及 normalized。
    - 依赖/注意：预计约 720-750 行，仍低于 800 行。依赖 ProductRevision、Sku、SkuRevision、MediaRole、ProductKind、ListingStatus、ProductListingStatus、SkuCoverageStatus、相关 ID、BusinessDate、Amount、Quantity、serde、validator、crate::errors::{Error, Result} 和 crate::query helper；从 super::common 引入 non_blank、normalize_sort、PageParams。validate_sales_price_range 留作本文件私有 helper。ProductSkuInput 被 product_workflow、sku_edit 和 voucher Service 使用，必须由 dto.rs 继续公开重新导出。该文件只依赖 common，不依赖 voucher.rs。
  - **backend/services/src/catalog/dto/voucher.rs**：放置卡券类目 DTO 簇：VOUCHER_PROFILE_SORT_FIELDS、NewVoucherCategoryInput、VoucherSkuInput、CreateVoucherCategoryRequest、UpdateVoucherCategoryRequest、VoucherCategoryProfileView、impl From<VoucherCategoryProfileRevision> for VoucherCategoryProfileView、VoucherCategoryProfileListParams、VoucherCategoryProfileListQuery、impl VoucherCategoryProfileListParams::normalized。
    - 依赖/注意：依赖 EnableStatus、VoucherCategoryProfileRevision、ProductCategoryId、ProductBrandId、SkuId、UnitOfMeasureId、BusinessDate、Amount、Quantity、serde、validator、crate::errors::Result 和 crate::query helper，并从 super::common 引入 non_blank、normalize_sort、PageParams。不要从 product.rs 直接导入 ProductSkuInput；voucher Service 可继续通过 dto.rs facade 分别导入 ProductSkuInput 和卡券 DTO，避免 DTO 子模块循环依赖。内部列表查询类型和 normalized 保持 pub(crate)。

## 实施约束

- 拆分应保持现有公开 API 和 re-export 路径，除非实施前另行确认契约变更。
- Service 继续负责事务边界；Repository 只负责数据访问；纯业务规则优先下沉到 Entity 或 Value Object。
- 私有 helper 应跟随唯一调用方迁移；仅在同一领域多个子模块复用时使用 `pub(super)`。
- 拆分完成后执行 `cargo fmt --all -- --check`、`cargo check --workspace`、`cargo clippy --workspace --all-targets --all-features` 和 `cargo test --workspace`。
