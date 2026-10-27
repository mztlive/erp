use application_core::AuditActor;
use entity_core::BaseModel;
use erp_catalog::CatalogExt;
use erp_core::common::time::Instant;
use erp_identity::{Permission, subject};
use erp_supply::catalog_adoption::{
    CatalogAdoptionExt, CatalogAdoptionPreview, MAX_CONFIRM_ROWS, MAX_PREVIEW_ROWS, PreviewAdoptionRequest,
};
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::{self, SupplierCatalogExt};
use erp_supply::service::supplier_api::SupplierApiService;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction};
use serde::Serialize;

use super::SupplierCatalogAdoptionProcess;
use crate::adapters::catalog_access;
use crate::{Error, Result};

#[derive(Serialize)]
pub struct AdoptionPreviewRow {
    pub item: SupplierCatalogItem,
    pub adopted: bool,
    pub missing_fields: Vec<String>,
    pub blocked: bool,
    pub blocking_reasons: Vec<String>,
}

#[derive(Serialize)]
pub struct AdoptionPreviewView {
    pub preview_id: String,
    pub version: u64,
    pub rows: Vec<AdoptionPreviewRow>,
    pub total: usize,
    pub max_confirm_rows: usize,
}

#[derive(Serialize)]
pub struct AdoptionTargetView {
    pub sku_id: String,
    pub sku_no: String,
    pub product_id: String,
    pub sku_version: u64,
    pub product_version: u64,
    pub unit_id: String,
    pub specification: String,
}

impl SupplierCatalogAdoptionProcess {
    /// 冻结勾选或完整筛选结果，禁止只冻结当前页面或截断超限结果。
    /// # 参数
    /// 连接、明确选择与当前操作人。
    /// # 返回
    /// 一小时内可分批确认的固定预览。
    /// # 错误
    /// 无权、选择无效、来源不存在或超过五百行时拒绝。
    pub async fn preview(
        &self,
        connection: &str,
        request: PreviewAdoptionRequest,
        actor: &AuditActor,
    ) -> Result<AdoptionPreviewView> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        self.ensure_permission(actor, "supplier_offering_cost:detail").await?;
        request.validate()?;
        SupplierApiService::new(self.db.clone()).load_connection(connection, &mut NoTransaction).await?;
        let items = self.selected_items(connection, request).await?;
        let encoded = serde_json::to_vec(&items).map_err(|_| Error::Internal("选品资料无法编码".into()))?;
        if encoded.len() > 8 * 1024 * 1024 {
            return Err(Error::ValidationError("本次选品资料过多，请缩小范围后分批预览".into()));
        }
        let total = items.len();
        let preview = CatalogAdoptionPreview {
            base: BaseModel::new(next_id()),
            connection_id: connection.into(),
            actor_id: actor.id().into(),
            expires_at: Instant::from_unix_secs(Instant::now().unix_secs() + 3600),
            items,
        };
        self.db.catalog_adoption_previews().create(&preview, &mut NoTransaction).await?;
        let rows = preview
            .items
            .into_iter()
            .map(|item| {
                let missing_fields = if item.offering_id.is_some() {
                    vec![]
                } else {
                    vec!["公司分类".into(), "公司品牌".into(), "基础单位与规格核对".into(), "供给条款".into()]
                };
                let blocking_reasons = vec![];
                AdoptionPreviewRow {
                    adopted: item.offering_id.is_some(),
                    blocked: !blocking_reasons.is_empty(),
                    blocking_reasons,
                    item,
                    missing_fields,
                }
            })
            .collect();
        Ok(AdoptionPreviewView {
            preview_id: preview.base.id,
            version: preview.base.version,
            rows,
            total,
            max_confirm_rows: MAX_CONFIRM_ROWS,
        })
    }

    async fn selected_items(
        &self,
        connection: &str,
        request: PreviewAdoptionRequest,
    ) -> Result<Vec<SupplierCatalogItem>> {
        if let Some(ids) = request.item_ids {
            let mut items = Vec::with_capacity(ids.len());
            for id in ids {
                items.push(self.load_item(connection, &id, &mut NoTransaction).await?);
            }
            return Ok(items);
        }
        let filter = request.filter.ok_or_else(|| Error::ValidationError("缺少选品筛选条件".into()))?;
        let mut items = Vec::new();
        for page in 1..=5 {
            let (rows, total) =
                supplier_catalog::filtered_page(&self.db, connection, &filter, page, 100, &mut NoTransaction)
                    .await?;
            if total > u64::try_from(MAX_PREVIEW_ROWS).expect("500 可表示") {
                return Err(Error::ValidationError(
                    "当前筛选超过 500 个规格，请缩小筛选范围后分批选品".into(),
                ));
            }
            items.extend(rows);
            if u64::try_from(items.len()).expect("最多500项") >= total {
                break;
            }
        }
        if items.is_empty() {
            return Err(Error::ValidationError("当前筛选没有可选商品".into()));
        }
        Ok(items)
    }

    /// 读取现有 SKU 的公司身份和并发版本，用于明确匹配。
    /// # 参数
    /// 连接、目标 SKU 和认证操作人。
    /// # 返回
    /// 已按公司商品读取权限过滤的目标版本。
    /// # 错误
    /// 无权、连接或商品不存在、商品停用时拒绝。
    pub async fn target(
        &self,
        connection: &str,
        sku_id: &str,
        actor: &AuditActor,
    ) -> Result<AdoptionTargetView> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        SupplierApiService::new(self.db.clone()).load_connection(connection, &mut NoTransaction).await?;
        let sku = self
            .db
            .skus()
            .find_by_id(sku_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("公司商品规格不存在".into()))?;
        let product = catalog_access(self.db.clone(), self.rbac.clone())
            .require_product(actor, "detail", sku.product_id.as_ref(), &mut NoTransaction)
            .await?;
        if !sku.is_active() || !product.is_active() {
            return Err(Error::BusinessLogicError("请选择启用的公司商品规格".into()));
        }
        Ok(AdoptionTargetView {
            sku_id: sku.base.id,
            sku_no: sku.sku_no,
            product_id: product.base.id,
            sku_version: sku.base.version,
            product_version: product.base.version,
            unit_id: sku.base_unit_id.to_string(),
            specification: sku.specification_signature,
        })
    }

    pub(super) async fn load_item(
        &self,
        connection: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCatalogItem> {
        self.db
            .supplier_catalog_items()
            .find_by_id(id, executor)
            .await?
            .filter(|item| item.connection_id == connection)
            .ok_or_else(|| Error::NotFound("供应商商品不存在".into()))
    }

    pub(super) async fn ensure_permission(&self, actor: &AuditActor, permission: &str) -> Result<()> {
        if !self.rbac.enforce(&subject(actor.kind(), actor.id()), &Permission::parse(permission)?).await? {
            return Err(Error::Forbidden("当前账号无权执行选品操作".into()));
        }
        Ok(())
    }
}
