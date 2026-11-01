//! 导入用精确查找：品牌名称、分类名称、商品编号、计量单位。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::catalog::{EnableStatus, Product, ProductBrand, ProductCategory, UnitOfMeasure};

/// 导入用品牌精确查找。
#[allow(async_fn_in_trait)]
pub trait ProductBrandRepositoryImportLookupExt {
    /// 按品牌名称精确查找启用中的品牌。
    ///
    /// # 参数
    /// * `name` - 已规范化品牌名称
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回品牌实体。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>>;

    /// 按品牌代码精确查找启用中的品牌。
    ///
    /// # 参数
    /// * `brand_code` - 稳定品牌代码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回品牌实体。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_code(
        &self,
        brand_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>>;
}

impl ProductBrandRepositoryImportLookupExt for persistence_core::Repository<'_, ProductBrand> {
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "name": name,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn find_enabled_by_code(
        &self,
        brand_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "brand_code": brand_code,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}

/// 导入用分类精确查找。
#[allow(async_fn_in_trait)]
pub trait ProductCategoryRepositoryImportLookupExt {
    /// 按分类名称精确查找启用中的分类。
    ///
    /// # 参数
    /// * `name` - 已规范化分类名称
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回分类实体。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>>;

    /// 按分类代码精确查找启用中的分类。
    ///
    /// # 参数
    /// * `category_code` - 稳定分类代码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回分类实体。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_code(
        &self,
        category_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>>;
}

impl ProductCategoryRepositoryImportLookupExt for persistence_core::Repository<'_, ProductCategory> {
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "name": name,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn find_enabled_by_code(
        &self,
        category_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "category_code": category_code,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}

/// 导入用商品精确查找。
#[allow(async_fn_in_trait)]
pub trait ProductRepositoryImportLookupExt {
    /// 按商品编号精确查找未删除商品。
    ///
    /// # 参数
    /// * `product_no` - 商品编号
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回商品实体。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_by_product_no(
        &self,
        product_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Product>>;
}

impl ProductRepositoryImportLookupExt for persistence_core::Repository<'_, Product> {
    async fn find_by_product_no(
        &self,
        product_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Product>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "product_no": product_no,
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}

/// 导入用计量单位精确查找。
#[allow(async_fn_in_trait)]
pub trait UnitOfMeasureRepositoryImportLookupExt {
    /// 按单位代码精确查找启用中的计量单位。
    ///
    /// # 参数
    /// * `unit_code` - 稳定单位代码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回计量单位。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_code(
        &self,
        unit_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>>;

    /// 按单位名称精确查找启用中的计量单位。
    ///
    /// # 参数
    /// * `name` - 单位名称
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 命中时返回计量单位。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>>;
}

impl UnitOfMeasureRepositoryImportLookupExt for persistence_core::Repository<'_, UnitOfMeasure> {
    async fn find_enabled_by_code(
        &self,
        unit_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "unit_code": unit_code,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }

    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! {
                "name": name,
                "status": EnableStatus::Active.as_str(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
    }
}
