//! 供应商定向SKU报价开放：商品读取资格与供应商配置资格独立验证。

use application_core::AuditActor;
use erp_catalog::{CatalogExt, Product, Sku};
use erp_permissions::{catalog as catalog_permissions, supplier, supplier_portal};
use erp_supply::portal::{PortalOfferingService, QuoteAccessGrant};
use persistence_core::{Executor, Transactional};
use serde_json::json;

use super::command::scoped_command;
use super::{PortalGrantInput, SupplierPortalProcess};
use crate::{Error, Result};

impl SupplierPortalProcess {
    /// 在供应商对象范围内读取定向报价记录。
    ///
    /// 指定供应商及内部真实身份。
    ///
    /// 返回精确供应商的开放记录。
    ///
    /// 无对象范围时拒绝。
    pub async fn quote_access_list(
        &self,
        supplier_id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<QuoteAccessGrant>> {
        let process = self.clone();
        let supplier_id = supplier_id.to_owned();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let access = Self::management_supplier(
                        &process.db,
                        &process.services.suppliers,
                        &actor,
                        &supplier_id,
                        executor,
                    )
                    .await?;

                    let grants = PortalOfferingService::new(process.db.clone())
                        .quote_access_list(&supplier_id, executor)
                        .await?;
                    access.validate(executor).await?;
                    Ok(grants)
                })
            })
            .await
    }

    /// 开放或撤销本供应商对指定SKU的首次报价资格。
    ///
    /// 供应商、精确SKU、原开放版本及内部真实身份。
    ///
    /// 返回已保存的定向开放版本。
    ///
    /// 对象范围、商品状态或版本失效时拒绝。
    pub async fn quote_access_update(
        &self,
        input: PortalGrantInput,
        actor: &AuditActor,
    ) -> Result<QuoteAccessGrant> {
        let payload = json!({"input":input});
        let supplier_id = input.supplier_id.clone();
        let key = input.idempotency_key.clone();
        let target = input.clone();
        self.internal_target_command(
            actor,
            ("supplier_portal.quote_access_update", supplier::DETAIL, supplier_portal::ADMIN_UPDATE_GRANT),
            move |this, actor, executor| {
                Box::pin(async move {
                    this.quote_product_access(&target, &actor, executor).await?;
                    let command = scoped_command(
                        actor.id(),
                        &supplier_id,
                        "supplier_portal.quote_access_update",
                        &key,
                        &payload,
                    )?;
                    Ok((supplier_id, command))
                })
            },
            move |this, actor, executor| {
                Box::pin(async move {
                    let (sku, product) = this.quote_product_access(&input, &actor, executor).await?;
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

    // 定向报价的商品来源授权在首执行和收据回放前均执行。
    async fn quote_product_access(
        &self,
        input: &PortalGrantInput,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<(Sku, Product)> {
        let sku = self
            .db
            .skus()
            .find_by_id(&input.sku_id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("公司SKU不存在".into()))?;
        let product = self
            .services
            .catalog_access
            .require_product(
                actor,
                &catalog_permissions::PRODUCT_PRODUCT_DETAIL,
                sku.product_id.as_ref(),
                executor,
            )
            .await?;
        Ok((sku, product))
    }
}
