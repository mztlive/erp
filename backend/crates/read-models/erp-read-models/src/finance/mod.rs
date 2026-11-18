//! 财务读模型：组合公开财务事实与外部展示、审批上下文。

pub mod document_files;
pub mod dto;
pub mod payable;
pub mod payment_request;
pub(crate) mod purchase_payments;

pub mod receivable;

mod search;

pub mod actual_profit_loss;

pub mod cost;
pub mod extra_cost;
pub mod funds_scope;
pub mod supplier_deposits;

use persistence_core::Executor;

use crate::{Error, Result};

/// 命令内完整投影必须沿用调用方已经完成授权的活动快照。
fn command_snapshot(executor: &mut dyn Executor) -> Result<()> {
    if executor.boundary().is_some_and(|boundary| boundary.is_snapshot() && boundary.is_current(executor)) {
        Ok(())
    } else {
        Err(Error::ValidationError(
            "暂时无法查看本次财务处理结果，请打开原单据核对当前状态，勿重复操作".into(),
        ))
    }
}

#[cfg(test)]
mod command_tests {
    use mongodb::Client;
    use persistence_core::ReadExecutor;

    use super::payable::PayableReadService;
    use super::receivable::ReceivableReadService;
    use super::*;

    #[tokio::test]
    async fn full_command_projections_reject_non_snapshot_before_any_business_read() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("finance_command_snapshot_unit");
        let payable = PayableReadService::new(db.clone());
        let receivable = ReceivableReadService::new(db);
        let mut executor = ReadExecutor::default();

        for result in [
            payable.payable_account_view("account".into(), true, &mut executor).await.map(|_| ()),
            payable.supplier_payment_detail_with_executor("payment", &mut executor).await.map(|_| ()),
            receivable.receivable_account_view("account".into(), &mut executor).await.map(|_| ()),
            receivable.customer_receipt_view_with_executor("receipt", &mut executor).await.map(|_| ()),
            receivable.invoice_request_detail_with_executor("request", &mut executor).await.map(|_| ()),
        ] {
            assert!(
                matches!(result, Err(Error::ValidationError(message)) if message == "暂时无法查看本次财务处理结果，请打开原单据核对当前状态，勿重复操作")
            );
        }
    }
}
