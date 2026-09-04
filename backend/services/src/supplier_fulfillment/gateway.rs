//! 域 D32 `supplier_fulfillment` 的供应商网关抽象（外部 HTTP 调用的接缝）。
//!
//! P3 §7：供应商 API 调用必须在事务之外完成，先事务内落 `inbox_message`，事务外
//! 调用网关；结果经 `inbox_message` + `integration_error_task` 承接。本模块定义
//! 网关 trait 与结果分类，禁止在事务闭包内触发网关。
//!
//! 生产默认使用失败关闭网关；[`SimulatedSupplierGateway`] 只允许明确
//! `sim://` 地址在测试中产生模拟结果。任何普通 URL 都不得被伪造为供应商成功。

use entities::integration_ops::ErrorClass;
use entities::supplier_api::SupplierApiConnection;
use entities::supplier_fulfillment::{SupplierFulfillmentOrder, SupplierOrderAction};
use serde::{Deserialize, Serialize};

/// 网关对一次供应商动作请求的处理结果分类（错误分类对齐 §6.21）。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
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

/// 对原供应商动作进行结果调查后的可证明结论。
///
/// “查询请求本身成功”不属于业务终态；适配器只有在供应商明确证明原请求没有
/// 形成结果时才能返回 [`Self::VerifiedNoResult`]，其余情况一律失败关闭为
/// [`Self::ResultUnknown`]。已落库的业务终态由 Service 在调用网关前独立复验。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "SCREAMING_SNAKE_CASE")]
pub enum InvestigationOutcome {
    /// 供应商明确证明原请求没有形成结果，可沿原供应商幂等键重放。
    VerifiedNoResult {
        /// 权限安全的查询证据摘要。
        summary: String,
    },
    /// 供应商没有返回足以证明原结果的证据。
    ResultUnknown {
        /// 权限安全的查询证据摘要。
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

    /// 查询原供应商动作结果，不把传输成功误判成业务成功。
    ///
    /// 实现必须使用原动作身份查询，不得创建新订单或改用新供应商幂等键。只有
    /// 外部系统明确返回“原请求未形成结果”时才能开放安全重放。
    fn investigate<'a>(
        &'a self,
        target_action: &'a SupplierOrderAction,
        order: &'a SupplierFulfillmentOrder,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = InvestigationOutcome> + Send + 'a>>;
}

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
                == entities::supplier_fulfillment::SupplierOrderActionType::Place)
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
    use entities::common::time::Instant;
    use entities::ids::{SupplierAccountId, SupplierApiConnectionId};
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
                error_class: entities::integration_ops::ErrorClass::CapabilityGap,
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
                error_class: entities::integration_ops::ErrorClass::TransientFailure,
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
        assert!(entities::money::Quantity::from_str("1.000000").is_ok());
        assert!(entities::money::Amount::from_str("9.99").is_ok());
    }
}
