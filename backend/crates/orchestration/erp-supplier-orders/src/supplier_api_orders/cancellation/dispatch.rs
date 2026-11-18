use std::sync::Arc;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supply::api_order::ApiOrderExecution;
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::ConnectorResult;
use erp_supply::ports::connector::order::{ActionAccepted, CancelOrder};

use super::SupplierApiOrderProcess;
use super::persistence::CancellationWrite;
use crate::Result;

#[async_trait]
pub(super) trait CancellationDispatch: Send + Sync {
    async fn validate(&self, _request: &CancelOrder) -> ConnectorResult<()> {
        Ok(())
    }
    async fn persist_intent(&self, execution: &mut ApiOrderExecution) -> Result<()>;
    async fn cancel(&self, request: &CancelOrder) -> ConnectorResult<ActionAccepted>;
    async fn persist_result(&self, execution: &mut ApiOrderExecution) -> Result<()>;
}

pub(super) struct MongoCancellationDispatch<'a> {
    pub process: &'a SupplierApiOrderProcess,
    pub connector: Arc<dyn SupplierConnector>,
}

#[async_trait]
impl CancellationDispatch for MongoCancellationDispatch<'_> {
    async fn validate(&self, request: &CancelOrder) -> ConnectorResult<()> {
        self.connector.cancellations().expect("取消适配器已核实").validate(request).await
    }
    async fn persist_intent(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.process.persist_cancellation(execution, CancellationWrite::SendIntent).await
    }
    async fn cancel(&self, request: &CancelOrder) -> ConnectorResult<ActionAccepted> {
        self.connector.cancellations().expect("取消适配器已在发送意图事务重验").cancel(request).await
    }
    async fn persist_result(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.process.persist_cancellation(execution, CancellationWrite::Progress).await
    }
}

