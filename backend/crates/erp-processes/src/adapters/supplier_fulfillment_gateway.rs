//! 供应商网关失败关闭与显式 sim 地址 adapter；不伪造真实 URL 成功。
use erp_supply::entity::failure::SupplierFailureClass as ErrorClass;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use erp_supply::ports::supplier_gateway::{DispatchOutcome, InvestigationOutcome, SupplierGateway};
/// 未配置生产 Connector 时的失败关闭网关。
#[derive(Debug, Default)]
pub struct UnavailableSupplierGateway;

impl SupplierGateway for UnavailableSupplierGateway {
    fn dispatch<'a>(
        &'a self,
        _action: &'a SupplierOrderAction,
        _order: &'a SupplierFulfillmentOrder,
        _connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + 'a>> {
        Box::pin(async {
            DispatchOutcome::Failed {
                error_class: ErrorClass::CapabilityGap,
                summary: "供应商连接器未配置，未发送外部请求".to_string(),
            }
        })
    }

    fn investigate<'a>(
        &'a self,
        _target_action: &'a SupplierOrderAction,
        _order: &'a SupplierFulfillmentOrder,
        _connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = InvestigationOutcome> + Send + 'a>> {
        Box::pin(async {
            InvestigationOutcome::ResultUnknown {
                summary: "供应商连接器未配置，无法验证原请求结果".to_string(),
            }
        })
    }
}

/// 模拟供应商网关：按连接地址配置模拟结果分类，供本批次接口与测试使用。
///
/// `endpoint_reference` 以 `sim://` 前缀开头的已知取值模拟对应结果，
/// 其余取值一律失败关闭，禁止把真实 URL 当成模拟成功。
/// 该网关不发任何网络请求，测试可借此注入失败路径验证降级。
#[derive(Debug, Default)]
pub struct SimulatedSupplierGateway;

impl SimulatedSupplierGateway {
    /// 构造模拟网关。
    ///
    /// # 返回
    /// 返回无状态模拟网关实例。
    pub fn new() -> Self {
        Self
    }
}

impl SupplierGateway for SimulatedSupplierGateway {
    fn dispatch<'a>(
        &'a self,
        action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + 'a>> {
        Box::pin(async move { simulate_outcome(action, order, connection) })
    }

    fn investigate<'a>(
        &'a self,
        target_action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = InvestigationOutcome> + Send + 'a>> {
        Box::pin(async move { simulate_investigation(target_action, order, connection) })
    }
}

/// 按连接地址配置模拟动作结果。
///
/// # 参数
/// * `action` - 待发送的动作
/// * `order` - 所属供应商子订单
/// * `connection` - 供应商 API 连接
///
/// # 返回
/// 返回模拟分类结果。
fn simulate_outcome(
    action: &SupplierOrderAction,
    order: &SupplierFulfillmentOrder,
    connection: &SupplierApiConnection,
) -> DispatchOutcome {
    let endpoint = connection.endpoint_reference.trim();
    match endpoint.strip_prefix("sim://") {
        Some("reject") => DispatchOutcome::Rejected {
            summary: "供应商明确拒绝（模拟）".to_string(),
        },
        Some("timeout") => DispatchOutcome::ResultUnknown {
            summary: "请求超时，结果未知（模拟）".to_string(),
        },
        Some("query-no-result") => DispatchOutcome::ResultUnknown {
            summary: "请求结果未知，需查询原结果（模拟）".to_string(),
        },
        Some("temporary-failure") => DispatchOutcome::Failed {
            error_class: ErrorClass::TransientFailure,
            summary: "供应商接口临时不可用（模拟）".to_string(),
        },
        Some("auth-signature") => DispatchOutcome::Failed {
            error_class: ErrorClass::AuthSignature,
            summary: "鉴权或签名校验失败（模拟）".to_string(),
        },
        Some("success") => DispatchOutcome::Succeeded {
            external_request_id: format!("SIM-REQ-{}", order.fulfillment_order_no),
            external_order_no: (action.action_type
                == erp_supply::entity::supplier_fulfillment::SupplierOrderActionType::Place)
                .then(|| format!("EXT-{}", order.fulfillment_order_no)),
        },
        _ => DispatchOutcome::Failed {
            error_class: ErrorClass::CapabilityGap,
            summary: "未配置可执行的供应商连接器，未发送外部请求".to_string(),
        },
    }
}

