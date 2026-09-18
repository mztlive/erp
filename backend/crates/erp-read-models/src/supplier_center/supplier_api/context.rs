use application_core::AuditActor;
use erp_core::ids::SupplierApiConnectionId;
use erp_identity::{Permission, subject};
use erp_supply::entity::supplier_api::{
    BusinessCapabilityConfirmation, SupplierApiCapability, SupplierApiConnection, SupplierConnectionAction,
    SupplierHealthCheckRun,
};
use erp_supply::repository::SupplierApiExt;
use erp_supply::service::supplier_api::context::action_permission;
use erp_support::BulkJobExt;
use erp_support::repository::prelude::*;

use super::SupplierApiReadService;
use crate::Result;
type SupplierConnectionImpact = <mongodb::Database as SupplierApiExt>::SupplierConnectionImpact;
pub(super) struct GovernanceContext {
    pub(super) capabilities: Vec<SupplierApiCapability>,
    pub(super) confirmations: Vec<BusinessCapabilityConfirmation>,
    pub(super) health_runs: Vec<SupplierHealthCheckRun>,
    pub(super) impact: SupplierConnectionImpact,
}
impl SupplierApiReadService {
    pub(super) async fn governance_context(
        &self,
        connection: &SupplierApiConnection,
        executor: &mut dyn persistence_core::Executor,
    ) -> Result<GovernanceContext> {
        let data = self
            .db
            .supplier_api()
            .governance_data(&SupplierApiConnectionId::new(&connection.base.id), 50, executor)
            .await?;
        let active_sync_jobs = self
            .db
            .background_jobs()
            .count_active_supplier_catalog_jobs(&connection.base.id, executor)
            .await?;
        Ok(GovernanceContext {
            capabilities: data.capabilities,
            confirmations: data.confirmations,
            health_runs: data.health_runs,
            impact: data.owned_impact.with_active_sync_jobs(active_sync_jobs),
        })
    }
    pub(super) async fn has_action_permission(
        &self,
        actor: &AuditActor,
        action: SupplierConnectionAction,
    ) -> Result<bool> {
        self.has_permission(actor, action_permission(action)).await
    }
    pub(super) async fn has_permission(&self, actor: &AuditActor, permission: &str) -> Result<bool> {
        let Some(rbac) = self.rbac.as_ref() else {
            return Ok(false);
        };
        let permission = Permission::parse(permission)?;
        Ok(rbac.enforce(&subject(actor.kind(), actor.id()), &permission).await?)
    }
}
