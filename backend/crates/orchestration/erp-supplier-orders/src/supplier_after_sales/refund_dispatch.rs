use async_trait::async_trait;
use erp_supply::after_sales::{RefundExecution, RefundExecutionState};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{Lookup, Snapshot};
use erp_supply::ports::connector::order::{RefundProgress, Refunds};

use crate::Result;

#[async_trait]
pub(super) trait RefundStore: Send + Sync {
    async fn save(&self, execution: &mut RefundExecution) -> Result<()>;
}

/// 实际生产及替身测试共用的发送/恢复编排；持久化失败时不执行后续外部调用。
pub(super) async fn process(
    store: &dyn RefundStore,
    source: Option<&dyn Refunds>,
    row: &mut RefundExecution,
) -> Result<()> {
    if matches!(
        row.state,
        RefundExecutionState::Unsupported
            | RefundExecutionState::Rejected
            | RefundExecutionState::Received
            | RefundExecutionState::Applied
    ) {
        return Ok(());
    }
    let Some(source) = source else {
        row.unsupported();
        return store.save(row).await;
    };
    if row.begin() {
        store.save(row).await?;
        match source.request(&row.request).await {
            Ok(accepted) if accepted.action_id == row.request.action.id => {
                row.state = RefundExecutionState::Accepted;
                row.external_request_id = accepted.external_request_id;
                row.defer("退款申请已受理，等待原动作最终结果");
            },
            Ok(_) => row.defer("退款受理身份不匹配，保留原动作继续调查"),
            Err(error) if error.class == SupplierFailureClass::BusinessRejected => {
                row.state = RefundExecutionState::Rejected;
                row.blocker = Some("供应商明确拒绝本次退款申请，请人工核对原因".into());
                row.next_run_at = None;
            },
            Err(_) => row.defer("退款发送结果尚未核实，继续按原动作查询，不重发"),
        }
    } else {
        observe(source, row).await;
    }
    store.save(row).await
}

async fn observe(source: &dyn Refunds, row: &mut RefundExecution) {
    match source.refund(&row.request.order, &row.request.action.id).await {
        Ok(Lookup::Found(snapshot)) => match snapshot.value {
            RefundProgress::Completed(receipt) => {
                if row.receive(Snapshot { stamp: snapshot.stamp, value: receipt }).is_err() {
                    row.defer("退款来源与原动作不匹配，等待核实原证据");
                }
            },
            RefundProgress::Rejected { .. } => {
                row.state = RefundExecutionState::Rejected;
                row.blocker = Some("原退款动作经供应商查询明确拒绝，请人工处理".into());
                row.next_run_at = None;
            },
            RefundProgress::Pending => row.defer("原退款动作仍在处理中"),
        },
        Ok(Lookup::NotVisible) => row.defer("原退款动作暂不可见，不能证明未执行；不重发"),
        Err(_) => row.defer("原退款动作查询失败，保留未知结果"),
    }
}

#[cfg(test)]
mod tests {
    use std::sync::{Arc, Mutex};

    use entity_core::BaseModel;
    use erp_core::common::time::Instant;
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::ports::connector::common::{
        ActionKey, ConnectorError, ConnectorResult, Snapshot, SourceRevision, SourceStamp,
    };
    use erp_supply::ports::connector::order::{ActionAccepted, OrderReference, RefundReceipt, RequestRefund};

    use super::*;
    use crate::Error;

