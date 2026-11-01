use async_trait::async_trait;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
use erp_identity::shared_rbac_service;
use erp_supplier::FailClosedSupplierDataScopePort;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
};
use erp_supply::ports::jd_vop::authorization::{AuthorizationGrant, AuthorizationParameters};
use persistence_core::Executor;
use zeroize::Zeroizing;

use super::*;

struct NoAudit;

#[async_trait]
impl IdentityAuditPort for NoAudit {
    fn resource_log(
        &self,
        _: AuditActor,
        _: &str,
        _: &str,
        _: String,
    ) -> erp_identity::Result<PreparedResourceAudit> {
        panic!("local binding must not prepare audit writes")
    }

    async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> erp_identity::Result<()> {
        panic!("local binding must not persist audit writes")
    }
}

struct UnavailableHttp;

#[async_trait]
impl AuthorizationHttp for UnavailableHttp {
    fn ensure_available(&self) -> Result<()> {
        Err(AuthorizationError::Unavailable)
    }

    fn verify_binding(&self, connection: &SupplierApiConnection) -> Result<()> {
        if connection.endpoint_reference == "bound-endpoint" {
            Ok(())
        } else {
            Err(AuthorizationError::BindingMismatch)
        }
    }

    fn prepare(&self, _: &JdVopAuthorizationRequest, _: &str) -> Result<AuthorizationParameters> {
        panic!("construction and binding must not prepare remote authorization")
    }

    fn exchange_parameters(&self, _: &str, _: &str, _: &str) -> Zeroizing<String> {
        panic!("construction and binding must not prepare token exchange")
    }

    async fn code(&self, _: AuthorizationParameters) -> Result<Zeroizing<String>> {
        panic!("construction and binding must not send authorization")
    }

    async fn exchange(&self, _: Zeroizing<String>) -> Result<AuthorizationGrant> {
        panic!("construction and binding must not exchange tokens")
    }
}

fn settings() -> JdVopConfig {
    serde_json::from_value(serde_json::json!({
        "enabled": true, "app_key": "app", "app_secret": "secret",
        "credential_id": "credential", "account_scope": "account",
        "credential_encryption_key": "offline-test-only-independent-key-at-least-32"
    }))
    .unwrap()
}

fn connection() -> SupplierApiConnection {
    SupplierApiConnection::new(
        SupplierApiConnectionId::new("connection"),
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new("supplier"),
            connection_code: "jd".into(),
            connector_type: SupplierConnectorType::JdVop,
            endpoint_reference: "bound-endpoint".into(),
            credential_reference: None,
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Disabled,
        },
        "actor",
    )
    .unwrap()
}

async fn process(
    settings: Option<&JdVopConfig>,
    http: Result<Arc<dyn AuthorizationHttp>>,
) -> JdVopAuthorizationProcess {
    // Construct handles only; no Mongo command is issued by the production constructor or binding check.
    let client = mongodb::Client::with_uri_str("mongodb://localhost:27017").await.unwrap();
    let db = client.database("jd-authorization-binding-unit-test");
    let rbac = shared_rbac_service(db.clone(), Arc::new(NoAudit));
    let supplier_access = SupplierAccess::new(db.clone(), FailClosedSupplierDataScopePort::shared());
    JdVopAuthorizationProcess::new(
        db,
        settings,
        rbac,
        supplier_access,
        Arc::new(JdVopAuthorizationTasks::default()),
        http,
    )
}

#[tokio::test]
async fn unavailable_authorization_engine_does_not_block_local_token_binding() {
    let process = process(Some(&settings()), Ok(Arc::new(UnavailableHttp))).await;
    assert!(matches!(process.engine, Err(AuthorizationError::Unavailable)));
    let mut connection = connection();
    assert_eq!(process.access.binding(&connection), Ok(()));
    connection.endpoint_reference = "wrong-endpoint".into();
    assert_eq!(process.access.binding(&connection), Err(AuthorizationError::BindingMismatch));
}

#[tokio::test]
async fn missing_authorization_configuration_keeps_unavailable_binding() {
    let process = process(None, Ok(Arc::new(UnavailableHttp))).await;
    assert!(matches!(process.engine, Err(AuthorizationError::Unavailable)));
    assert_eq!(process.access.binding(&connection()), Err(AuthorizationError::Unavailable));
}
