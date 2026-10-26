//! 人工绑定现有公司 SKU；同步任务始终保留此匹配。
use application_core::AuditActor;
use erp_audit::AuditActorLogs;
use erp_catalog::repository::CatalogExt;
use erp_supply::dto::supplier_catalog::MatchCatalogItemRequest;
use erp_supply::entity::supplier_catalog::SupplierCatalogItem;
use erp_supply::repository::supplier_catalog::SupplierCatalogExt;
use persistence_core::Transactional;

use super::SupplierApiGovernanceProcess;
use crate::adapters::catalog_access;
use crate::audit::persist_log;
use crate::{Error, Result};

impl SupplierApiGovernanceProcess {
    /// 明确匹配一个供应商规格和一个已有公司 SKU。
    /// # 参数
    /// 连接和目录身份来自路径；版本及目标 SKU 来自人工选择。
    /// # 返回
    /// 已保存的目录匹配；重复选择相同 SKU 不增加变更。
    /// # 错误
    /// 无权限、目录跨连接、版本冲突、商品不在范围或已停用时拒绝。
    pub async fn match_catalog_item(
        &self,
        id: &str,
        item_id: &str,
        request: MatchCatalogItemRequest,
        actor: &AuditActor,
    ) -> Result<SupplierCatalogItem> {
        self.ensure_permission(actor, "supplier_api_connection:catalog_sync").await?;
        if request.sku_id.trim().is_empty() || request.expected_version == 0 {
            return Err(Error::ValidationError("请选择公司商品并刷新目录版本".into()));
        }
        let rbac = self.rbac.clone().ok_or_else(|| Error::Forbidden("当前账号不能匹配商品".into()))?;
        let db = self.db.clone();
        let client = db.client().clone();
        let (id, item_id, actor) = (id.to_owned(), item_id.to_owned(), actor.clone());
        client
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let repository = db.supplier_catalog_items();
                    let mut item = repository
                        .find_by_id(&item_id, executor)
                        .await?
                        .filter(|item| item.connection_id == id)
                        .ok_or_else(|| Error::NotFound("供应商商品不存在".into()))?;
                    let sku = db
                        .skus()
                        .find_by_id(&request.sku_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("公司商品规格不存在".into()))?;
                    let product = catalog_access(db.clone(), rbac)
                        .require_product(&actor, "detail", sku.product_id.as_ref(), executor)
                        .await?;
                    if !sku.is_active() || !product.is_active() {
                        return Err(Error::BusinessLogicError("请选择启用的公司商品规格".into()));
                    }
                    if !item.match_sku(request.expected_version, sku.base.id, sku.sku_no, product.base.id)? {
                        return Ok(item);
                    }
                    repository.update(&mut item, executor).await?;
                    let audit =
                        actor.resource_log("supplier_catalog.match", "supplier_api_connection", id)?;
                    persist_log(&db, &audit, executor).await?;
                    Ok(item)
                })
            })
            .await
    }
}
