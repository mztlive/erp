//! 将目录和身份提供方公开读取映射为采购责任消费事实。

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_core::{
    ids::{ProductCategoryId, SkuId},
    AccountKind,
};
use erp_identity::{AccessControlExt, AccountCore};
use erp_procurement::entity::{
    facts::IdentityOwnerFact, procurement_responsibility::ProcurementCatalogBundle,
};
use erp_procurement::ports::procurement_responsibility::ProcurementResponsibilityFactsPort;
use persistence_core::Executor;

pub(super) struct ResponsibilityFactsAdapter {
    pub(super) db: mongodb::Database,
}

/// 保留提供方真实账号资格判断；姓名仅作为展示值透传。
fn owner_fact(account: AccountCore) -> IdentityOwnerFact {
    let can_login = account.can_login();
    let is_admin = account.is_kind(AccountKind::Admin);
    IdentityOwnerFact {
        id: account.base.id,
        name: account.name,
        can_login,
        is_admin,
    }
}

#[async_trait]
impl ProcurementResponsibilityFactsPort for ResponsibilityFactsAdapter {
    async fn load_catalog(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<ProcurementCatalogBundle> {
        Ok(
            erp_read_models::purchase_center::procurement_responsibility::load_procurement_catalog_bundle(
                &self.db, sku_ids, executor,
            )
            .await?,
        )
    }
    async fn load_owners(
        &self,
        owner_ids: &[String],
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Vec<IdentityOwnerFact>> {
        Ok(self
            .db
            .accounts()
            .list_procurement_responsibility_owners(owner_ids, executor)
            .await?
            .into_iter()
            .map(owner_fact)
            .collect())
    }
    async fn load_owner(
        &self,
        owner_id: &str,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<Option<IdentityOwnerFact>> {
        Ok(self
            .db
            .accounts()
            .find_procurement_responsibility_owner(owner_id, executor)
            .await?
            .map(owner_fact))
    }
    async fn sku_exists(&self, id: &SkuId, executor: &mut dyn Executor) -> persistence_core::Result<bool> {
        Ok(self
            .db
            .skus()
            .has_procurement_responsibility_sku(id, executor)
            .await?)
    }
    async fn category_exists(
        &self,
        id: &ProductCategoryId,
        executor: &mut dyn Executor,
    ) -> persistence_core::Result<bool> {
        Ok(self
            .db
            .product_categories()
            .has_procurement_responsibility_category(id, executor)
            .await?)
    }
}
