//! 官方5871拉取及5787单次删除，未知删除结果不自动重发。

use std::collections::BTreeSet;
use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use async_trait::async_trait;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::messages::{MessageAcknowledgement, PulledMessage, SupplierMessageSource};
use serde_json::value::RawValue;

use super::transport::{JdVopReadRequest, JdVopWriteRequest, WriteClient};
use super::wire::{WireId, WireText};
use super::{JdVopConnector, mapping, protocol_required};

pub(super) struct MessageRuntime {
    pub writer: Arc<WriteClient>,
    pub types: BTreeSet<u32>,
    pub source_type: u32,
    pub interval: u64,
}

impl JdVopConnector {
    pub(super) fn message_settings(&self) -> ConnectorResult<(&BTreeSet<u32>, u32, &WriteClient)> {
        if let Some(runtime) = &self.message_runtime {
            return Ok((&runtime.types, runtime.source_type, &runtime.writer));
        }
        let profile = self.profile.as_ref().ok_or_else(protocol_required)?;
        let types = profile
            .verified_message_types
            .as_ref()
            .filter(|types| !types.is_empty())
            .ok_or_else(protocol_required)?;
        Ok((types, profile.source_type, self.writer.as_deref().ok_or_else(protocol_required)?))
    }
}

#[async_trait]
impl SupplierMessageSource for JdVopConnector {
    fn poll_interval(&self) -> Duration {
        Duration::from_secs(self.message_runtime.as_ref().map_or(60, |runtime| runtime.interval))
    }

    fn account_scope(&self) -> &str {
        &self.account_scope
    }

    fn order_only_notice(&self, message_type: &str) -> bool {
        matches!(message_type, "10" | "14" | "15" | "18")
    }

    async fn pull(&self, limit: NonZeroU32) -> ConnectorResult<Vec<PulledMessage>> {
        let (types, source_type, _) = self.message_settings()?;
        if types.is_empty() || types.len() > 100 || self.account_scope.trim().is_empty() {
            return Err(mapping("京东消息订阅及消费账号尚未完整核验"));
        }
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let records: Vec<Box<RawValue>> = reader
            .read(JdVopReadRequest::Messages {
                types: types.iter().copied().collect(),
                read_type: 2,
                source_type,
            })
            .await?;
        // 官方查询没有条数参数；只接收本地批次上限，其他远端消息继续保留而不删除。
        records
            .into_iter()
            .take(usize::try_from(limit.get()).map_err(|_| mapping("消息批次上限无效"))?)
            .map(|record| super::wire_messages::pulled(record.as_ref(), types))
            .collect()
    }

    async fn acknowledge(&self, source_id: &str) -> ConnectorResult<MessageAcknowledgement> {
        let (_, source_type, writer) = self.message_settings()?;
        let id: WireId = serde_json::from_value(serde_json::Value::String(source_id.into()))
            .map_err(|_| mapping("京东来源消息号无效"))?;
        let result: WireText =
            match writer.send(JdVopWriteRequest::DeleteMessages { ids: vec![id.0], source_type }).await {
                Ok(result) => result,
                Err(failure) if failure.class == SupplierFailureClass::ResultUnknown => {
                    return Ok(MessageAcknowledgement::Unknown);
                },
                Err(failure) => return Err(failure),
            };
        match result.0.as_str() {
            "true" => Ok(MessageAcknowledgement::Confirmed),
            _ => Ok(MessageAcknowledgement::Unknown),
        }
    }
}

#[cfg(test)]
mod tests {
    use std::collections::BTreeSet;
    use std::sync::{Arc, Mutex};

    use erp_core::ids::SupplierApiConnectionId;
    use erp_supply::ports::connector::messages::MessageInterpretation;

    use super::super::transport::{JdVopReadTransport, JdVopWriteTransport, ReadClient, WriteClient};
    use super::super::{
        JdVopConfirmationFundsPolicy, JdVopExecutionProfile, JdVopInvoiceProfile, JdVopOfferMapper, error,
    };
    use super::*;

