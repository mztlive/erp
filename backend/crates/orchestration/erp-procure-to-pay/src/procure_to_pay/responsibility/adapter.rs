//! 将目录和身份提供方公开读取映射为采购责任消费事实。

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::{ProductCategoryId, SkuId};
use erp_identity::repository::prelude::*;
use erp_identity::{AccessControlExt, AccountSummary};
use erp_procurement::entity::procurement_responsibility::ProcurementCatalogBundle;
use erp_procurement::ports::procurement_responsibility::ProcurementResponsibilityFactsPort;
use persistence_core::Executor;

pub(super) struct ResponsibilityFactsAdapter {
    pub(super) db: mongodb::Database,
}

#[async_trait]
impl ProcurementResponsibilityFactsPort for ResponsibilityFactsAdapter {
    async fn load_catalog(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<ProcurementCatalogBundle> {
        Ok(erp_read_models::purchase_center::procurement_responsibility::load_procurement_catalog_bundle(
            &self.db, sku_ids, executor,
        )
        .await?)
    }
    async fn load_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<AccountSummary>> {
        Ok(self
            .db
            .accounts()
            .list_procurement_responsibility_owners(owner_ids, executor)
            .await?
            .into_iter()
            .map(|account| AccountSummary::from(&account))
            .collect())
    }
    async fn load_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Option<AccountSummary>> {
        Ok(self
            .db
            .accounts()
            .find_procurement_responsibility_owner(owner_id, executor)
            .await?
            .map(|account| AccountSummary::from(&account)))
    }
    async fn sku_exists(&self, id: &SkuId, executor: &mut dyn Executor) -> persistence_core::Result<bool> {
        Ok(self.db.skus().has_procurement_responsibility_sku(id, executor).await?)
    }
    async fn category_exists(
        &self,
        id: &ProductCategoryId,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<bool> {
        Ok(self.db.product_categories().has_procurement_responsibility_category(id, executor).await?)
    }
}
