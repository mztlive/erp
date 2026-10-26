//! 供应商要求的履约支付结果通知；不登记 ERP 收付款或银行交易。
use erp_core::common::time::Instant;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage};
use erp_supply::entity::supplier_fulfillment::CancelStatus;
use erp_supply::ports::connector::common::Lookup;
use erp_supply::ports::connector::order::{
    ConfirmPayment, OrderKey, OrderReference, PaymentConfirmation, PaymentState,
};
use erp_supply::repository::SupplierFulfillmentExt;
use erp_supply::service::supplier_api::SupplierApiService;
use persistence_core::NoTransaction;

use super::SupplierApiOrderProcess;
use super::steps::{freeze_action, record_failure};
use crate::{Error, Result};

impl SupplierApiOrderProcess {
    pub(super) async fn confirm_payment(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connector = self.connector(&execution).await?;
        let snapshot = self.decode(&execution)?;
        let Some(amount) = snapshot.actual_payable.filter(|amount| *amount <= snapshot.approved_total) else {
            execution
                .defer("供应商已创建订单，但未取得批准范围内的准确应付金额，已阻断履约通知，请核查原订单");
            return self.persist(&mut execution).await;
        };
        let order = self
            .db
            .supplier_fulfillment_orders()
            .find_by_id(&execution.base.id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("供应商订单不存在".into()))?;
        if order.cancel_status == CancelStatus::Canceled {
            execution.stage = ApiOrderStage::Stopped;
            execution.defer("供应商已明确取消原订单，已停止履约通知");
            return self.persist(&mut execution).await;
        }
        let reference = OrderReference {
            merchant_order_no: execution.base.id.clone(),
            external_order_no: order
                .external_order_no
                .ok_or_else(|| Error::BusinessLogicError("供应商订单号尚未确认".into()))?,
        };
        // 这是履约通知业务编号，不是银行流水或 ERP 付款凭证。
        let transaction_no = format!("{}-fulfillment-notice", execution.base.id);
        let action = freeze_action(&mut execution, "payment", &(&reference, &transaction_no, amount))?;
        let request = ConfirmPayment { action, order: reference, transaction_no, amount };
        self.claim(&mut execution, ApiOrderStage::PaymentUnknown).await?;
        match connector.confirm(&request).await {
            Ok(state) if state.value == PaymentState::Confirmed => {
                execution.stage = ApiOrderStage::Observe;
                execution.blocker = None;
                execution.next_run_at = Instant::now();
            },
            Ok(_) => execution.defer("供应商尚未确认履约通知，系统将查询原订单恢复，不重复通知"),
            Err(error) => record_failure(&mut execution, error, ApiOrderStage::ConfirmPayment),
        }
        self.persist(&mut execution).await
    }

    pub(super) async fn recover_payment(&self, mut execution: ApiOrderExecution) -> Result<()> {
        let connection = SupplierApiService::new(self.db.clone())
            .load_connection(&execution.connection_id, &mut NoTransaction)
            .await?;
        match self.runtime.observe_order(&connection, &OrderKey::Merchant(execution.base.id.clone())).await {
            Ok(Lookup::Found(snapshot)) if snapshot.value.payment == Some(PaymentState::Confirmed) => {
                execution.stage = ApiOrderStage::Observe;
                execution.blocker = None;
            },
            Ok(Lookup::Found(snapshot)) if snapshot.value.cancellation == Some(CancelStatus::Canceled) => {
                execution.stage = ApiOrderStage::Stopped;
                execution.blocker = Some("供应商已明确取消原订单，已停止履约通知".into());
            },
            Ok(_) => execution.defer("原履约通知结果尚未确认，系统保留原动作并继续查询，不重复通知"),
            Err(error) => execution.defer(error.summary),
        }
        self.persist(&mut execution).await
    }
}
