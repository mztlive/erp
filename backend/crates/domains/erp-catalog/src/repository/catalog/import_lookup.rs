//! 导入用精确查找：品牌名称、分类名称、商品编号、计量单位。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::doc;
use persistence_core::{Executor, Result, mongo_ops};

use crate::entity::catalog::{EnableStatus, Product, ProductBrand, ProductCategory, UnitOfMeasure};

/// 导入用品牌精确查找。
#[allow(async_fn_in_trait)]
pub trait ProductBrandRepositoryImportLookupExt {
    /// 按重放身份、代码、精确名称依次查找品牌，不忽略停用记录。既有记录；身份和代码查找包括软删除，名称查找仅限未删除记录。
    ///
    /// - `id`：本次导入的稳定品牌身份。
    /// - `code`：用户明确填写的代码；空代码不查。
    /// - `executor`：当前行事务执行器。
    async fn find_import_match(
        &self,
        id: &str,
        code: Option<&str>,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>>;

    /// 按品牌名称精确查找启用中的品牌。
    ///
    /// 命中时返回品牌实体。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>>;

    /// 按品牌代码精确查找启用中的品牌。
    ///
    /// 命中时返回品牌实体。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    async fn find_enabled_by_code(
        &self,
        brand_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>>;
}

impl ProductBrandRepositoryImportLookupExt for persistence_core::Repository<'_, ProductBrand> {
    async fn find_import_match(
        &self,
        id: &str,
        code: Option<&str>,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductBrand>> {
        if let Some(brand) = self.find_by_id_including_deleted(id, executor).await? {
            return Ok(Some(brand));
        }
        if let Some(code) = code
            && let Some(brand) =
                self.find_one_by_field_including_deleted("brand_code", code, executor).await?
        {
            return Ok(Some(brand));
        }
        self.find_one_by_field("name", name, executor).await
    }

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
    /// 命中时返回分类实体。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    async fn find_enabled_by_exact_name(
        &self,
        name: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductCategory>>;

    /// 按分类代码精确查找启用中的分类。
    ///
    /// 命中时返回分类实体。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
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
    /// 命中时返回商品实体。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
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
    /// 命中时返回计量单位。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
    ///
    /// `unit_code` 为稳定单位代码。
    async fn find_enabled_by_code(
        &self,
        unit_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<UnitOfMeasure>>;

    /// 按单位名称精确查找启用中的计量单位。
    ///
    /// 命中时返回计量单位。MongoDB 查询失败时返回错误。
    ///
    /// 沿用调用方传入的执行器。
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
