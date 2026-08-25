//! 卡券类目默认字典与分类选择值规则。
//!
//! 默认分类、品牌和计量单位是 Catalog 域稳定业务约定；Repository 负责按稳定代码
//! 查询持久化事实，Service 只编排“查询或创建”。

use crate::catalog::product_brand::{ProductBrand, ProductBrandData};
use crate::catalog::product_category::{ProductCategory, ProductCategoryData};
use crate::catalog::product_kind::ProductKind;
use crate::catalog::status::EnableStatus;
use crate::catalog::unit_of_measure::{UnitOfMeasure, UnitOfMeasureData};
use crate::errors::{Error, Result};
use crate::ids::{ProductBrandId, ProductCategoryId, UnitOfMeasureId};

/// 卡券根分类稳定代码。
pub const VOUCHER_ROOT_CATEGORY_CODE: &str = "VOUCHER";
/// 卡券根分类名称。
pub const VOUCHER_ROOT_CATEGORY_NAME: &str = "卡券";
/// 卡券默认品牌稳定代码。
pub const VOUCHER_DEFAULT_BRAND_CODE: &str = "FSY";
/// 卡券默认品牌名称。
pub const VOUCHER_DEFAULT_BRAND_NAME: &str = "福尚云";
/// 卡券默认基础单位代码、名称与符号。
pub const VOUCHER_DEFAULT_UNIT_CODE: &str = "张";

/// 卡券类目创建时的分类来源。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum VoucherCategorySelection<T> {
    /// 引用已有分类。
    Existing(ProductCategoryId),
    /// 内联新建分类。
    New(T),
    /// 两者均未指定，使用共用卡券根分类。
    DefaultRoot,
}

impl<T> VoucherCategorySelection<T> {
    /// 从可选已有分类与可选新分类构造互斥选择。
    ///
    /// # 参数
    /// * `category_id` - 已有分类稳定 ID
    /// * `new_category` - 内联新建分类数据
    ///
    /// # 返回
    /// 返回已有分类、新分类或默认根分类三种确定性选择之一。
    ///
    /// # 错误
    /// 两种显式来源同时给出时返回领域错误，避免创建语义含糊。
    pub fn from_options(category_id: Option<ProductCategoryId>, new_category: Option<T>) -> Result<Self> {
        match (category_id, new_category) {
            (Some(category_id), None) => Ok(Self::Existing(category_id)),
            (None, Some(new_category)) => Ok(Self::New(new_category)),
            (None, None) => Ok(Self::DefaultRoot),
            (Some(_), Some(_)) => Err(Error::from("分类只能二选一：引用已有分类或新建分类")),
        }
    }
}

/// 卡券默认字典实体工厂与兼容性规则。
pub struct VoucherCatalogDefaults;

impl VoucherCatalogDefaults {
    /// 构造共用卡券根分类实体。
    ///
    /// # 参数
    /// * `id` - 新分类稳定 ID
    /// * `created_by` - 创建人身份
    ///
    /// # 返回
    /// 返回代码 `VOUCHER`、名称“卡券”且商品类型为卡券的启用根分类。
    ///
    /// # 错误
    /// 默认值违反分类实体不变式时返回领域错误。
    pub fn root_category(id: ProductCategoryId, created_by: impl Into<String>) -> Result<ProductCategory> {
        ProductCategory::new(
            id,
            ProductCategoryData {
                category_code: VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                parent_category_id: None,
                name: VOUCHER_ROOT_CATEGORY_NAME.to_string(),
                product_kind: ProductKind::Voucher,
                status: EnableStatus::Active,
            },
            created_by,
        )
    }

    /// 校验已存在的共用根分类仍保持卡券类型。
    ///
    /// # 参数
    /// * `category` - 按默认稳定代码查询到的分类
    ///
    /// # 返回
    /// 商品类型为卡券时返回 `Ok(())`。
    ///
    /// # 错误
    /// 稳定代码被错误地用于其他商品类型时返回领域错误。
    pub fn ensure_root_category_compatible(category: &ProductCategory) -> Result<()> {
        if category.product_kind != ProductKind::Voucher {
            return Err(Error::from("系统卡券根分类的商品类型不是卡券"));
        }
        Ok(())
    }

    /// 构造卡券默认品牌实体。
    ///
    /// # 参数
    /// * `id` - 新品牌稳定 ID
    /// * `created_by` - 创建人身份
    ///
    /// # 返回
    /// 返回代码 `FSY`、名称“福尚云”的启用品牌。
    ///
    /// # 错误
    /// 默认值违反品牌实体不变式时返回领域错误。
    pub fn brand(id: ProductBrandId, created_by: impl Into<String>) -> Result<ProductBrand> {
        ProductBrand::new(
            id,
            ProductBrandData {
                brand_code: VOUCHER_DEFAULT_BRAND_CODE.to_string(),
                name: VOUCHER_DEFAULT_BRAND_NAME.to_string(),
                status: EnableStatus::Active,
                logo_file_asset_id: None,
            },
            created_by,
        )
    }

