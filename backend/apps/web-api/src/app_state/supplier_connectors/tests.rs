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