/// 按连接地址模拟对原动作的只读结果查询。
///
/// 默认值必须保持结果未知；仅显式 `sim://query-no-result` 返回“明确无结果”，
/// 防止把普通连接或一次成功 HTTP 查询误判为可安全重放。
fn simulate_investigation(
    _target_action: &SupplierOrderAction,
    _order: &SupplierFulfillmentOrder,
    connection: &SupplierApiConnection,
) -> InvestigationOutcome {
    match connection.endpoint_reference.trim().strip_prefix("sim://") {
        Some("query-no-result") => InvestigationOutcome::VerifiedNoResult {
            summary: "供应商明确返回原请求未形成结果（模拟）".to_string(),
        },
        _ => InvestigationOutcome::ResultUnknown {
            summary: "供应商未返回足以证明原请求结果的证据".to_string(),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{simulate_investigation, simulate_outcome, DispatchOutcome, InvestigationOutcome};
    use erp_core::common::time::Instant;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_supply::entity::supplier_api::{
        ConnectionEnvironment, SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };
    use erp_supply::entity::supplier_fulfillment::{
        CancelStatus, FulfillmentStatus, RefundStatus, SupplierFulfillmentOrder,
        SupplierFulfillmentOrderData, SupplierFulfillmentOrderId, SupplierOrderAction,
        SupplierOrderActionData, SupplierOrderActionId, SupplierOrderActionStatus, SupplierOrderActionType,
    };
    use std::str::FromStr;

    fn sample_connection(endpoint_reference: &str) -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("connection-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_code: "SUP-1".to_string(),
                environment: ConnectionEnvironment::Production,
                endpoint_reference: endpoint_reference.to_string(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Active,
            },
            "actor-1",
        )
        .unwrap()
    }

    fn sample_order() -> SupplierFulfillmentOrder {
        SupplierFulfillmentOrder::new(
            SupplierFulfillmentOrderId::new("order-1"),
            SupplierFulfillmentOrderData {
                fulfillment_order_no: "FO-2026-001".to_string(),
                supplier_id: SupplierAccountId::new("supplier-1"),
                connection_id: SupplierApiConnectionId::new("connection-1"),
                split_no: 1,
                fulfillment_status: FulfillmentStatus::Submitting,
                cancel_status: CancelStatus::None,
                refund_status: RefundStatus::None,
                external_order_no: None,
                submitted_at: Some(Instant::from_unix_secs(1_700_000_000)),
                accepted_at: None,
                completed_at: None,
                address_snapshot_encrypted: "encrypted".to_string(),
                address_snapshot_fingerprint: "fingerprint".to_string(),
            },
        )
        .unwrap()
    }

    fn sample_action(action_type: SupplierOrderActionType) -> SupplierOrderAction {
        SupplierOrderAction::new(
            SupplierOrderActionId::new("action-1"),
            SupplierOrderActionData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                action_type,
                idempotency_key: "FO-2026-001".to_string(),
                status: SupplierOrderActionStatus::Pending,
                external_request_id: None,
                request_summary: None,
                response_summary: None,
                attempt_count: 0,
                next_attempt_at: None,
            },
        )
        .unwrap()
    }

    #[test]
    fn ordinary_endpoint_never_simulates_accepted_place() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Place);
        let outcome = simulate_outcome(
            &action,
            &order,
            &sample_connection("https://supplier.example.com/api"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Failed {
                error_class: erp_supply::entity::failure::SupplierFailureClass::CapabilityGap,
                ..
            }
        ));
    }

    #[test]
    fn simulated_endpoints_classify_failure_paths() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Cancel);
        assert!(matches!(
            simulate_outcome(&action, &order, &sample_connection("sim://reject")),
            DispatchOutcome::Rejected { .. }
        ));
        assert!(matches!(
            simulate_outcome(&action, &order, &sample_connection("sim://timeout")),
            DispatchOutcome::ResultUnknown { .. }
        ));
        assert!(matches!(
            simulate_outcome(&action, &order, &sample_connection("sim://temporary-failure")),
            DispatchOutcome::Failed {
                error_class: erp_supply::entity::failure::SupplierFailureClass::TransientFailure,
                ..
            }
        ));
    }

    #[test]
    fn successful_query_transport_is_not_a_verified_business_result() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Place);

        assert!(matches!(
            simulate_investigation(
                &action,
                &order,
                &sample_connection("https://supplier.example.com/api")
            ),
            InvestigationOutcome::ResultUnknown { .. }
        ));
        assert!(matches!(
            simulate_investigation(&action, &order, &sample_connection("sim://query-no-result")),
            InvestigationOutcome::VerifiedNoResult { .. }
        ));
    }

    #[test]
    fn query_no_result_scenario_starts_unknown_before_proving_no_result() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Place);
        let connection = sample_connection("sim://query-no-result");

        assert!(matches!(
            simulate_outcome(&action, &order, &connection),
            DispatchOutcome::ResultUnknown { .. }
        ));
        assert!(matches!(
            simulate_investigation(&action, &order, &connection),
            InvestigationOutcome::VerifiedNoResult { .. }
        ));
    }

    #[test]
    fn non_place_action_has_no_external_order_no() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Refund);
        let outcome = simulate_outcome(&action, &order, &sample_connection("sim://success"));
        assert!(matches!(
            outcome,
            DispatchOutcome::Succeeded {
                external_order_no: None,
                ..
            }
        ));
    }

    #[test]
    fn amounts_parse_from_string_shape() {
        assert!(erp_core::money::Quantity::from_str("1.000000").is_ok());
        assert!(erp_core::money::Amount::from_str("9.99").is_ok());
    }
}
