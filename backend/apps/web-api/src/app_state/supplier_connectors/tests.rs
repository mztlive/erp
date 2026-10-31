use std::future::pending;
use std::sync::Mutex;
use std::sync::atomic::{AtomicUsize, Ordering};

use async_trait::async_trait;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_processes::connectors::jd_vop::token::JdVopAccessToken;
use erp_supply::entity::supplier_api::SupplierApiCapabilityCode;
use erp_supply::ports::supplier_reference_registry::SupplierReferenceTarget;

use super::super::tests::connector_config;
use super::*;

#[derive(Default)]
struct Tokens {
    calls: AtomicUsize,
    settled: AtomicUsize,
    credential_ids: Mutex<Vec<String>>,
}

#[async_trait]
impl JdVopTokenProvider for Tokens {
    async fn access_token(&self, credential_id: &str) -> ConnectorResult<JdVopAccessToken> {
        self.calls.fetch_add(1, Ordering::SeqCst);
        self.credential_ids.lock().unwrap().push(credential_id.into());
        Err(unavailable("TEST_NO_IO", "测试构造不能读取凭据或远端"))
    }

    async fn settle_refresh(&self) -> ConnectorResult<()> {
        self.settled.fetch_add(1, Ordering::SeqCst);
        Ok(())
    }
}

fn jd_settings() -> &'static str {
    "[jd_vop]\nenabled=true\napp_key='test-app'\napp_secret='test-secret'\ncredential_id='test-jd-token'\ncredential_encryption_key='offline-test-only-independent-key-at-least-32'\naccount_scope='test-account'"
}

#[test]
fn jd_startup_without_mongo_provider_never_silently_registers_empty_runtime() {
    let config = connector_config(jd_settings());
    let error = ExternalConnectorPorts::from_config(&config).err().unwrap();
    assert_eq!(error.code, "JD_VOP_MONGO_PROVIDER_REQUIRED");
}

#[test]
fn jd_only_read_transport_is_assembled_without_token_reads_or_execution_capabilities() {
    let config = connector_config(jd_settings());
    let tokens = Arc::new(Tokens::default());
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens.clone())).unwrap();
    assert!(ports.dangaoshushu.is_none());
    assert!(ports.order_connectors.is_some());
    assert!(ports.jd_vop_credentials.is_some());
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 0);
    assert_eq!(ports.readiness.supplier_api, ConnectorMode::Configured);
    assert_eq!(ports.readiness.supplier_fulfillment, ConnectorMode::FailClosed);
    let capabilities = ports
        .supplier_reference_registry
        .capabilities(&SupplierReferenceTarget {
            connection_id: SupplierApiConnectionId::new("test-connection"),
            connector_type: Some(SupplierConnectorType::JdVop),
            supplier_id: SupplierAccountId::new("test-supplier"),
        })
        .unwrap();
    assert!(capabilities.contains(&SupplierApiCapabilityCode::Product));
    for capability in [
        SupplierApiCapabilityCode::Order,
        SupplierApiCapabilityCode::OrderConfirmation,
        SupplierApiCapabilityCode::ReceiptConfirmation,
        SupplierApiCapabilityCode::Cancel,
        SupplierApiCapabilityCode::Settlement,
    ] {
        assert!(!capabilities.contains(&capability));
    }
}

#[test]
fn jd_mongo_read_assembly_preserves_existing_supplier_registration() {
    let config = connector_config(&format!(
        "[dangaoshushu]\nenabled=true\nchannel_no='test-channel'\nprivate_key='test-key'\n{}",
        jd_settings(),
    ));
    let tokens = Arc::new(Tokens::default());
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens)).unwrap();
    assert!(ports.dangaoshushu.is_some());
    assert_eq!(
        ports.supplier_api.catalog_connector_types(),
        vec![SupplierConnectorType::Dangaoshushu, SupplierConnectorType::JdVop]
    );
}

