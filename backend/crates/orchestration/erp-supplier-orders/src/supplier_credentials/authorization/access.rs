use std::sync::Arc;

use application_core::AuditActor;
use config::JdVopConfig;
use erp_identity::{Permission, SharedRbacService, subject};
use erp_supplier::SupplierAccess;
use erp_supply::entity::supplier_api::{SupplierApiConnection, SupplierConnectorType};
use erp_supply::ports::jd_vop::authorization::AuthorizationHttp;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::Executor;

use super::{AuthorizationError, Result};

#[derive(Clone)]
pub(super) struct ConnectionAccess {
    pub(super) db: Database,
    pub(super) rbac: SharedRbacService,
    pub(super) supplier_access: SupplierAccess,
    pub(super) settings: Option<JdVopConfig>,
    pub(super) protocol: Result<Arc<dyn AuthorizationHttp>>,
}

impl ConnectionAccess {
    pub(super) async fn permission(&self, actor: &AuditActor, action: &str) -> Result<bool> {
        let permission = Permission::parse(format!("supplier_api_connection:{action}"))
            .map_err(|_| AuthorizationError::Unavailable)?;
        self.rbac
            .enforce(&subject(actor.kind(), actor.id()), &permission)
            .await
            .map_err(|_| AuthorizationError::Unavailable)
    }

    pub(super) async fn require(
        &self,
        id: &str,
        actor: &AuditActor,
        action: &str,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiConnection> {
        if !self.permission(actor, action).await? {
            return Err(AuthorizationError::Forbidden);
        }
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(id, executor)
            .await
            .map_err(|_| AuthorizationError::Forbidden)?;
        self.supplier_access
            .require_with(actor, "detail", &connection.supplier_id, executor)
            .await
            .map_err(|_| AuthorizationError::Forbidden)?;
        if connection.connector_type != Some(SupplierConnectorType::JdVop) || connection.base.is_deleted() {
            return Err(AuthorizationError::Forbidden);
        }
        Ok(connection)
    }

    pub(super) fn binding(&self, connection: &SupplierApiConnection) -> Result<()> {
        self.settings.as_ref().ok_or(AuthorizationError::Unavailable)?;
        self.protocol.as_ref().map_err(|error| *error)?.verify_binding(connection)
    }
}