    struct Store {
        stored: Mutex<RefundExecution>,
        trace: Arc<Mutex<Vec<&'static str>>>,
        fail_on: usize,
        calls: Mutex<usize>,
    }
    #[async_trait]
    impl RefundStore for Store {
        async fn save(&self, row: &mut RefundExecution) -> Result<()> {
            self.trace.lock().unwrap().push("save");
            let mut calls = self.calls.lock().unwrap();
            *calls += 1;
            if self.fail_on == *calls {
                return Err(Error::ConflictError("test save failed".into()));
            }
            *self.stored.lock().unwrap() = row.clone();
            Ok(())
        }
    }
    struct Source {
        trace: Arc<Mutex<Vec<&'static str>>>,
        fail_send: bool,
        reject_send: bool,
        visible: bool,
        wrong_amount: bool,
    }
    #[async_trait]
    impl Refunds for Source {
        async fn request(&self, request: &RequestRefund) -> ConnectorResult<ActionAccepted> {
            self.trace.lock().unwrap().push("send");
            assert_eq!(request.action.id, "action");
            if self.fail_send || self.reject_send {
                return Err(ConnectorError {
                    class: if self.reject_send {
                        SupplierFailureClass::BusinessRejected
                    } else {
                        SupplierFailureClass::ResultUnknown
                    },
                    code: "TIMEOUT".into(),
                    summary: "unknown".into(),
                    retry_after: None,
                });
            }
            Ok(ActionAccepted {
                action_id: request.action.id.clone(),
                external_request_id: Some("remote-request".into()),
            })
        }
        async fn refund(
            &self,
            order: &OrderReference,
            action: &str,
        ) -> ConnectorResult<Lookup<Snapshot<RefundProgress>>> {
            self.trace.lock().unwrap().push("query");
            assert_eq!(action, "action");
            if !self.visible {
                return Ok(Lookup::NotVisible);
            }
            Ok(Lookup::Found(Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(200),
                },
                value: RefundProgress::Completed(RefundReceipt {
                    action_id: action.into(),
                    order: order.clone(),
                    external_refund_no: "refund".into(),
                    amount: if self.wrong_amount { "9" } else { "10" }.parse().unwrap(),
                    completed_at: Instant::from_unix_secs(100),
                }),
            }))
        }
    }
    fn row() -> RefundExecution {
        RefundExecution {
            base: BaseModel::new("action".into()),
            connection_id: "connection".into(),
            supplier_id: "supplier".into(),
            request: RequestRefund {
                action: ActionKey { id: "action".into(), payload_hash: "hash".into() },
                order: OrderReference {
                    merchant_order_no: "order".into(),
                    external_order_no: "external".into(),
                },
                lines: vec![],
                amount: "10".parse().unwrap(),
                reason_code: "reason".into(),
            },
            line_amounts: vec![],
            state: RefundExecutionState::Ready,
            receipt: None,
            external_request_id: None,
            blocker: None,
            next_run_at: Some(Instant::now()),
            application_hash: None,
            fact_id: None,
            manual_evidence: None,
        }
    }
    fn fixture(fail_on: usize) -> (Store, Source) {
        let trace = Arc::new(Mutex::new(vec![]));
        (
            Store { stored: Mutex::new(row()), trace: trace.clone(), fail_on, calls: Mutex::new(0) },
            Source { trace, fail_send: false, reject_send: false, visible: true, wrong_amount: false },
        )
    }
    #[tokio::test]
    async fn writes_intent_before_send_then_queries_receipt_without_financial_completion() {
        let (store, source) = fixture(0);
        let mut row = row();
        process(&store, Some(&source), &mut row).await.unwrap();
        assert_eq!(row.state, RefundExecutionState::Accepted);
        assert!(row.receipt.is_none());
        let mut recovered: RefundExecution =
            serde_json::from_str(&serde_json::to_string(&row).unwrap()).unwrap();
        process(&store, Some(&source), &mut recovered).await.unwrap();
        assert_eq!(recovered.state, RefundExecutionState::Received);
        assert!(recovered.fact_id.is_none());
        process(&store, Some(&source), &mut recovered).await.unwrap();
        assert_eq!(*store.trace.lock().unwrap(), ["save", "send", "save", "query", "save"]);
    }
    #[tokio::test]
    async fn failed_intent_save_never_calls_supplier_and_failed_result_save_recovers_by_query() {
        for fail_on in [1, 2] {
            let (store, source) = fixture(fail_on);
            let mut row = row();
            assert!(process(&store, Some(&source), &mut row).await.is_err());
            let saved = store.stored.lock().unwrap().clone();
            if fail_on == 1 {
                assert_eq!(saved.state, RefundExecutionState::Ready);
                assert_eq!(*store.trace.lock().unwrap(), ["save"]);
            } else {
                assert_eq!(saved.state, RefundExecutionState::Unknown);
                let mut recovered = saved;
                process(&store, Some(&source), &mut recovered).await.unwrap();
                assert_eq!(recovered.state, RefundExecutionState::Received);
                assert_eq!(store.trace.lock().unwrap().iter().filter(|s| **s == "send").count(), 1);
            }
        }
    }
    #[tokio::test]
    async fn timeout_and_not_visible_never_allow_resend() {
        let (store, mut source) = fixture(0);
        source.fail_send = true;
        source.visible = false;
        let mut row = row();
        process(&store, Some(&source), &mut row).await.unwrap();
        for _ in 0..3 {
            process(&store, Some(&source), &mut row).await.unwrap();
        }
        assert_eq!(row.state, RefundExecutionState::Unknown);
        assert!(row.receipt.is_none());
        assert_eq!(store.trace.lock().unwrap().iter().filter(|s| **s == "send").count(), 1);
    }
    #[tokio::test]
    async fn explicit_business_rejection_is_terminal_and_is_not_scheduled_as_unknown() {
        let (store, mut source) = fixture(0);
        source.reject_send = true;
        let mut row = row();
        process(&store, Some(&source), &mut row).await.unwrap();
        assert_eq!(row.state, RefundExecutionState::Rejected);
        assert!(row.next_run_at.is_none());
        process(&store, Some(&source), &mut row).await.unwrap();
        assert_eq!(*store.trace.lock().unwrap(), ["save", "send", "save"]);
    }

    #[tokio::test]
    async fn absent_capability_is_manual_and_stops_scheduling_without_success() {
        let (store, _) = fixture(0);
        let mut row = row();
        process(&store, None, &mut row).await.unwrap();
        process(&store, None, &mut row).await.unwrap();
        assert_eq!(row.state, RefundExecutionState::Unsupported);
        assert!(row.next_run_at.is_none());
        assert!(row.receipt.is_none());
        assert_eq!(*store.trace.lock().unwrap(), ["save"]);
    }
    #[tokio::test]
    async fn wrong_receipt_amount_stays_unresolved_and_capability_loss_preserves_sent_identity() {
        let (store, mut source) = fixture(0);
        source.wrong_amount = true;
        let mut row = row();
        process(&store, Some(&source), &mut row).await.unwrap();
        process(&store, Some(&source), &mut row).await.unwrap();
        assert_eq!(row.state, RefundExecutionState::Accepted);
        assert!(row.receipt.is_none());
        process(&store, None, &mut row).await.unwrap();
        assert_eq!(row.state, RefundExecutionState::Accepted);
        assert!(row.next_run_at.is_none());
    }
}
