#![cfg(test)]

use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use erp_core::AccountKind;
use erp_identity::testing::unavailable_authorization;
use mongodb::Client;
use mongodb::options::{ClientOptions, ServerAddress};
use persistence_core::NoTransaction;

use super::*;
use crate::ports::{FailClosedAuditPort, FailClosedFingerprintPort, HandlerIdentityFact, IdentityFactPort};

#[derive(Default)]
struct Identities(Mutex<Vec<(String, usize)>>);

#[async_trait]
impl IdentityFactPort for Identities {
    async fn handler_identity(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<HandlerIdentityFact>> {
        self.0.lock().unwrap().push((id.into(), executor as *mut dyn Executor as *mut () as usize));
        Ok(Some(HandlerIdentityFact {
            user_id: id.into(),
            display_name: id.into(),
            account: id.into(),
            can_login: true,
            inbound_eligible: true,
            outbound_eligible: true,
        }))
    }

    async fn admin_handler_identities(&self, _: &mut dyn Executor) -> Result<Vec<HandlerIdentityFact>> {
        panic!("写入资格不得扫描全部账号")
    }
}

fn service(identity: Arc<dyn IdentityFactPort>) -> WarehouseService {
    let options = ClientOptions::builder()
        .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
        .build();
    WarehouseService::new(
        Client::with_options(options).unwrap().database("warehouse_command_unit"),
        identity,
        Arc::new(FailClosedAuditPort),
        Arc::new(FailClosedFingerprintPort),
        unavailable_authorization(),
    )
}

fn request() -> serde_json::Value {
    serde_json::json!({
        "warehouse_code": "WH-1", "version": 1, "name": "仓库", "region_code": "110105",
        "address": "地址", "contact": "联系人", "effective_from": "2026-11-15",
        "effective_to": null, "change_reason": "更新", "status": "active",
        "inbound_handler_user_id": "inbound", "outbound_handler_user_id": "outbound"
    })
}

#[tokio::test]
async fn writes_reject_non_snapshot_before_identity_object_version_and_overlap_reads() {
    let identities = Arc::new(Identities::default());
    let service = service(identities.clone());
    let actor = AuditActor::new("actor".into(), "login".into(), AccountKind::Admin);
    let mut executor = NoTransaction;
    let results = [
        service.create(&actor, serde_json::from_value(request()).unwrap(), &mut executor).await.map(|_| ()),
        service
            .update(&actor, "warehouse", serde_json::from_value(request()).unwrap(), &mut executor)
            .await
            .map(|_| ()),
        service
            .update_handlers(&actor, "warehouse", serde_json::from_value(request()).unwrap(), &mut executor)
            .await
            .map(|_| ()),
        service
            .update_policy(
                &actor,
                "policy",
                UpdateWarehouseSkuPolicyRequest {
                    version: 1,
                    minimum_available_quantity: None,
                    status: Some(EnableStatus::Active),
                },
                &mut executor,
            )
            .await
            .map(|_| ()),
        service.delete_policy(&actor, "policy", &mut executor).await,
    ];
    for result in results {
        assert!(
            matches!(result, Err(Error::ValidationError(message)) if message == "写入授权必须使用快照事务")
        );
    }
    assert!(identities.0.lock().unwrap().is_empty());
}

#[tokio::test]
async fn both_handler_checks_use_the_original_executor() {
    let identities = Arc::new(Identities::default());
    let service = service(identities.clone());
    let mut executor = NoTransaction;
    let expected = &mut executor as *mut NoTransaction as usize;
    service.ensure_handlers("inbound", "outbound", &mut executor).await.unwrap();
    assert_eq!(*identities.0.lock().unwrap(), [("inbound".into(), expected), ("outbound".into(), expected)]);
}