pub(super) async fn send_once<P: CancellationDispatch>(
    port: &P,
    execution: &mut ApiOrderExecution,
    target: &str,
) -> Result<bool> {
    let Some(ready) = execution.cancellation_request(target)? else { return Ok(false) };
    if let Err(error) = port.validate(&ready).await {
        execution.defer(error.summary);
        port.persist_result(execution).await?;
        return Ok(false);
    }
    let Some(request) = execution.begin_cancellation(target)? else { return Ok(false) };
    port.persist_intent(execution).await?;
    match port.cancel(&request).await {
        Ok(accepted) => {
            if let Err(error) = execution.accept_cancellation(target, accepted) {
                execution.defer(error.to_string());
            }
        },
        Err(error) => execution.defer(error.summary),
    }
    execution.refresh_cancellation_stage();
    execution.next_run_at = Instant::from_unix_secs(Instant::now().unix_secs().saturating_add(60));
    port.persist_result(execution).await?;
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_supply::api_order::{ApiOrderStage, CancellationActionState, CancellationScope};
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::ports::connector::common::{ConnectorError, SupplierSku};
    use erp_supply::ports::connector::order::{ConfirmationState, OrderLine};
    use erp_supply::ports::connector::order_tree::{OrderNode, OrderNodeLine, OrderNodeRole, OrderTree};

    use super::*;
    use crate::Error;

    #[derive(Default)]
    struct State {
        trace: Vec<&'static str>,
        stored: Option<ApiOrderExecution>,
    }
    struct RecordingDispatch {
        state: Mutex<State>,
        fail_save: bool,
        response: Option<SupplierFailureClass>,
    }

    struct RejectedEligibility(Mutex<Option<ApiOrderExecution>>);

    #[async_trait]
    impl CancellationDispatch for RejectedEligibility {
        async fn validate(&self, _: &CancelOrder) -> ConnectorResult<()> {
            Err(ConnectorError {
                class: SupplierFailureClass::BusinessRejected,
                code: "JD_VOP_CANCEL_3204".into(),
                summary: "原订单当前不可取消".into(),
                retry_after: None,
            })
        }
        async fn persist_intent(&self, _: &mut ApiOrderExecution) -> Result<()> {
            panic!("只读资格拒绝不得登记可能发送")
        }
        async fn cancel(&self, _: &CancelOrder) -> ConnectorResult<ActionAccepted> {
            panic!("只读资格拒绝不得发送取消")
        }
        async fn persist_result(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            *self.0.lock().unwrap() = Some(execution.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn eligibility_denial_preserves_ready_action_and_never_marks_or_sends_unknown() {
        let port = RejectedEligibility(Mutex::new(None));
        let mut execution = execution();
        let original = execution.cancellation_actions["root"].request();
        assert!(!send_once(&port, &mut execution, "root").await.unwrap());
        assert_eq!(execution.cancellation_actions["root"].state, CancellationActionState::Ready);
        assert_eq!(execution.cancellation_actions["root"].request(), original);
        assert_eq!(port.0.lock().unwrap().as_ref().unwrap().blocker.as_deref(), Some("原订单当前不可取消"));
    }

    #[async_trait]
    impl CancellationDispatch for RecordingDispatch {
        async fn persist_intent(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.trace.push("intent");
            assert_eq!(execution.cancellation_actions["root"].state, CancellationActionState::SentUnknown);
            if self.fail_save {
                return Err(Error::ConflictError("保存失败".into()));
            }
            state.stored = Some(execution.clone());
            Ok(())
        }
        async fn cancel(&self, request: &CancelOrder) -> ConnectorResult<ActionAccepted> {
            let mut state = self.state.lock().unwrap();
            state.trace.push("send");
            let stored = state.stored.as_ref().expect("必须先保存原动作");
            assert_eq!(stored.cancellation_actions["root"].action, request.action);
            assert!(request.lines.is_none());
            if let Some(class) = self.response {
                return Err(ConnectorError {
                    class,
                    code: "TEST".into(),
                    summary: "原取消结果需查询".into(),
                    retry_after: None,
                });
            }
            Ok(ActionAccepted {
                action_id: request.action.id.clone(),
                external_request_id: Some("remote-request".into()),
            })
        }
        async fn persist_result(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            let mut state = self.state.lock().unwrap();
            state.trace.push("result");
            state.stored = Some(execution.clone());
            Ok(())
        }
    }

    fn execution() -> ApiOrderExecution {
        let line = OrderLine {
            source_purchase_terms: None,
            line_id: "item".into(),
            sku: SupplierSku { product_id: None, spec_id: "sku".into() },
            quantity: "2".parse().unwrap(),
            approved_unit_price: "10".parse().unwrap(),
            attributes: vec![],
        };
        let mut execution = ApiOrderExecution::queued(
            "group".into(),
            "purchase".into(),
            "revision".into(),
            "connection".into(),
            "supplier".into(),
            vec![],
        );
        execution
            .apply_order_tree(
                OrderTree {
                    root_order_no: "root".into(),
                    version: 1,
                    nodes: vec![OrderNode {
                        external_order_no: "root".into(),
                        parent_order_no: None,
                        role: OrderNodeRole::Leaf,
                        lines: vec![OrderNodeLine {
                            line_id: line.line_id.clone(),
                            sku: line.sku.clone(),
                            quantity: line.quantity,
                        }],
                        confirmation: Some(ConfirmationState::Confirmed),
                        amounts: None,
                        fulfillment: None,
                        cancellation: None,
                        refund: None,
                    }],
                },
                &[line],
            )
            .unwrap();
        execution
            .prepare_cancellations(
                &CancellationScope { payload_hash: "payload".into(), reason_code: "reason".into() },
                "aggregate",
            )
            .unwrap();
        execution
    }

    #[tokio::test]
    async fn cancellation_is_saved_before_one_send_and_acceptance_never_means_complete() {
        let port = RecordingDispatch { state: Mutex::default(), fail_save: false, response: None };
        let mut execution = execution();
        assert!(send_once(&port, &mut execution, "root").await.unwrap());
        assert_eq!(port.state.lock().unwrap().trace, ["intent", "send", "result"]);
        assert_eq!(execution.cancellation_actions["root"].state, CancellationActionState::Accepted);
        assert_eq!(execution.stage, ApiOrderStage::CancellationUnknown);
        assert!(!execution.all_cancellations_confirmed());
        let mut recovered: ApiOrderExecution =
            serde_json::from_str(&serde_json::to_string(&execution).unwrap()).unwrap();
        assert!(!send_once(&port, &mut recovered, "root").await.unwrap());
        assert_eq!(port.state.lock().unwrap().trace.len(), 3);
    }

    #[tokio::test]
    async fn cancellation_save_failure_stops_before_supplier_http() {
        let port = RecordingDispatch { state: Mutex::default(), fail_save: true, response: None };
        assert!(send_once(&port, &mut execution(), "root").await.is_err());
        assert_eq!(port.state.lock().unwrap().trace, ["intent"]);
        assert!(port.state.lock().unwrap().stored.is_none());
    }

    #[tokio::test]
    async fn timeout_authentication_and_business_errors_preserve_original_unknown_without_resend() {
        for class in [
            SupplierFailureClass::ResultUnknown,
            SupplierFailureClass::AuthSignature,
            SupplierFailureClass::BusinessRejected,
        ] {
            let port = RecordingDispatch { state: Mutex::default(), fail_save: false, response: Some(class) };
            let mut execution = execution();
            send_once(&port, &mut execution, "root").await.unwrap();
            assert_eq!(execution.cancellation_actions["root"].state, CancellationActionState::SentUnknown);
            assert!(execution.blocker.is_some());
            assert!(!send_once(&port, &mut execution, "root").await.unwrap());
            assert_eq!(port.state.lock().unwrap().trace, ["intent", "send", "result"]);
        }
    }
}
