//! 原订单确认只读取原单事实；创建预检不能作为已预占订单的再次购买门槛。

use async_trait::async_trait;
use erp_supply::api_order::{ApiOrderExecution, ApiOrderStage};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{ConnectorResult, Snapshot};
use erp_supply::ports::connector::order::{ConfirmOrder, ConfirmationState, OrderConfirmation};

use super::super::{SupplierApiOrderProcess, missing_capability};
use super::next_confirmation;
use crate::Result;

/// 当前原单查询、同事务资格锁定及独立发送的最小确认步骤。
#[async_trait]
pub(super) trait ConfirmationApplication: Sync {
    /// 查询当前完整原单树并核对数量、实际金额及批准预算。
    /// # 参数
    /// 原执行快照；不得查询目录购买库存或重建创建请求。
    /// # 返回
    /// 更新后的当前子单及确认事实。
    /// # 错误
    /// 身份、数量、费用或来源事实不完整时拒绝。
    async fn refresh(&self, execution: &mut ApiOrderExecution) -> Result<()>;

    /// 原子登记未知动作，并重验采购、开关、付款批准及执行组资金覆盖。
    /// # 参数
    /// 已选择当前叶子且保留独立原目标的执行记录。
    /// # 返回
    /// 事务提交后才允许发送；资金覆盖包含本组剩余占用和已核实净扣款。
    /// # 错误
    /// 版本、授权、资金或持久化失败时拒绝且不发送。
    async fn claim(&self, execution: &mut ApiOrderExecution) -> Result<()>;

    /// 对已可靠保存的原目标发送一次确认。
    /// # 参数
    /// 原动作身份、当前叶子及拓扑版本。
    /// # 返回
    /// 本次明确结果。
    /// # 错误
    /// 不确定结果继续调查，不隐藏重试。
    async fn send(&self, request: &ConfirmOrder) -> ConnectorResult<Snapshot<ConfirmationState>>;

    /// 保存当前结果或阻断原因，不改变原创建载荷。
    /// # 参数
    /// 本轮执行记录。
    /// # 返回
    /// 保存成功。
    /// # 错误
    /// 持久化失败时返回错误，已登记未知身份保持有效。
    async fn persist(&self, execution: &mut ApiOrderExecution) -> Result<()>;
}

/// 生产确认步骤复用既有查询、事务与供应商窄能力。
pub(super) struct MongoConfirmationApplication<'a> {
    process: &'a SupplierApiOrderProcess,
    confirmation: &'a dyn OrderConfirmation,
}

impl<'a> MongoConfirmationApplication<'a> {
    /// 复用真实原单查询、事务资格锁定及已登记确认协议。
    /// # 参数
    /// 当前 Process 与已经核对技术及业务绑定的连接器。
    /// # 返回
    /// 不发送任何请求的确认步骤集合。
    /// # 错误
    /// 缺独立确认能力时拒绝。
    pub(super) fn new(
        process: &'a SupplierApiOrderProcess,
        connector: &'a dyn SupplierConnector,
    ) -> Result<Self> {
        let confirmation =
            connector.order_confirmation().ok_or_else(|| missing_capability("预占订单确认"))?;
        Ok(Self { process, confirmation })
    }
}

#[async_trait]
impl ConfirmationApplication for MongoConfirmationApplication<'_> {
    async fn refresh(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.process.refresh_order_tree(execution).await
    }

    async fn claim(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.process.claim(execution, ApiOrderStage::ConfirmationUnknown).await
    }

    async fn send(&self, request: &ConfirmOrder) -> ConnectorResult<Snapshot<ConfirmationState>> {
        self.confirmation.confirm_order(request).await
    }

    async fn persist(&self, execution: &mut ApiOrderExecution) -> Result<()> {
        self.process.persist(execution).await
    }
}

