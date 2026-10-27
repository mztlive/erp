use application_core::AuditActor;
use entity_core::BaseModel;
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
        let source =
            SupplierApiService::new(self.db.clone()).load_connection(connection, &mut NoTransaction).await?;
        self.ensure_source(&source)?;
        let items = self.selected_items(connection, request).await?;
        for item in &items {
            source.ensure_offering_source(&item.supplier_id)?;
        }
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
                    vec!["公司计量单位".into(), "新建时所需分类".into(), "供给条款".into()]
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