    /// 构造卡券默认基础单位实体。
    ///
    /// # 参数
    /// * `id` - 新计量单位稳定 ID
    /// * `created_by` - 创建人身份
    ///
    /// # 返回
    /// 返回代码、名称和符号均为“张”，且数量精度为整数的启用单位。
    ///
    /// # 错误
    /// 默认值违反计量单位实体不变式时返回领域错误。
    pub fn unit(id: UnitOfMeasureId, created_by: impl Into<String>) -> Result<UnitOfMeasure> {
        UnitOfMeasure::new(
            id,
            UnitOfMeasureData {
                unit_code: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                name: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                symbol: VOUCHER_DEFAULT_UNIT_CODE.to_string(),
                quantity_scale: 0,
                status: EnableStatus::Active,
            },
            created_by,
        )
    }

    /// 校验已存在的默认基础单位可继续用于创建卡券类目。
    ///
    /// # 参数
    /// * `unit` - 按默认稳定代码查询到的计量单位
    ///
    /// # 返回
    /// 单位处于启用状态时返回 `Ok(())`。
    ///
    /// # 错误
    /// 默认单位已停用时返回领域错误。
    pub fn ensure_unit_active(unit: &UnitOfMeasure) -> Result<()> {
        if !unit.is_active() {
            return Err(Error::from("默认单位“张”已停用"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::catalog::product_category::ProductCategoryData;
    use crate::catalog::unit_of_measure::UnitOfMeasureUpdate;

    /// 分类来源接受已有、新建和缺省三种互斥形态。
    #[test]
    fn category_selection_accepts_each_unambiguous_source() {
        assert!(matches!(
            VoucherCategorySelection::<String>::from_options(Some(ProductCategoryId::new("cat-1")), None)
                .unwrap(),
            VoucherCategorySelection::Existing(_)
        ));
        assert!(matches!(
            VoucherCategorySelection::from_options(None, Some("new".to_string())).unwrap(),
            VoucherCategorySelection::New(_)
        ));
        assert!(matches!(
            VoucherCategorySelection::<String>::from_options(None, None).unwrap(),
            VoucherCategorySelection::DefaultRoot
        ));
    }

    /// 分类来源同时给出已有与新建时被拒绝。
    #[test]
    fn category_selection_rejects_ambiguous_sources() {
        assert!(VoucherCategorySelection::from_options(
            Some(ProductCategoryId::new("cat-1")),
            Some("new".to_string())
        )
        .is_err());
    }

    /// 默认字典工厂生成稳定代码、启用状态和固定数量精度。
    #[test]
    fn default_factories_produce_canonical_entities() {
        let category =
            VoucherCatalogDefaults::root_category(ProductCategoryId::new("cat-1"), "tester").unwrap();
        let brand = VoucherCatalogDefaults::brand(ProductBrandId::new("brand-1"), "tester").unwrap();
        let unit = VoucherCatalogDefaults::unit(UnitOfMeasureId::new("unit-1"), "tester").unwrap();

        assert_eq!(category.category_code, VOUCHER_ROOT_CATEGORY_CODE);
        assert_eq!(category.product_kind, ProductKind::Voucher);
        assert_eq!(brand.brand_code, VOUCHER_DEFAULT_BRAND_CODE);
        assert_eq!(brand.name, VOUCHER_DEFAULT_BRAND_NAME);
        assert_eq!(unit.unit_code, VOUCHER_DEFAULT_UNIT_CODE);
        assert_eq!(unit.quantity_scale, 0);
        assert!(unit.is_active());
    }

    /// 默认根分类类型漂移和默认单位停用均被领域规则拒绝。
    #[test]
    fn default_compatibility_rules_reject_drift() {
        let wrong_category = ProductCategory::new(
            ProductCategoryId::new("cat-1"),
            ProductCategoryData {
                category_code: VOUCHER_ROOT_CATEGORY_CODE.to_string(),
                parent_category_id: None,
                name: VOUCHER_ROOT_CATEGORY_NAME.to_string(),
                product_kind: ProductKind::Physical,
                status: EnableStatus::Active,
            },
            "tester",
        )
        .unwrap();
        assert!(VoucherCatalogDefaults::ensure_root_category_compatible(&wrong_category).is_err());

        let mut unit = VoucherCatalogDefaults::unit(UnitOfMeasureId::new("unit-1"), "tester").unwrap();
        unit.update(
            UnitOfMeasureUpdate {
                status: Some(EnableStatus::Disabled),
                ..Default::default()
            },
            "tester",
        )
        .unwrap();
        assert!(VoucherCatalogDefaults::ensure_unit_active(&unit).is_err());
    }
}
