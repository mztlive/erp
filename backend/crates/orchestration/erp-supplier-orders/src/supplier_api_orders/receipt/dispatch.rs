//! 真实单次发送与只读恢复共用的最窄编排边界，数据库意图提交先于 HTTP。

use async_trait::async_trait;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::OrderReference;
use erp_supply::ports::connector::receipt::{
    ConfirmReceipt, OrderReceiptConfirmation, ReceiptConfirmationState,
};

use crate::Result;
use crate::supplier_api_orders::SupplierApiOrderProcess;

#[async_trait]
pub(super) trait ReceiptDispatchStore: Sync {
    async fn claim(
        &self,
        expected: &ApiOrderExecution,
        order: &OrderReference,
    ) -> Result<(ApiOrderExecution, ConfirmReceipt)>;
    async fn save(&self, execution: &mut ApiOrderExecution) -> Result<()>;
}

#[async_trait]
impl ReceiptDispatchStore for SupplierApiOrderProcess {
    async fn claim(
        &self,
        expected: &ApiOrderExecution,
        order: &OrderReference,
    ) -> Result<(ApiOrderExecution, ConfirmReceipt)> {
        self.claim_receipt(expected, order).await
    }
    async fn save(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.save_receipt_progress(execution).await
    }
}

pub(super) async fn send(
    store: &impl ReceiptDispatchStore,
    expected: &ApiOrderExecution,
    order: &OrderReference,
    port: &dyn OrderReceiptConfirmation,
) -> Result<()> {
    let (mut claimed, request) = store.claim(expected, order).await?;
    match port.confirm_receipt(&request).await {
        Ok(snapshot) => {
            claimed.observe_receipt_confirmation(order, request.topology_version, snapshot.value)?;
            claimed.receipt_blocker = (snapshot.value != ReceiptConfirmationState::Confirmed)
                .then(|| "原收货通知结果尚未查明，系统继续查询原结果，不重复发送".into());
        },
        Err(error) => claimed.receipt_blocker = Some(error.summary),
    }
    claimed.receipt_next_run_at = Some(super::later());
    store.save(&mut claimed).await
}

pub(super) async fn recover(
    store: &impl ReceiptDispatchStore,
    execution: &mut ApiOrderExecution,
    order: &OrderReference,
    version: u64,
    port: &dyn OrderReceiptConfirmation,
    source_error: Option<String>,
) -> Result<()> {
    let reason = match port.receipt_confirmation(order).await {
        Ok(Lookup::Found(snapshot)) if snapshot.value == ReceiptConfirmationState::Confirmed => {
            execution.observe_receipt_confirmation(order, version, snapshot.value)?;
            None
        },
        Err(error) => Some(error.summary),
        _ => Some("原收货通知未取得明确结果；未妥投或暂不可见不代表可以重发".into()),
    };
    execution.receipt_blocker = source_error
        .map(|error| format!("原收货通知可能已送达，但ERP实收依据已变更，请核对原签收或入库冲正：{error}"))
        .or(reason);
    execution.receipt_next_run_at = Some(super::later());
    store.save(execution).await
}