#[tokio::test]
async fn maintenance_checks_only_configured_identity_and_disabled_technology_has_no_job() {
    let mut config = connector_config(jd_settings());
    let tokens = Arc::new(Tokens::default());
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens.clone())).unwrap();
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 0);
    let error = ports.jd_vop_credentials.as_ref().unwrap().run_once().await.unwrap_err();
    assert_eq!(error.code, "TEST_NO_IO");
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 1);
    assert_eq!(*tokens.credential_ids.lock().unwrap(), ["test-jd-token"]);
    ports.jd_vop_credentials.as_ref().unwrap().settle_refresh().await.unwrap();
    assert_eq!(tokens.settled.load(Ordering::SeqCst), 1);
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 1);
    config.jd_vop.as_mut().unwrap().enabled = false;
    let disabled = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens.clone())).unwrap();
    assert!(disabled.jd_vop_credentials.is_none());
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 1);
}

struct PendingTokens(Arc<AtomicUsize>);

#[async_trait]
impl JdVopTokenProvider for PendingTokens {
    async fn access_token(&self, _: &str) -> ConnectorResult<JdVopAccessToken> {
        self.0.fetch_add(1, Ordering::SeqCst);
        pending().await
    }
}

#[tokio::test]
async fn maintenance_timeout_preserves_failure_and_never_retries_provider() {
    let calls = Arc::new(AtomicUsize::new(0));
    let maintenance = JdVopCredentialMaintenance {
        tokens: Arc::new(PendingTokens(calls.clone())),
        credential_id: "test-jd-token".into(),
        timeout: Duration::from_millis(1),
    };
    assert_eq!(maintenance.run_once().await.unwrap_err().code, "JD_VOP_CREDENTIAL_MAINTENANCE_TIMEOUT");
    assert_eq!(calls.load(Ordering::SeqCst), 1);
}

fn execution_settings() -> String {
    format!(
        "{}\n{}",
        jd_settings(),
        r#"
[jd_vop.execution]
evidence_reference='offline-verified-fixture'
account_scope='test-account'
app_key='test-app'
external_funds_account_ref='funds-account'
confirmation_funds_evidence='offline-original-funds-fixture'
preflight_validity_seconds=60
ordinary_skus=['123']
[jd_vop.execution.invoice]
invoice_type=23
invoice_put_type=2
invoice_selected_title=4
invoice_content_type=1
invoice_company_name='测试公司'
"#
    )
}

fn bound_connection(
    ports: &ExternalConnectorPorts,
) -> erp_supply::entity::supplier_api::SupplierApiConnection {
    use erp_supply::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };
    let mut connection = SupplierApiConnection::new(
        SupplierApiConnectionId::new("jd-connection"),
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new("test-supplier"),
            connection_code: "jd".into(),
            connector_type: SupplierConnectorType::JdVop,
            endpoint_reference: String::new(),
            credential_reference: None,
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Disabled,
        },
        "actor",
    )
    .unwrap();
    let refs = ports
        .supplier_reference_registry
        .deployment_config(&SupplierReferenceTarget::from(&connection))
        .unwrap();
    connection.endpoint_reference = refs.endpoint;
    connection.credential_reference = Some(refs.credential);
    connection.endpoint_reference_bound = true;
    connection.credential_reference_bound = true;
    connection.supplier_deposit_external_account_ref = Some("funds-account".into());
    connection
}

#[test]
fn startup_execution_uses_verified_profile_without_reading_tokens_or_bypassing_business_gate() {
    let config = connector_config(&execution_settings());
    let tokens = Arc::new(Tokens::default());
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens.clone())).unwrap();
    let mut connection = bound_connection(&ports);
    let registry = ports.order_connectors.as_ref().unwrap();
    let connector = registry.fulfillment_connector(&connection).unwrap();
    assert!(connector.orders().unwrap().supports_creation());
    assert!(connector.preflight().is_some());
    assert!(connector.order_confirmation().is_some());
    assert!(connector.receipt_confirmation().is_some());
    assert!(connector.cancellations().is_some());
    assert!(connector.messages().is_none());
    assert!(registry.creation_connector(&connection).is_err());
    connection.supplier_deposit_external_account_ref = Some("different-account".into());
    assert_eq!(
        registry.fulfillment_connector(&connection).err().unwrap().code,
        "JD_VOP_FUNDS_ACCOUNT_MISMATCH"
    );
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 0);
}

