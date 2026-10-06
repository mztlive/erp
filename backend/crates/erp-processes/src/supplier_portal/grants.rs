//! 供应商定向SKU报价开放：商品读取资格与供应商配置资格独立验证。

use application_core::AuditActor;
use erp_catalog::CatalogExt;
use erp_supply::portal::{PortalOfferingService, QuoteAccessGrant};
use persistence_core::NoTransaction;
use serde_json::json;

use super::{PortalGrantInput, SupplierPortalProcess};
use crate::adapters::catalog_access;
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 在供应商对象范围内读取定向报价记录。
    /// # 参数
    /// 指定供应商及内部真实身份。
    /// # 返回
    /// 返回精确供应商的开放记录。
    /// # 错误
    /// 无对象范围时拒绝。
    pub async fn quote_access_list(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<QuoteAccessGrant>> {
        self.internal_supplier(actor, "detail", supplier_id, &mut NoTransaction).await?;
        Ok(PortalOfferingService::new(self.db.clone())
            .quote_access_list(supplier_id, &mut NoTransaction)
            .await?)
    }

    /// 开放或撤销本供应商对指定SKU的首次报价资格。
    /// # 参数
    /// 供应商、精确SKU、原开放版本及内部真实身份。
    /// # 返回
    /// 返回已保存的定向开放版本。
    /// # 错误
    /// 对象范围、商品状态或版本失效时拒绝。
    pub async fn quote_access_update(
        &self,
        input: PortalGrantInput,
        actor: &AuditActor,
    ) -> Result<QuoteAccessGrant> {
        let payload = json!({"input":input});
        let supplier_id = input.supplier_id.clone();
        let key = input.idempotency_key.clone();
        self.internal_command(
            actor,
            &supplier_id,
            "supplier_portal.quote_access_update",
            &key,
            &payload,
            move |this, actor, executor| {
                Box::pin(async move {
                    let sku = this
                        .db
                        .skus()
                        .find_by_id(&input.sku_id, executor)
                        .await?
                        .ok_or_else(|| Error::NotFound("公司SKU不存在".into()))?;
                    let product = catalog_access(this.db.clone(), this.rbac.clone())
                        .require_product(&actor, "detail", sku.product_id.as_ref(), executor)
                        .await?;
                    if input.active && (!sku.is_active() || !product.is_active()) {
                        return Err(Error::BusinessLogicError("停用商品或SKU不能开放报价".into()));
                    }
                    Ok(PortalOfferingService::new(this.db.clone())
                        .quote_access_update(
                            &input.supplier_id,
                            &input.sku_id,
                            input.active,
                            input.expected_version,
                            &actor,
                            executor,
                        )
                        .await?)
                })
            },
        )
        .await
    }
}
