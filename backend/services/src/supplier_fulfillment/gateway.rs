//! 域 D32 `supplier_fulfillment` 的供应商网关抽象（外部 HTTP 调用的接缝）。
//!
//! P3 §7：供应商 API 调用必须在事务之外完成，先事务内落 `inbox_message`，事务外
//! 调用网关；结果经 `inbox_message` + `integration_error_task` 承接。本模块定义
//! 网关 trait 与结果分类，禁止在事务闭包内触发网关。
//!
//! 本批次不实现真实 HTTP Connector（二期 Supplier Connector 属于 D25 能力扩展，
//! 见 erp-phase-2.md §6.2），由 [`SimulatedSupplierGateway`] 按连接地址配置
//! 模拟结果分类：`sim://reject` / `sim://timeout` / `sim://temporary-failure` /
//! `sim://auth-signature`，其余视为接单成功。真实网关接入时以同 trait 实现即可，
//! 接口形状不变。

use entities::integration_ops::ErrorClass;
use entities::supplier_api::SupplierApiConnection;
use entities::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};

/// 网关对一次供应商动作请求的处理结果分类（错误分类对齐 §6.21）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DispatchOutcome {
    /// 成功：返回供应商请求号与（下单动作的）外部订单号。
    Succeeded {
        /// 供应商请求号。
        external_request_id: String,
        /// 供应商订单号（下单动作返回，其余动作为 `None`）。
        external_order_no: Option<String>,
    },
    /// 业务明确拒绝（不自动重试）。
    Rejected {
        /// 脱敏拒绝原因摘要。
        summary: String,
    },
    /// 网络超时/查询能力不足导致结果未知（先查询原请求，不盲目重发）。
    ResultUnknown {
        /// 脱敏结果摘要。
        summary: String,
    },
    /// 其他失败分类（临时故障/鉴权签名/限流等）。
    Failed {
        /// 错误分类。
        error_class: ErrorClass,
        /// 脱敏失败摘要。
        summary: String,
    },
}

/// 供应商网关：向 API 供应商发起下单/取消/退款请求并返回分类结果。
///
/// 实现必须自带超时、重试上限与错误分类（AGENTS.md 外部依赖容错）；
/// 失败只以 [`DispatchOutcome`] 返回，不向调用方抛传输层错误。
pub trait SupplierGateway: Send + Sync {
    /// 向供应商发起一次动作请求。
    ///
    /// # 参数
    /// * `action` - 待发送的动作（含幂等键与摘要）
    /// * `order` - 所属供应商子订单
    /// * `connection` - 供应商 API 连接（地址/密钥引用等配置）
    ///
    /// # 返回
    /// 返回分类后的处理结果；实现内部完成超时与重试，不直接失败。
    fn dispatch<'a>(
        &'a self,
        action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = DispatchOutcome> + Send + 'a>>;
}

/// 模拟供应商网关：按连接地址配置模拟结果分类，供本批次接口与测试使用。
///
/// `endpoint_reference` 以 `sim://` 前缀开头的已知取值模拟对应结果，
/// 其余取值一律模拟接单成功（`Succeeded`，外部订单号 `EXT-{订单号}`）。
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
        Some("temporary-failure") => DispatchOutcome::Failed {
            error_class: ErrorClass::TransientFailure,
            summary: "供应商接口临时不可用（模拟）".to_string(),
        },
        Some("auth-signature") => DispatchOutcome::Failed {
            error_class: ErrorClass::AuthSignature,
            summary: "鉴权或签名校验失败（模拟）".to_string(),
        },
        _ => DispatchOutcome::Succeeded {
            external_request_id: format!("SIM-REQ-{}", order.fulfillment_order_no),
            external_order_no: (action.action_type
                == entities::supplier_fulfillment::SupplierOrderActionType::Place)
                .then(|| format!("EXT-{}", order.fulfillment_order_no)),
        },
    }
}

#[cfg(test)]
mod tests {
    use super::{simulate_outcome, DispatchOutcome};
    use entities::common::time::Instant;
    use entities::ids::{MallOrderId, SupplierAccountId, SupplierApiConnectionId};
    use entities::supplier_api::{
        ConnectionEnvironment, SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };
    use entities::supplier_fulfillment::{
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
                mall_order_id: MallOrderId::new("mall-order-1"),
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
        let after_sales_request_id = action_type
            .requires_after_sales_request()
            .then(|| entities::ids::MallAfterSalesRequestId::new("request-1"));
        SupplierOrderAction::new(
            SupplierOrderActionId::new("action-1"),
            SupplierOrderActionData {
                supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
                action_type,
                after_sales_request_id,
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
    fn default_endpoint_simulates_accepted_place() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Place);
        let outcome = simulate_outcome(
            &action,
            &order,
            &sample_connection("https://supplier.example.com/api"),
        );
        assert!(matches!(
            outcome,
            DispatchOutcome::Succeeded {
                external_order_no: Some(no), ..
            } if no == "EXT-FO-2026-001"
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
                error_class: entities::integration_ops::ErrorClass::TransientFailure,
                ..
            }
        ));
    }

    #[test]
    fn non_place_action_has_no_external_order_no() {
        let order = sample_order();
        let action = sample_action(SupplierOrderActionType::Refund);
        let outcome = simulate_outcome(
            &action,
            &order,
            &sample_connection("https://supplier.example.com/api"),
        );
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
        assert!(entities::money::Quantity::from_str("1.000000").is_ok());
        assert!(entities::money::Amount::from_str("9.99").is_ok());
    }
}