/// 仅基于刚查询的原订单事实及原执行组资金资格确认当前叶子。
/// # 参数
/// 实际确认步骤与服务器持久化的原执行记录。
/// # 返回
/// 原确认明确成功或待调查状态已保存。
/// # 错误
/// 当前资格不满足或事务失败时停止；已发送动作不再次发送。
pub(super) async fn confirm_current_order(
    application: &impl ConfirmationApplication,
    mut execution: ApiOrderExecution,
) -> Result<()> {
    if let Err(error) = application.refresh(&mut execution).await {
        execution.defer(error.to_string());
        return application.persist(&mut execution).await;
    }
    if execution.stage != ApiOrderStage::ConfirmOrder {
        return application.persist(&mut execution).await;
    }

    let request = next_confirmation(&mut execution)?;
    if !execution.begin_confirmation(&request)? {
        execution.defer("原订单确认已经发送，请查询原结果");
        return application.persist(&mut execution).await;
    }
    // 本事务仍重验批准、授权和专属资金；不重复要求已预占库存或整单可用余额。
    application.claim(&mut execution).await?;

    match application.send(&request).await {
        Ok(result) => {
            execution.observe_confirmation(&request.order.external_order_no, result.value)?;
            execution.blocker = None;
            if result.value == ConfirmationState::Pending {
                execution.defer("原订单确认尚未查明，系统继续查询，不重复确认");
            }
        },
        Err(error) => execution.defer(error.summary),
    }

    application.persist(&mut execution).await
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use erp_core::common::time::Instant;
    use erp_supply::api_order::ConfirmationActionState;
    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::ports::connector::common::{ConnectorError, SourceRevision, SourceStamp};
    use serde_json::to_value;

    use super::super::tests::{budget, fixture};
    use super::super::{apply_confirmation_observations, validate_leaf_budget};
    use super::*;
    use crate::Error;

    #[derive(Clone, Copy)]
    enum ClaimResult {
        Covered,
        MissingCoverage,
        AuthorizationRevoked,
    }

    struct Application {
        calls: Mutex<Vec<&'static str>>,
        sent: Mutex<Vec<ConfirmOrder>>,
        saved: Mutex<Option<ApiOrderExecution>>,
        claim_result: ClaimResult,
        send_result: ConfirmationState,
        send_unknown: bool,
    }

    impl Application {
        fn new(claim_result: ClaimResult) -> Self {
            Self {
                calls: Mutex::new(vec![]),
                sent: Mutex::new(vec![]),
                saved: Mutex::new(None),
                claim_result,
                send_result: ConfirmationState::Confirmed,
                send_unknown: false,
            }
        }
    }

    #[async_trait]
    impl ConfirmationApplication for Application {
        async fn refresh(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            self.calls.lock().unwrap().push("query-original-order");
            // 来源替身只提供当前原树，金额规则及状态应用仍执行生产函数。
            validate_leaf_budget(execution.order_tree.as_ref().unwrap(), &budget())?;
            apply_confirmation_observations(execution)
        }

        async fn claim(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            self.calls.lock().unwrap().push("claim-current-qualification");
            assert_eq!(execution.stage, ApiOrderStage::ConfirmationUnknown);
            assert_eq!(
                execution
                    .confirmations
                    .values()
                    .filter(|action| action.state == ConfirmationActionState::SentUnknown)
                    .count(),
                1
            );
            match self.claim_result {
                ClaimResult::Covered => Ok(()),
                ClaimResult::MissingCoverage => Err(Error::BusinessLogicError("本组资金保障不足".into())),
                ClaimResult::AuthorizationRevoked => Err(Error::BusinessLogicError("原授权已撤销".into())),
            }
        }

        async fn send(&self, request: &ConfirmOrder) -> ConnectorResult<Snapshot<ConfirmationState>> {
            self.calls.lock().unwrap().push("send-original-leaf-once");
            self.sent.lock().unwrap().push(request.clone());
            if self.send_unknown {
                return Err(ConnectorError {
                    class: SupplierFailureClass::ResultUnknown,
                    code: "CONFIRMATION_TIMEOUT".into(),
                    summary: "原确认响应待核实".into(),
                    retry_after: None,
                });
            }
            Ok(Snapshot {
                stamp: SourceStamp {
                    revision: SourceRevision::Unversioned,
                    changed_at: None,
                    observed_at: Instant::from_unix_secs(1),
                },
                value: self.send_result,
            })
        }

        async fn persist(&self, execution: &mut ApiOrderExecution) -> Result<()> {
            self.calls.lock().unwrap().push("persist-original-result");
            *self.saved.lock().unwrap() = Some(execution.clone());
            Ok(())
        }
    }

    #[tokio::test]
    async fn reserved_order_confirms_with_original_identity_without_reusing_expired_creation_proof() {
        let (mut execution, _) = fixture();
        execution.request_encrypted = Some("original-create-snapshot-with-expired-delivery-proof".into());
        execution.action_hashes.insert("create".into(), "original-create-fingerprint".into());
        let original_create = execution.action_hashes["create"].clone();
        let original_payload = execution.request_encrypted.clone();
        let application = Application::new(ClaimResult::Covered);
        confirm_current_order(&application, execution).await.unwrap();

        assert_eq!(
            *application.calls.lock().unwrap(),
            [
                "query-original-order",
                "claim-current-qualification",
                "send-original-leaf-once",
                "persist-original-result"
            ]
        );

        let sent = application.sent.lock().unwrap();
        assert_eq!(sent.len(), 1);
        assert_eq!(sent[0].order.merchant_order_no, "group");
        assert_eq!(sent[0].order.external_order_no, "child-a");

        let saved = application.saved.lock().unwrap();
        let saved = saved.as_ref().unwrap();
        assert_eq!(saved.stage, ApiOrderStage::ConfirmOrder);
        assert_eq!(saved.action_hashes["create"], original_create);
        assert_eq!(saved.request_encrypted, original_payload);
        assert_eq!(saved.confirmations["child-a"].state, ConfirmationActionState::Confirmed);
    }

    #[tokio::test]
    async fn original_order_over_budget_stops_before_claim_and_confirm() {
        let (mut execution, _) = fixture();
        let amounts = execution.order_tree.as_mut().unwrap().nodes[1].amounts.as_mut().unwrap();
        amounts.shipping_cost = Some("3".parse().unwrap());
        amounts.payable = "13".parse().unwrap();
        let application = Application::new(ClaimResult::Covered);
        confirm_current_order(&application, execution).await.unwrap();
        assert_eq!(*application.calls.lock().unwrap(), ["query-original-order", "persist-original-result"]);
        assert!(application.sent.lock().unwrap().is_empty());

        let saved = application.saved.lock().unwrap();
        let saved = saved.as_ref().unwrap();
        assert!(saved.confirmations.is_empty());
        assert!(saved.blocker.as_ref().unwrap().contains("对应批准范围"));
    }

    #[tokio::test]
    async fn revoked_authorization_or_missing_local_coverage_never_sends_confirmation() {
        for claim in [ClaimResult::MissingCoverage, ClaimResult::AuthorizationRevoked] {
            let (execution, _) = fixture();
            let application = Application::new(claim);
            let error = confirm_current_order(&application, execution).await.unwrap_err();
            assert!(matches!(error, Error::BusinessLogicError(_)));
            assert_eq!(
                *application.calls.lock().unwrap(),
                ["query-original-order", "claim-current-qualification"]
            );
            assert!(application.sent.lock().unwrap().is_empty());
            assert!(application.saved.lock().unwrap().is_none());
        }
    }

    #[tokio::test]
    async fn pending_original_confirmation_is_only_queried_and_never_resent() {
        let (mut execution, _) = fixture();
        let request = next_confirmation(&mut execution).unwrap();
        execution.begin_confirmation(&request).unwrap();
        let original = to_value(&execution.confirmations).unwrap();
        let application = Application::new(ClaimResult::Covered);
        confirm_current_order(&application, execution).await.unwrap();

        assert_eq!(*application.calls.lock().unwrap(), ["query-original-order", "persist-original-result"]);
        assert!(application.sent.lock().unwrap().is_empty());

        let saved = application.saved.lock().unwrap();
        let saved = saved.as_ref().unwrap();
        assert_eq!(saved.stage, ApiOrderStage::ConfirmationUnknown);
        assert_eq!(to_value(&saved.confirmations).unwrap(), original);
    }

    #[tokio::test]
    async fn successful_partial_confirmation_keeps_other_leaves_available_without_resending_the_first() {
        let (execution, _) = fixture();
        let application = Application::new(ClaimResult::Covered);
        confirm_current_order(&application, execution).await.unwrap();
        let next = application.saved.lock().unwrap().clone().unwrap();
        confirm_current_order(&application, next).await.unwrap();

        let sent = application.sent.lock().unwrap();
        assert_eq!(sent.len(), 2);
        assert_eq!(sent[0].order.external_order_no, "child-a");
        assert_eq!(sent[1].order.external_order_no, "child-b");
        let saved = application.saved.lock().unwrap();
        assert_eq!(saved.as_ref().unwrap().stage, ApiOrderStage::Observe);
    }

    #[tokio::test]
    async fn confirmation_timeout_preserves_original_target_and_the_next_run_only_queries() {
        let (execution, _) = fixture();
        let mut application = Application::new(ClaimResult::Covered);
        application.send_unknown = true;
        confirm_current_order(&application, execution).await.unwrap();

        let original = application.sent.lock().unwrap()[0].clone();
        let saved = application.saved.lock().unwrap().clone().unwrap();
        assert_eq!(saved.stage, ApiOrderStage::ConfirmationUnknown);
        assert_eq!(saved.confirmations["child-a"].request, original);
        assert!(saved.blocker.as_ref().unwrap().contains("待核实"));

        application.calls.lock().unwrap().clear();
        confirm_current_order(&application, saved).await.unwrap();
        assert_eq!(*application.calls.lock().unwrap(), ["query-original-order", "persist-original-result"]);
        assert_eq!(*application.sent.lock().unwrap(), [original]);
    }
}
