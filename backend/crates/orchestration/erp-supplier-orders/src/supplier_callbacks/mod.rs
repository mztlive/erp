//! 按已绑定协议验签、可靠接收刷新线索；应答必须晚于证据保存。
mod intake;
mod store;
#[cfg(test)]
mod tests;

use std::future::Future;

use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_party::SensitiveDataCodec;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierConnectorType;
use erp_supply::ports::connector::callback::{CallbackReply, CallbackRequest};
use mongodb::Database;
use persistence_core::Error as PersistenceError;
pub use store::CallbackReceiptStore;
use store::MongoCallbackReceipts;

use crate::{Error, Result};

/// 按服务端连接绑定选择协议并持久化回调。
/// # 参数
/// 数据库、注册表、加密器来自组合根；kind 仅用于核对路由与连接协议。
/// # 返回
/// 可靠保存证据后返回供应商协议应答。
/// # 错误
/// 协议不符、能力缺失、验签或持久化失败时拒绝成功应答。
pub async fn receive(
    db: &Database,
    registry: &SupplierConnectorRegistry,
    codec: &SensitiveDataCodec,
    kind: SupplierConnectorType,
    connection_id: &str,
    request: &CallbackRequest<'_>,
) -> Result<CallbackReply> {
    receive_from(&MongoCallbackReceipts(db), registry, codec, kind, connection_id, request).await
}

/// 使用持久化边界执行完整回调接收，保留协议绑定、验签、去重和保存后应答顺序。
/// # 参数
/// `store` 提供服务端连接与可靠回执；其余参数与 HTTP 接收入口一致。
/// # 返回
/// 首次回执已保存或原回执证据已核实后返回原协议应答。
/// # 错误
/// 绑定、验签、保存或重放证据核验失败时拒绝成功应答。
pub async fn receive_from(
    store: &dyn CallbackReceiptStore,
    registry: &SupplierConnectorRegistry,
    codec: &SensitiveDataCodec,
    kind: SupplierConnectorType,
    connection_id: &str,
    request: &CallbackRequest<'_>,
) -> Result<CallbackReply> {
    let connection = store.connection(connection_id).await?;
    let connector = registry
        .callback_connector(&connection, kind)
        .map_err(|failure| Error::Unauthenticated(failure.summary))?;
    let callbacks =
        connector.callbacks().ok_or_else(|| Error::ValidationError("供应商未提供回调能力".into()))?;

    let scope = format!(
        "{}\0{}",
        connection.endpoint_reference,
        connection.credential_reference.as_deref().unwrap_or_default()
    );
    let (receipt, reply) = intake::prepare(callbacks, connector.connection_id(), &scope, codec, request)?;

    acknowledge_after_persistence(reply, persist_receipt(store, codec, &receipt)).await
}

async fn acknowledge_after_persistence(
    reply: CallbackReply,
    persisted: impl Future<Output = Result<()>>,
) -> Result<CallbackReply> {
    persisted.await?;
    Ok(reply)
}

async fn persist_receipt(
    store: &dyn CallbackReceiptStore,
    codec: &SensitiveDataCodec,
    receipt: &SupplierCallbackReceipt,
) -> Result<()> {
    match store.create(receipt).await {
        Ok(()) => Ok(()),
        Err(PersistenceError::DuplicateKey(_)) => {
            let existing = store
                .find(&receipt.base.id)
                .await?
                .ok_or_else(|| Error::ConflictError("推送接收去重证据无法核实".into()))?;
            intake::verify_replay(codec, &existing, receipt)
        },
        Err(error) => Err(error.into()),
    }
}
