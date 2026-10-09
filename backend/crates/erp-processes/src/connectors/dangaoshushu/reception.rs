use std::future::Future;

use erp_core::ids::SupplierApiConnectionId;
use erp_integration::entity::supplier_callback::{SupplierCallbackReceipt, SupplierRefreshHint};
use erp_integration::repository::SupplierCallbackExt;
use erp_party::SensitiveDataCodec;
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::callback::{
    CallbackReply, CallbackRequest, Callbacks, ChangeNotice, ChangeTopic, RefreshTarget,
};
use erp_supply::ports::connector::order::OrderKey;
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::{Error as PersistenceError, NoTransaction};

use super::{DangaoshushuConnector, DangaoshushuRuntime, proof};
use crate::{Error, Result};

/// 接收已绑定连接的推送；保存原始密文及补查意图后才返回供应商成功应答。
/// # 参数
/// `db` 为应用数据库，`runtime` 为配置运行时，`codec` 为应用加密器，`request` 为原始报文。
/// # 返回
/// 已可靠持久化接收记录时返回应答。
/// # 错误
/// 连接不匹配、验签失败、加密或持久化失败时不返回成功应答。
pub async fn receive(
    db: &Database,
    runtime: &DangaoshushuRuntime,
    codec: &SensitiveDataCodec,
    connection_id: &str,
    request: &CallbackRequest<'_>,
) -> Result<CallbackReply> {
    let connection =
        SupplierApiService::new(db.clone()).load_connection(connection_id, &mut NoTransaction).await?;
    runtime.validate_binding(&connection, true).map_err(|failure| Error::Unauthenticated(failure.summary))?;
    let connector = runtime.connector();
    let verified = connector.verify(request).map_err(|failure| {
        if failure.class == SupplierFailureClass::AuthSignature {
            Error::Unauthenticated(failure.summary)
        } else {
            Error::ValidationError(failure.summary)
        }
    })?;
    let plaintext =
        std::str::from_utf8(request.body).map_err(|_| Error::ValidationError("推送正文不是 UTF-8".into()))?;
    let body_encrypted = codec.encrypt(plaintext)?;
    let route_kind = request
        .path_and_query
        .split('?')
        .next()
        .and_then(|path| path.rsplit('/').next())
        .ok_or_else(|| Error::ValidationError("推送类型无效".into()))?;
    let hints = verified.notices.iter().map(hint).collect::<Result<Vec<_>>>()?;
    let receipt = SupplierCallbackReceipt::received(
        receipt_id(&connector, request),
        SupplierApiConnectionId::new(connection_id),
        request.received_at,
        route_kind.into(),
        body_encrypted,
        hints,
    )?;
    acknowledge_after_persistence(verified.durable_ack, persist_receipt(db, &receipt)).await
}

async fn acknowledge_after_persistence(
    reply: CallbackReply,
    persisted: impl Future<Output = Result<()>>,
) -> Result<CallbackReply> {
    persisted.await?;
    Ok(reply)
}

fn receipt_id(connector: &DangaoshushuConnector, request: &CallbackRequest<'_>) -> String {
    let body = [connector.binding.as_bytes(), b"\0", request.path_and_query.as_bytes(), b"\0", request.body]
        .concat();
    format!("dgss-{}", proof::hash(&connector.settings.private_key, &body))
}

async fn persist_receipt(db: &Database, receipt: &SupplierCallbackReceipt) -> Result<()> {
    let repository = db.supplier_callback_receipts();
    match repository.create(receipt, &mut NoTransaction).await {
        Ok(()) => Ok(()),
        Err(PersistenceError::DuplicateKey(_)) => {
            // 本地字节级去重只确认已保存接收证据，不制造供应商事件身份或业务事实。
            let existing = repository.find_by_id(&receipt.base.id, &mut NoTransaction).await?;
            if existing.is_some_and(|existing| existing.connection_id == receipt.connection_id) {
                Ok(())
            } else {
                Err(Error::ConflictError("推送接收去重证据无法核实".into()))
            }
        },
        Err(error) => Err(error.into()),
    }
}

fn hint(notice: &ChangeNotice) -> Result<SupplierRefreshHint> {
    let topic = match notice.topic {
        ChangeTopic::Product => "product",
        ChangeTopic::Availability => "availability",
        ChangeTopic::Price => "price",
        ChangeTopic::Region => "region",
        ChangeTopic::Order => "order",
        ChangeTopic::Refund => "refund",
    };
    let (target_kind, object_id, product_id) = match &notice.target {
        RefreshTarget::Brand(id) => ("brand", id.clone(), None),
        RefreshTarget::Product(id) => ("product", id.clone(), None),
        RefreshTarget::Sku(sku) => ("sku", sku.spec_id.clone(), sku.product_id.clone()),
        RefreshTarget::Order(OrderKey::Merchant(id)) => ("merchant_order", id.clone(), None),
        RefreshTarget::Order(OrderKey::External(id)) => ("external_order", id.clone(), None),
        RefreshTarget::Refund { .. } => {
            return Err(Error::ValidationError("不接受缺少单笔身份的退款事实".into()));
        },
    };
    Ok(SupplierRefreshHint { topic: topic.into(), target_kind: target_kind.into(), object_id, product_id })
}

#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_supply::ports::connector::common::SourceRevision;

    use super::super::test_support::connector;
    #[tokio::test]
    async fn successful_ack_requires_successful_durable_persistence() {
        let reply = || CallbackReply {
            status: 200,
            content_type: "application/json".into(),
            body: br#"{"code":200,"message":"success"}"#.to_vec(),
        };
        assert!(
            acknowledge_after_persistence(reply(), async {
                Err(Error::ConflictError("write failed".into()))
            })
            .await
            .is_err()
        );
        let mut persisted = false;
        let result = acknowledge_after_persistence(reply(), async {
            persisted = true;
            Ok(())
        })
        .await
        .unwrap();
        assert!(persisted);
        assert_eq!(result.status, 200);
    }
    use super::*;
    #[test]
    fn byte_identical_replay_uses_local_identity_without_collapsing_other_events() {
        let connector = connector(vec![]);
        let mut request = CallbackRequest {
            method: "POST",
            path_and_query: "/callbacks/dangaoshushu/connection-1/order",
            headers: &[],
            body: b"original",
            received_at: Instant::now(),
        };
        let original = receipt_id(&connector, &request);
        assert_eq!(receipt_id(&connector, &request), original);
        request.body = b"changed";
        assert_ne!(receipt_id(&connector, &request), original);
        request.body = b"original";
        request.path_and_query = "/callbacks/dangaoshushu/connection-1/price";
        assert_ne!(receipt_id(&connector, &request), original);
    }
    #[test]
    fn hints_carry_refresh_identity_without_claiming_business_completion() {
        let notice = ChangeNotice {
            source_event_id: None,
            revision: SourceRevision::Unversioned,
            occurred_at: None,
            target: RefreshTarget::Order(OrderKey::Merchant("order-1".into())),
            topic: ChangeTopic::Order,
            observation: None,
        };
        let hint = hint(&notice).unwrap();
        assert_eq!((hint.target_kind.as_str(), hint.object_id.as_str()), ("merchant_order", "order-1"));
        assert_eq!(hint.topic, "order");
    }
}
