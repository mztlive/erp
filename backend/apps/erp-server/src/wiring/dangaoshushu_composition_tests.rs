//! 真实配置绑定、协议解析和回调编排的离线组合；仅通信与持久化使用替身。

use std::collections::VecDeque;
use std::sync::{Arc, Mutex};

use async_trait::async_trait;
use config::DangaoshushuConfig;
use erp_connector_dangaoshushu::DangaoshushuRuntime;
use erp_connector_dangaoshushu::transport::{Request, Transport};
use erp_core::common::time::Instant;
use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_party::SensitiveDataCodec;
use erp_supplier_catalog::supplier_catalog_callbacks::{CatalogUpdate, query_current};
use erp_supplier_catalog::{Error as CatalogError, Result as CatalogResult};
use erp_supplier_orders::supplier_callbacks::{CallbackReceiptStore, receive_from};
use erp_supplier_orders::{Error as IntakeError, Result as IntakeResult};
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::{
    SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
};
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SupplierCatalogItem};
use erp_supply::ports::connector::callback::{CallbackReply, CallbackRequest};
use erp_supply::ports::connector::common::{ConnectorResult, SupplierSku};
use erp_supply::ports::supplier_reference_registry::{SupplierReferenceRegistry, SupplierReferenceTarget};
use mongodb::error::Error as MongoError;
use persistence_core::{Error as StoreError, Result as StoreResult};
use serde_json::{Value, json};

