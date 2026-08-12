//! 域 D10 `catalog` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳；金额与数量为十进制字符串；
//! 生效日期为 `YYYY-MM-DD`（`BusinessDate` 的既有序列化形态）。
//!
//! 排序白名单校验辅助（`normalize_sort`/`PageParams`/`PageView`）与 D01
//! source_registry 同构；抽取到冻结的 `services/src/query.rs` 属地基修订
//! 候选（见域报告）。

mod attribute;
mod brand_unit;
mod category;
mod common;
mod product;
mod voucher;

pub use attribute::{
    CreateSkuAttributeRequest, CreateSkuAttributeValueRequest, SkuAttributeListParams,
    SkuAttributeValueListParams, SkuAttributeValueView, SkuAttributeView, UpdateSkuAttributeRequest,
    UpdateSkuAttributeValueRequest,
};
#[allow(unused_imports)]
pub(crate) use attribute::{
    SkuAttributeListQuery, SkuAttributeValueListQuery, SKU_ATTRIBUTE_SORT_FIELDS,
    SKU_ATTRIBUTE_VALUE_SORT_FIELDS,
};
pub use brand_unit::{
    CreateProductBrandRequest, CreateUnitOfMeasureRequest, ProductBrandListParams, ProductBrandView,
    UnitOfMeasureListParams, UnitOfMeasureView, UpdateProductBrandRequest, UpdateUnitOfMeasureRequest,
};
#[allow(unused_imports)]
pub(crate) use brand_unit::{
    ProductBrandListQuery, UnitOfMeasureListQuery, PRODUCT_BRAND_SORT_FIELDS, UNIT_OF_MEASURE_SORT_FIELDS,
};
pub use category::{
    CreateProductCategoryRequest, MoveProductCategoryRequest, ProductCategoryListParams, ProductCategoryView,
    UpdateProductCategoryRequest,
};
#[allow(unused_imports)]
pub(crate) use category::{ProductCategoryListQuery, PRODUCT_CATEGORY_SORT_FIELDS};
#[allow(unused_imports)]
pub(crate) use common::normalize_sort;
#[allow(unused_imports)]
pub use common::{PageParams, PageView, SortDir};
pub use product::{
    CreateProductRequest, ProductListParams, ProductListingView, ProductMediaInput,
    ProductRevisionListParams, ProductRevisionMediaView, ProductRevisionView, ProductSkuInput, ProductView,
    SkuListParams, SkuRevisionListParams, SkuRevisionView, SkuView, SpecEntryInput,
    UpdateProductListingRequest, UpdateProductRequest, UpdateSkuListingRequest,
};
#[allow(unused_imports)]
pub(crate) use product::{
    ProductListQuery, ProductRevisionListQuery, SkuListQuery, SkuRevisionListQuery,
    PRODUCT_REVISION_SORT_FIELDS, PRODUCT_SORT_FIELDS, SKU_REVISION_SORT_FIELDS, SKU_SORT_FIELDS,
};
pub use voucher::{
    CreateVoucherCategoryRequest, NewVoucherCategoryInput, UpdateVoucherCategoryRequest,
    VoucherCategoryProfileListParams, VoucherCategoryProfileView, VoucherSkuInput,
};
#[allow(unused_imports)]
pub(crate) use voucher::{VoucherCategoryProfileListQuery, VOUCHER_PROFILE_SORT_FIELDS};

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir};
    use entities::catalog::{ListingStatus, ProductKind, ProductListingStatus, SkuCoverageStatus};
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(&Some("name".to_string()), &None, &["created_at"]).is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" created_at ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at"],
        )
        .unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Asc);

        let (field, direction) = normalize_sort(&None, &None, &["created_at"]).unwrap();
        assert_eq!(field, "created_at");
        assert_eq!(direction, SortDir::Desc);
    }

    #[test]
    fn product_category_params_normalize_root_parent_and_paging() {
        let params: super::ProductCategoryListParams = serde_json::from_value(serde_json::json!({
            "category_code": " CAT-001 ",
            "parent_category_id": "root",
            "page": 2,
            "page_size": 50,
            "sort_by": "name",
            "sort_dir": "asc",
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.category_code.as_deref(), Some("CAT-001"));
        assert_eq!(query.parent_category_id, Some(None));
        assert_eq!(query.paging.page, 2);
        assert_eq!(query.paging.page_size, 50);
        assert_eq!(query.paging.sort_by, "name");
        assert_eq!(query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn product_params_normalize_filters_and_defaults() {
        let params: super::ProductListParams = serde_json::from_value(serde_json::json!({
            "product_no": " P-1 ",
            "keyword": " 礼盒 ",
            "product_kind": "PHYSICAL",
            "category_id": " category-1 ",
            "brand_id": " brand-1 ",
            "supplier_id": " supplier-1 ",
            "status": "active",
            "listing_status": "partially_listed",
            "supply_coverage": "complete",
            "sales_price_min": "100.00",
            "sales_price_max": "200.00",
        }))
        .unwrap();
        let query = params.normalized().unwrap();
        assert_eq!(query.product_no.as_deref(), Some("P-1"));
        assert_eq!(query.keyword.as_deref(), Some("礼盒"));
        assert_eq!(query.product_kind, Some(ProductKind::Physical));
        assert_eq!(query.category_id.as_deref(), Some("category-1"));
        assert_eq!(query.brand_id.as_deref(), Some("brand-1"));
        assert_eq!(query.supplier_id.as_deref(), Some("supplier-1"));
        assert_eq!(query.listing_status, Some(ProductListingStatus::PartiallyListed));
        assert_eq!(query.supply_coverage, Some(SkuCoverageStatus::Complete));
        assert_eq!(query.sales_price_min.unwrap().to_string(), "100.00");
        assert_eq!(query.sales_price_max.unwrap().to_string(), "200.00");
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn product_params_reject_inverted_sales_price_range() {
        let params: super::ProductListParams = serde_json::from_value(serde_json::json!({
            "sales_price_min": "200.00",
            "sales_price_max": "100.00",
        }))
        .unwrap();

        assert!(params.normalized().is_err());
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params: super::SkuListParams = serde_json::from_value(serde_json::json!({
            "page": 0,
            "page_size": 1000,
        }))
        .unwrap();
        assert!(params.validate().is_err());
    }

    #[test]
    fn sku_params_accept_listing_status_filter() {
        let params: super::SkuListParams = serde_json::from_value(serde_json::json!({
            "listing_status": "listed",
        }))
        .unwrap();
        let query = params.normalized().unwrap();

        assert_eq!(query.listing_status, Some(ListingStatus::Listed));
    }

    #[test]
    fn create_product_request_rejects_empty_skus() {
        let request: super::CreateProductRequest = serde_json::from_value(serde_json::json!({
            "product_no": "P-1",
            "product_kind": "PHYSICAL",
            "name": "商品",
            "category_id": "cat-1",
            "brand_id": "brand-1",
            "effective_from": "2026-01-01",
            "skus": [],
        }))
        .unwrap();
        assert!(request.validate().is_err(), "空 SKU 列表必须被拒绝");
    }

    fn voucher_category_request_json() -> serde_json::Value {
        serde_json::json!({
            "voucher_no": "VC-1",
            "name": "满100减20券",
            "description": "满100元可用",
            "category_id": "cat-1",
            "brand_id": "brand-1",
            "sku": { "base_unit_id": "unit-1" },
            "effective_from": "2026-01-01",
        })
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_voucher_no() {
        let mut value = voucher_category_request_json();
        value["voucher_no"] = serde_json::json!("   ");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白卡券类目编号必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_name_and_description() {
        let mut value = voucher_category_request_json();
        value["name"] = serde_json::json!("");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空名称必须被拒绝");

        let mut value = voucher_category_request_json();
        value["description"] = serde_json::json!("  ");
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白描述必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_rejects_blank_new_category_fields() {
        let mut value = voucher_category_request_json();
        value.as_object_mut().unwrap().remove("category_id");
        value["new_category"] = serde_json::json!({ "category_code": "", "name": "卡券分类" });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白分类代码必须被拒绝");

        let mut value = voucher_category_request_json();
        value.as_object_mut().unwrap().remove("category_id");
        value["new_category"] = serde_json::json!({ "category_code": "VC-CAT", "name": "  " });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(request.validate().is_err(), "空白分类名称必须被拒绝");
    }

    #[test]
    fn create_voucher_category_request_allows_minimal_identity_only() {
        let value = serde_json::json!({
            "voucher_no": "VC-MIN",
            "name": "心意卡",
            "description": "员工福利卡",
        });
        let request: super::CreateVoucherCategoryRequest = serde_json::from_value(value).unwrap();
        assert!(
            request.validate().is_ok(),
            "仅身份字段应通过校验，字典由服务端默认"
        );
        assert!(request.category_id.is_none());
        assert!(request.brand_id.is_none());
        assert!(request.sku.is_none());
        assert!(request.effective_from.is_none());
    }
}
