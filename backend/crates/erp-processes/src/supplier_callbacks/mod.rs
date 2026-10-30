//! 按已绑定协议验签、可靠接收刷新线索；应答必须晚于证据保存。
mod intake;
#[cfg(test)]
mod tests;

use std::future::Future;

use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_integration::repository::SupplierCallbackExt;
use erp_party::SensitiveDataCodec;
use erp_supply::entity::supplier_api::SupplierConnectorType;
use erp_supply::ports::connector::callback::{CallbackReply, CallbackRequest};
use erp_supply::service::supplier_api::SupplierApiService;
use mongodb::Database;
use persistence_core::{Error as PersistenceError, NoTransaction};

use crate::connectors::registry::SupplierConnectorRegistry;
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
    let connection = SupplierApiService::new(db.clone())
        .load_connection(connection_id, &mut NoTransaction).await?;
    let connector = registry.callback_connector(&connection, kind)
        .map_err(|failure| Error::Unauthenticated(failure.summary))?;
    let callbacks = connector.callbacks()
        .ok_or_else(|| Error::ValidationError("供应商未提供回调能力".into()))?;
    let scope = format!("{}\0{}", connection.endpoint_reference,
        connection.credential_reference.as_deref().unwrap_or_default());
    let (receipt, reply) = intake::prepare(callbacks, connector.connection_id(), &scope, codec, request)?;
    acknowledge_after_persistence(reply, persist_receipt(db, codec, &receipt)).await
}

async fn acknowledge_after_persistence(
    reply: CallbackReply,
    persisted: impl Future<Output = Result<()>>,
) -> Result<CallbackReply> {
    persisted.await?;
    Ok(reply)
}

async fn persist_receipt(
    db: &Database,
    codec: &SensitiveDataCodec,
    receipt: &SupplierCallbackReceipt,
) -> Result<()> {
    let repository = db.supplier_callback_receipts();
    match repository.create(receipt, &mut NoTransaction).await {
        Ok(()) => Ok(()),
        Err(PersistenceError::DuplicateKey(_)) => {
            let existing = repository.find_by_id(&receipt.base.id, &mut NoTransaction).await?
                .ok_or_else(|| Error::ConflictError("推送接收去重证据无法核实".into()))?;
            intake::verify_replay(codec, &existing, receipt)
        },
        Err(error) => Err(error.into()),
    }
}