type ReadRequest = (&'static str, Vec<(String, String)>);

#[derive(Default)]
struct ProtocolTransport {
    replies: Mutex<VecDeque<Value>>,
    reads: Mutex<Vec<ReadRequest>>,
}

#[async_trait]
impl Transport for ProtocolTransport {
    async fn send(&self, request: Request) -> ConnectorResult<Value> {
        let Request::Read { path, parameters } = request else {
            panic!("回调补查不得发送外部写入");
        };
        self.reads.lock().unwrap().push((path, parameters));
        Ok(self.replies.lock().unwrap().pop_front().expect("每次补查必须有唯一响应"))
    }
}

struct ReceiptStore {
    connection: SupplierApiConnection,
    receipt: Mutex<Option<SupplierCallbackReceipt>>,
    calls: Mutex<Vec<&'static str>>,
    unknown_write: bool,
}

#[async_trait]
impl CallbackReceiptStore for ReceiptStore {
    async fn connection(&self, id: &str) -> IntakeResult<SupplierApiConnection> {
        self.calls.lock().unwrap().push("connection");
        assert_eq!(id, self.connection.base.id);
        Ok(self.connection.clone())
    }

    async fn create(&self, receipt: &SupplierCallbackReceipt) -> StoreResult<()> {
        self.calls.lock().unwrap().push("create");
        if self.unknown_write {
            return Err(StoreError::CommitOutcomeUnknown(MongoError::custom("original callback write")));
        }
        let mut stored = self.receipt.lock().unwrap();
        if stored.is_some() {
            return Err(StoreError::DuplicateKey(MongoError::custom("same callback identity")));
        }
        *stored = Some(receipt.clone());
        Ok(())
    }

    async fn find(&self, id: &str) -> StoreResult<Option<SupplierCallbackReceipt>> {
        self.calls.lock().unwrap().push("find");
        let stored = self.receipt.lock().unwrap();
        assert_eq!(stored.as_ref().unwrap().base.id, id);
        Ok(stored.clone())
    }
}

struct Fixture {
    registry: SupplierConnectorRegistry,
    store: ReceiptStore,
    codec: SensitiveDataCodec,
    transport: Arc<ProtocolTransport>,
}

impl Fixture {
    fn new(replies: Vec<Value>) -> Self {
        let transport =
            Arc::new(ProtocolTransport { replies: Mutex::new(replies.into()), ..Default::default() });
        let settings: DangaoshushuConfig = serde_json::from_value(json!({
            "enabled": true, "channel_no": "test-channel", "private_key": "test-key",
            "user_id": "user-1", "spec_units": {"spec-1": "个"}, "city_regions": {"2": "110100"}
        }))
        .unwrap();
        let runtime = Arc::new(DangaoshushuRuntime::with_transport(settings, transport.clone()).unwrap());
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "dgss".into(),
                connector_type: SupplierConnectorType::Dangaoshushu,
                endpoint_reference: "unbound".into(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "actor",
        )
        .unwrap();
        let references = runtime.deployment_config(&SupplierReferenceTarget::from(&connection)).unwrap();
        connection.apply_deployment(&references, "actor").unwrap();
        let mut registry = SupplierConnectorRegistry::default();
        registry.register(SupplierConnectorType::Dangaoshushu, runtime).unwrap();
        Self {
            registry,
            store: ReceiptStore {
                connection,
                receipt: Mutex::new(None),
                calls: Mutex::new(vec![]),
                unknown_write: false,
            },
            codec: SensitiveDataCodec::from_secret(b"offline-callback-composition"),
            transport,
        }
    }

    async fn receive(&self, route: &str, body: &[u8]) -> IntakeResult<CallbackReply> {
        let path = format!("/callbacks/dangaoshushu/connection-1/{route}?original=query");
        receive_from(
            &self.store,
            &self.registry,
            &self.codec,
            SupplierConnectorType::Dangaoshushu,
            "connection-1",
            &CallbackRequest {
                method: "POST",
                path_and_query: &path,
                headers: &[],
                body,
                received_at: Instant::from_unix_secs(1700000000),
            },
        )
        .await
    }

    fn receipt(&self) -> SupplierCallbackReceipt {
        self.store.receipt.lock().unwrap().as_ref().unwrap().clone()
    }

    async fn query(&self, receipt: &SupplierCallbackReceipt) -> CatalogResult<CatalogUpdate> {
        let connector = self.registry.notification_connector(&self.store.connection).unwrap();
        query_current(
            &self.store.connection,
            connector.as_ref(),
            receipt,
            SupplierSku { product_id: Some("product-1".into()), spec_id: "spec-1".into() },
            &self.registry,
        )
        .await
    }

    fn assert_read(&self, path: &str) {
        assert_eq!(
            *self.transport.reads.lock().unwrap(),
            [(path, vec![("product_id".to_string(), "product-1".to_string())])]
        );
        assert!(self.transport.replies.lock().unwrap().is_empty());
    }
}

fn body(route: &str) -> Vec<u8> {
    // 沿用迁移前的固定签名向量；业务正文仅作为补查线索。
    let mut value = json!({
        "channel_no": "test-channel", "timestamp": 1700000000,
        "sign": "b918d6992b799b477691f8beebc45c8f"
    });
    match route {
        "order" => value["out_order_no"] = json!("order-1"),
        "price" => {
            value["data"] = json!([{
                "product_id": "product-1", "spec_id": "spec-1", "clearing_price": "0.01"
            }])
        },
        _ => {
            value["id"] = json!("product-1");
            value["type"] = json!("product");
            value["can_buy"] = json!("1");
        },
    }
    serde_json::to_vec(&value).unwrap()
}

fn product() -> Value {
    json!({
        "brand": {"status": "1"},
        "product": {"id": "product-1", "title": "查询得到的新名称", "status": "1", "can_buy": "1"},
        "specs": [{"id": "spec-1", "name": "6寸", "clearing_price": "123.45",
            "deleted": "0", "can_buy": "1", "stock": "5"}]
    })
}

#[tokio::test]
async fn original_order_callback_preserves_binding_identity_evidence_and_ack_after_save() {
    let fixture = Fixture::new(vec![]);
    let bytes = body("order");
    let reply = fixture.receive("order", &bytes).await.unwrap();
    let first = fixture.receipt();
    assert_eq!(*fixture.store.calls.lock().unwrap(), ["connection", "create"]);
    assert_eq!(reply.status, 200);
    assert_eq!(reply.content_type, "application/json");
    assert_eq!(reply.body, br#"{"code":200,"message":"success"}"#);
    assert_eq!(first.connection_id.as_ref(), "connection-1");
    assert_eq!(first.route_kind, "order");
    assert_eq!(first.integrity, "envelope_only");
    assert!(first.source_event_id.is_none());
    assert_eq!(first.refresh_hints.len(), 1);
    assert_eq!(first.refresh_hints[0].target_kind, "merchant_order");
    assert_eq!(first.refresh_hints[0].object_id, "order-1");
    assert_eq!(fixture.codec.decrypt(&first.body_encrypted).unwrap().as_bytes(), bytes);
    assert!(!first.body_encrypted.contains("order-1"));

    let repeated = fixture.receive("order", &bytes).await.unwrap();
    assert_eq!(repeated.body, reply.body);
    let retained = fixture.receipt();
    assert_eq!(retained.base.id, first.base.id);
    assert_eq!(retained.body_encrypted, first.body_encrypted);
    assert_eq!(retained.content_fingerprint, first.content_fingerprint);
    assert_eq!(
        *fixture.store.calls.lock().unwrap(),
        ["connection", "create", "connection", "create", "find"]
    );
    assert!(fixture.transport.reads.lock().unwrap().is_empty());
}

#[tokio::test]
async fn callback_rejects_signature_binding_write_and_replay_failures_without_success_ack() {
    let mut fixture = Fixture::new(vec![]);
    let mut invalid: Value = serde_json::from_slice(&body("order")).unwrap();
    invalid["sign"] = json!("00000000000000000000000000000000");
    assert!(matches!(
        fixture.receive("order", &serde_json::to_vec(&invalid).unwrap()).await,
        Err(IntakeError::Unauthenticated(_))
    ));
    assert_eq!(*fixture.store.calls.lock().unwrap(), ["connection"]);
    assert!(fixture.store.receipt.lock().unwrap().is_none());

    fixture.store.unknown_write = true;
    let error = fixture.receive("order", &body("order")).await.err().unwrap();
    let IntakeError::OutcomeUnknown(StoreError::CommitOutcomeUnknown(source)) = error else {
        panic!("必须保留原写入未知错误");
    };
    assert_eq!(source.get_custom::<&str>(), Some(&"original callback write"));
    assert_eq!(*fixture.store.calls.lock().unwrap(), ["connection", "connection", "create"]);
    fixture.store.unknown_write = false;
    fixture.receive("order", &body("order")).await.unwrap();
    fixture.store.receipt.lock().unwrap().as_mut().unwrap().content_fingerprint = Some("different".into());
    assert!(matches!(fixture.receive("order", &body("order")).await, Err(IntakeError::ConflictError(_))));
    assert_eq!(fixture.receipt().content_fingerprint.as_deref(), Some("different"));

    let writes = fixture.store.calls.lock().unwrap().iter().filter(|step| **step == "create").count();
    fixture.store.connection.credential_reference = Some("another-connection-reference".into());
    assert!(matches!(fixture.receive("order", &body("order")).await, Err(IntakeError::Unauthenticated(_))));
    assert_eq!(fixture.store.calls.lock().unwrap().iter().filter(|step| **step == "create").count(), writes);
    assert!(fixture.transport.reads.lock().unwrap().is_empty());
}

#[tokio::test]
async fn catalog_callback_uses_real_protocol_query_for_price_attributes_and_parent_availability() {
    for route in ["product", "price", "status"] {
        let mut response = product();
        if route == "status" {
            response["product"]["can_buy"] = json!("0");
        }
        let fixture = Fixture::new(vec![response]);
        fixture.receive(route, &body(route)).await.unwrap();
        let receipt = fixture.receipt();
        let CatalogUpdate::Offer(item) = fixture.query(&receipt).await.unwrap() else {
            panic!("必须补查商品事实");
        };
        assert_eq!(item.base.id, SupplierCatalogItem::source_id("connection-1", "product-1", "spec-1"));
        assert_eq!(item.connection_id, "connection-1");
        assert_eq!(item.supplier_id, "supplier-1");
        assert_eq!(item.name, "查询得到的新名称");
        assert_eq!(item.specification, "6寸");
        assert_eq!(item.purchase_price.as_deref(), Some("123.45"));
        assert_eq!(item.available_quantity.as_deref(), Some("5"));
        assert_eq!(
            item.availability,
            if route == "status" { CatalogAvailability::Unavailable } else { CatalogAvailability::Available }
        );
        assert_eq!(item.job_id, receipt.base.id);
        fixture.assert_read("/dsapi/product/get_product_details");
    }
}

#[tokio::test]
async fn catalog_protocol_and_orchestration_reject_incomplete_or_mismatched_observations() {
    for (route, mutation) in [("price", 0), ("status", 1), ("product", 2), ("product", 3), ("price", 4)] {
        let mut response = product();
        match mutation {
            0 => response["specs"][0]["clearing_price"] = Value::Null,
            1 => response["specs"][0]["can_buy"] = Value::Null,
            2 => response["product"]["id"] = json!("another-product"),
            3 => response["specs"] = json!([]),
            _ => {},
        }
        let fixture = Fixture::new(vec![response]);
        fixture.receive(route, &body(route)).await.unwrap();
        let mut receipt = fixture.receipt();
        if mutation == 4 {
            receipt.received_at = Instant::from_unix_secs(Instant::now().unix_secs() + 3600);
        }
        let error = fixture.query(&receipt).await.err().expect("不完整观察必须拒绝");
        match mutation {
            0 | 1 => assert!(
                matches!(error, CatalogError::BusinessLogicError(message) if message == "商品补查未取得明确可售状态或采购报价")
            ),
            2 => assert!(
                matches!(error, CatalogError::BusinessLogicError(message) if message == "商品详情 product.id 与请求商品编号不一致")
            ),
            3 => assert!(
                matches!(error, CatalogError::BusinessLogicError(message) if message == "商品或规格暂不可见，请核实原来源；不按缺项下架")
            ),
            4 => assert!(
                matches!(error, CatalogError::ConflictError(message) if message == "商品补查身份或观察时间不一致")
            ),
            _ => unreachable!(),
        }
        assert_eq!(fixture.receipt().catalog_progress, receipt.catalog_progress);
        fixture.assert_read("/dsapi/product/get_product_details");
    }
}

#[tokio::test]
async fn city_callback_preserves_explicit_empty_regions_and_rejects_partial_protocol_mapping() {
    for (response, expected) in [(json!([]), vec![]), (json!([{"city_id": "2"}]), vec!["110100".to_string()])]
    {
        let fixture = Fixture::new(vec![response]);
        fixture.receive("cities", &body("cities")).await.unwrap();
        let CatalogUpdate::Regions { id, regions } = fixture.query(&fixture.receipt()).await.unwrap() else {
            panic!("必须补查地区事实");
        };
        assert_eq!(id, SupplierCatalogItem::source_id("connection-1", "product-1", "spec-1"));
        assert_eq!(regions.included, expected);
        fixture.assert_read("/dsapi/product/get_product_cities_info");
    }
    let fixture = Fixture::new(vec![json!([{"city_id": "2"}, {"city_id": "unmapped"}])]);
    fixture.receive("cities", &body("cities")).await.unwrap();
    assert!(
        matches!(fixture.query(&fixture.receipt()).await, Err(CatalogError::BusinessLogicError(message)) if message == "供应商城市尚未绑定公司标准地区")
    );
    fixture.assert_read("/dsapi/product/get_product_cities_info");
}
