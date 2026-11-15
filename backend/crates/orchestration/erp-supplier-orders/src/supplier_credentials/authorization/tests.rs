use async_trait::async_trait;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
};
use erp_supply::ports::jd_vop::authorization::{AuthorizationGrant, AuthorizationParameters};
use zeroize::Zeroizing;

use super::*;

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

pub(super) fn connection() -> SupplierApiConnection {
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

#[test]
fn unavailable_remote_authorization_does_not_block_local_token_binding() {
    let protocol: Result<Arc<dyn AuthorizationHttp>> = Ok(Arc::new(UnavailableHttp));
    assert_eq!(protocol.as_ref().unwrap().ensure_available(), Err(AuthorizationError::Unavailable));
    let mut connection = connection();
    assert_eq!(access::verify_binding(Some(&settings()), &protocol, &connection), Ok(()));
    connection.endpoint_reference = "wrong-endpoint".into();
    assert_eq!(
        access::verify_binding(Some(&settings()), &protocol, &connection),
        Err(AuthorizationError::BindingMismatch)
    );
}

#[test]
fn missing_configuration_keeps_local_binding_unavailable() {
    let protocol: Result<Arc<dyn AuthorizationHttp>> = Ok(Arc::new(UnavailableHttp));
    assert_eq!(access::verify_binding(None, &protocol, &connection()), Err(AuthorizationError::Unavailable));
}