    struct Transport {
        reads: Mutex<Vec<JdVopReadRequest>>,
        writes: Mutex<Vec<Vec<String>>>,
        read_body: Vec<u8>,
        write_result: Mutex<Option<ConnectorResult<Vec<u8>>>>,
    }

    #[async_trait]
    impl JdVopReadTransport for Transport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            self.reads.lock().unwrap().push(request.clone());
            Ok(self.read_body.clone())
        }
    }

    #[async_trait]
    impl JdVopWriteTransport for Transport {
        async fn write(&self, request: &JdVopWriteRequest) -> ConnectorResult<Vec<u8>> {
            let JdVopWriteRequest::DeleteMessages { ids, source_type } = request else {
                panic!("delete expected");
            };
            assert_eq!(*source_type, 3);
            self.writes.lock().unwrap().push(ids.clone());
            self.write_result.lock().unwrap().take().expect("每次删除只允许一次传输")
        }
    }

    fn profile() -> JdVopExecutionProfile {
        JdVopExecutionProfile {
            source_type: 3,
            evidence_reference: "verified-account-fixture".into(),
            verified_external_funds_account_ref: "funds-account".into(),
            confirmation_funds_policy: Some(JdVopConfirmationFundsPolicy::OriginalOrderReservation {
                evidence_reference: "verified-original-order-funding".into(),
            }),
            preflight_validity_seconds: 60,
            ordinary_skus: BTreeSet::from(["123".into()]),
            verified_message_types: Some(BTreeSet::from([1, 2, 14])),
            verified_funds_profile: None,
            invoice: JdVopInvoiceProfile {
                invoice_type: 23,
                invoice_put_type: 2,
                invoice_selected_title: 4,
                invoice_content_type: 1,
                invoice_company_name: "测试公司".into(),
                invoice_reg_code: None,
                invoice_reg_company_name: None,
                invoice_reg_address: None,
                invoice_reg_phone: None,
                invoice_reg_bank: None,
                invoice_reg_bank_account: None,
                recipient: None,
            },
        }
    }

    fn connector(write_result: ConnectorResult<Vec<u8>>) -> (JdVopConnector, Arc<Transport>) {
        let transport = Arc::new(Transport {
            reads:Mutex::new(vec![]),writes:Mutex::new(vec![]),
            read_body:br#"{"jingdong_vop_message_queryTransByVopNormal_responce":{"openRpcResult":{"success":"true","resultCode":"success","result":[{"id":9007199254740993,"type":"2","yn":"1","clientId":"test-account","content":"{\"skuId\":9007199254740995}"},{"id":"2","type":2,"yn":1,"content":"{\"skuId\":123}"}]}}}"#.to_vec(),
            write_result:Mutex::new(Some(write_result)),
        });
        let reader = Arc::new(ReadClient::new(transport.clone(), 2, 1000, 5));
        let writer = Arc::new(WriteClient { transport: transport.clone(), reader: reader.clone() });
        (
            JdVopConnector {
                connection_id: SupplierApiConnectionId::new("connection"),
                account_scope: "verified-account".into(),
                binding: "binding".into(),
                cursor_secret: "fixture".into(),
                reader: Some(reader),
                mapper: JdVopOfferMapper::default(),
                writer: Some(writer),
                profile: Some(Arc::new(profile())),
                evidence_codec: None,
                message_runtime: None,
            },
            transport,
        )
    }

    fn deleted() -> Vec<u8> {
        br#"{"jingdong_vop_message_deleteClientMsgByIdList_responce":{"vopOrderRpcResult":{"success":"true","resultCode":"0000","result":"true"}}}"#.to_vec()
    }

    #[tokio::test]
    async fn independent_message_transport_pulls_and_acknowledges_without_order_capability() {
        use erp_supply::ports::connector::SupplierConnector;
        let (mut connector, transport) = connector(Ok(deleted()));
        connector.message_runtime = Some(Arc::new(MessageRuntime {
            writer: connector.writer.take().unwrap(),
            types: BTreeSet::from([1, 2, 14]),
            source_type: 3,
            interval: 90,
        }));
        connector.profile = None;
        assert!(connector.orders().is_none());
        assert!(connector.cancellations().is_none());
        let source = connector.messages().unwrap();
        assert_eq!(source.poll_interval(), Duration::from_secs(90));
        let records = source.pull(NonZeroU32::new(1).unwrap()).await.unwrap();
        assert_eq!(
            source.acknowledge(&records[0].source_id).await.unwrap(),
            MessageAcknowledgement::Confirmed
        );
        assert_eq!(transport.reads.lock().unwrap().len(), 1);
        assert_eq!(*transport.writes.lock().unwrap(), [vec!["9007199254740993".to_string()]]);
    }

    #[tokio::test]
    async fn official_query_and_delete_envelopes_use_verified_types_and_single_message_identity() {
        let (connector, transport) = connector(Ok(deleted()));
        assert_eq!(connector.account_scope(), "verified-account");
        let messages = connector.pull(NonZeroU32::new(1).unwrap()).await.unwrap();
        assert_eq!(messages.len(), 1);
        assert_eq!(messages[0].source_id, "9007199254740993");
        assert!(matches!(messages[0].interpretation, MessageInterpretation::Refresh(_)));
        assert_eq!(
            *transport.reads.lock().unwrap(),
            [JdVopReadRequest::Messages { types: vec![1, 2, 14], read_type: 2, source_type: 3 }]
        );
        assert_eq!(
            connector.acknowledge(&messages[0].source_id).await.unwrap(),
            MessageAcknowledgement::Confirmed
        );
        assert_eq!(*transport.writes.lock().unwrap(), [vec!["9007199254740993".to_string()]]);
    }

    #[tokio::test]
    async fn unknown_delete_and_wrong_rpc_never_hide_a_second_write() {
        let (unknown, transport) =
            connector(Err(error(SupplierFailureClass::ResultUnknown, "TIMEOUT", "unknown")));
        assert_eq!(unknown.acknowledge("123").await.unwrap(), MessageAcknowledgement::Unknown);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
        let (wrong_rpc,transport) = connector(Ok(br#"{"jingdong_vop_message_deleteClientMsgByIdList_responce":{"openRpcResult":{"success":true,"resultCode":"0000","result":true}}}"#.to_vec()));
        assert_eq!(wrong_rpc.acknowledge("123").await.unwrap(), MessageAcknowledgement::Unknown);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn false_delete_result_is_unknown_and_sends_once() {
        let (connector, transport) = connector(Ok(br#"{"jingdong_vop_message_deleteClientMsgByIdList_responce":{"vopOrderRpcResult":{"success":true,"resultCode":"0000","result":false}}}"#.to_vec()));
        assert_eq!(connector.acknowledge("123").await.unwrap(), MessageAcknowledgement::Unknown);
        assert_eq!(transport.writes.lock().unwrap().len(), 1);
    }

    #[tokio::test]
    async fn missing_verified_subscription_or_invalid_source_id_sends_nothing() {
        let (mut connector, transport) = connector(Ok(deleted()));
        for kind in ["10", "14", "15", "18"] {
            assert!(connector.order_only_notice(kind));
        }
        assert!(!connector.order_only_notice("109"));
        assert!(connector.acknowledge("1.5").await.is_err());
        connector.profile = None;
        assert!(connector.pull(NonZeroU32::new(1).unwrap()).await.is_err());
        assert!(connector.acknowledge("123").await.is_err());
        assert!(transport.reads.lock().unwrap().is_empty());
        assert!(transport.writes.lock().unwrap().is_empty());
    }
}
