use std::collections::HashSet;

use database::{AccessControlExt, CatalogExt, FileAssetExt, NoTransaction, Transactional};
use entities::catalog::product_brand::{ProductBrand, ProductBrandData, ProductBrandUpdate};
use entities::catalog::{EnableStatus, ProductBrandId};
use id_generator::next_id;
use validator::Validate;

use super::support::ensure_version;
use super::CatalogService;
use crate::audit::AuditActor;
use crate::catalog::dto::{
    CreateProductBrandRequest, PageView, ProductBrandListParams, ProductBrandView, SortDir,
    UpdateProductBrandRequest,
};
use crate::errors::Result;
use crate::file_asset::PendingFileAssetRequest;
use crate::pending_file_assets::PendingFileAssets;

/// 商品品牌列表筛选条件类型。
type ProductBrandFilter = <mongodb::Database as CatalogExt>::ProductBrandFilter;

impl CatalogService {
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

    /// 创建商品品牌（品牌、文件元数据与审计日志在同一事务内提交）。
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
        self.product_brand_create_with_assets(req, Vec::new(), actor)
            .await
    }

    /// 创建品牌，并把同一次 multipart 命令携带的 Logo 文件资产原子登记。
    ///
    /// # 参数
    /// * `req` - 品牌创建请求
    /// * `asset_requests` - 已写入对象存储、尚未登记的文件资产
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建品牌视图。
    ///
    /// # 错误
    /// Logo 引用无效、品牌代码冲突或事务写入失败时返回错误。
    pub async fn product_brand_create_with_assets(
        &self,
        mut req: CreateProductBrandRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ProductBrandView> {
        req.validate()?;
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let mut used = HashSet::new();
        if let Some(asset_id) = req.logo_file_asset_id.as_mut() {
            pending_assets.resolve_id(asset_id, &mut used)?;
        }
        pending_assets.ensure_all_used(&used)?;
        self.ensure_brand_logo_exists(req.logo_file_asset_id.as_ref(), &pending_assets)
            .await?;
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
        let db = self.db.clone();
        let client = db.client().clone();
        let brand_for_tx = brand.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    pending_assets.persist(&db, session).await?;
                    db.product_brands().create(&brand_for_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;
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
        self.product_brand_update_with_assets(id, req, Vec::new(), actor)
            .await
    }

    /// 更新品牌，并把同一次 multipart 命令携带的 Logo 文件资产原子登记。
    ///
    /// # 参数
    /// * `id` - 品牌 ID
    /// * `req` - 品牌更新请求
    /// * `asset_requests` - 已写入对象存储、尚未登记的文件资产
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后的品牌视图。
    ///
    /// # 错误
    /// Logo 引用无效、版本冲突或事务写入失败时返回错误。
    pub async fn product_brand_update_with_assets(
        &self,
        id: &str,
        mut req: UpdateProductBrandRequest,
        asset_requests: Vec<PendingFileAssetRequest>,
        actor: &AuditActor,
    ) -> Result<ProductBrandView> {
        req.validate()?;
        let pending_assets = PendingFileAssets::prepare(asset_requests, actor)?;
        let mut used = HashSet::new();
        if let Some(Some(asset_id)) = req.logo_file_asset_id.as_mut() {
            pending_assets.resolve_id(asset_id, &mut used)?;
        }
        pending_assets.ensure_all_used(&used)?;
        self.ensure_brand_logo_exists(
            req.logo_file_asset_id.as_ref().and_then(Option::as_ref),
            &pending_assets,
        )
        .await?;
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
                    pending_assets.persist(&db, session).await?;
                    db.product_brands().update(&mut brand, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<ProductBrand, crate::errors::Error>(brand)
                })
            })
            .await?;
        Ok(updated.into())
    }

    /// 校验既有 Logo 文件资产存在；本次待登记资产由调用方事务负责。
    async fn ensure_brand_logo_exists(
        &self,
        asset_id: Option<&entities::ids::FileAssetId>,
        pending_assets: &PendingFileAssets,
    ) -> Result<()> {
        let Some(asset_id) = asset_id else {
            return Ok(());
        };
        if pending_assets.contains_id(asset_id) {
            return Ok(());
        }
        self.db
            .file_assets()
            .find_by_id(asset_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| crate::errors::Error::NotFound("品牌 Logo 文件不存在".to_string()))?;
        Ok(())
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
}