#[tokio::test]
async fn startup_messages_work_without_order_profile_and_do_not_expose_order_writes() {
    let config = connector_config(&format!(
        "{}\n{}",
        jd_settings(),
        r#"
[jd_vop.messages]
evidence_reference='offline-message-fixture'
account_scope='test-account'
app_key='test-app'
types=[1,2]
"#
    ));
    let tokens = Arc::new(Tokens::default());
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(tokens.clone())).unwrap();
    let connection = bound_connection(&ports);
    let connector = ports.order_connectors.as_ref().unwrap().fulfillment_connector(&connection).unwrap();
    let messages = connector.messages().unwrap();
    assert_eq!(messages.account_scope(), "test-account");
    assert_eq!(messages.poll_interval(), Duration::from_secs(60));
    assert!(connector.orders().is_none());
    assert!(connector.order_confirmation().is_none());
    assert!(connector.preflight().is_none());
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 0);
    assert!(messages.pull(std::num::NonZeroU32::new(1).unwrap()).await.is_err());
    assert_eq!(tokens.calls.load(Ordering::SeqCst), 1);
}

#[test]
fn startup_rejects_invalid_execution_and_keeps_missing_confirmation_read_only_for_orders() {
    let mut config = connector_config(&execution_settings());
    config.jd_vop.as_mut().unwrap().execution.as_mut().unwrap().invoice.invoice_company_name.clear();
    assert!(ExternalConnectorPorts::with_jd_tokens(&config, Some(Arc::new(Tokens::default()))).is_err());
    let mut config = connector_config(&execution_settings());
    config.jd_vop.as_mut().unwrap().execution.as_mut().unwrap().account_scope = "other-account".into();
    assert!(ExternalConnectorPorts::with_jd_tokens(&config, Some(Arc::new(Tokens::default()))).is_err());
    let mut config = connector_config(&execution_settings());
    config.jd_vop.as_mut().unwrap().execution.as_mut().unwrap().confirmation_funds_evidence = None;
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(Arc::new(Tokens::default()))).unwrap();
    let connector =
        ports.order_connectors.as_ref().unwrap().fulfillment_connector(&bound_connection(&ports)).unwrap();
    assert!(!connector.orders().unwrap().supports_creation());
    assert!(connector.order_confirmation().is_none());
    assert!(connector.cancellations().is_some());
}

#[test]
fn generic_callback_registry_uses_connection_protocol_and_actual_capability() {
    let config = connector_config(&format!(
        "[dangaoshushu]\nenabled=true\nchannel_no='test-channel'\nprivate_key='test-key'\n{}",
        jd_settings()
    ));
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(Arc::new(Tokens::default()))).unwrap();
    let registry = ports.order_connectors.as_ref().unwrap();
    let mut connection = bound_connection(&ports);
    assert!(registry.callback_connector(&connection, SupplierConnectorType::JdVop).is_err());
    assert!(registry.callback_connector(&connection, SupplierConnectorType::Dangaoshushu).is_err());
    connection.connector_type = Some(SupplierConnectorType::Dangaoshushu);
    let refs = ports
        .supplier_reference_registry
        .deployment_config(&SupplierReferenceTarget::from(&connection))
        .unwrap();
    connection.endpoint_reference = refs.endpoint;
    connection.credential_reference = Some(refs.credential);
    assert!(
        registry
            .callback_connector(&connection, SupplierConnectorType::Dangaoshushu)
            .unwrap()
            .callbacks()
            .is_some()
    );
    assert_eq!(
        registry.connector_types(),
        vec![SupplierConnectorType::Dangaoshushu, SupplierConnectorType::JdVop]
    );
}

#[test]
fn notification_binding_is_independent_of_order_funds_admission() {
    let config = connector_config(&format!(
        "{}\n{}",
        execution_settings(),
        r#"
[jd_vop.messages]
evidence_reference='offline-message-fixture'
account_scope='test-account'
app_key='test-app'
types=[1,2]
"#
    ));
    let ports = ExternalConnectorPorts::with_jd_tokens(&config, Some(Arc::new(Tokens::default()))).unwrap();
    let mut connection = bound_connection(&ports);
    connection.supplier_deposit_external_account_ref = None;
    let registry = ports.order_connectors.as_ref().unwrap();
    assert!(registry.notification_connector(&connection).unwrap().messages().is_some());
    assert_eq!(
        registry.fulfillment_connector(&connection).err().unwrap().code,
        "JD_VOP_FUNDS_ACCOUNT_MISMATCH"
    );
}
