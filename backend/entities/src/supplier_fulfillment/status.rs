//! 供应商履约的三条正交状态机（数据模型 §6.19、§7.6）：履约主线、取消与退款。
//!
//! `CANCEL_PENDING`/`CANCELED`/`REFUND_PENDING`/`REFUNDED` 不得折叠为单一状态枚举
//! （§6.19）；`COMPLETED`/`REJECTED` 是终态，乱序或重复回调经
//! [`crate::common::state::ensure_transition`] 拒绝（从高状态回低状态即非法迁移）。
//! 本模块只承载状态定义与邻接矩阵，不引用订单实体（避免与 `fulfillment_order` 循环依赖）。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

/// 可由订单与原供应商动作共同证明的业务终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum VerifiedSupplierOrderResolution {
    /// 供应商已接单。
    OrderAccepted,
    /// 供应商明确拒单。
    OrderRejected,
    /// 供应商履约完成。
    OrderCompleted,
    /// 供应商取消完成。
    Canceled,
    /// 供应商退款完成。
    Refunded,
}

/// 供应商履约主线状态（数据模型 §6.19、§7.6）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentStatus {
    /// 接收：已形成子订单，尚未提交供应商。
    Received,
    /// 提交中：下单请求已发出，等待接单结果。
    Submitting,
    /// 已接单：供应商明确接单。
    Accepted,
    /// 明确拒绝：供应商明确拒单，终态；退款/余额恢复闭环由 refund_status 与退款事实表达。
    Rejected,
    /// 结果未知：网络超时或查询能力不足，先查询原请求，不盲目重复下单。
    ResultUnknown,
    /// 履约中：供应商履约中（含已发货）。
    Fulfilling,
    /// 已发货：适用时（存在发货阶段的履约）。
    Shipped,
    /// 已完成：终态。
    Completed,
    /// 异常：需人工查询或补偿；恢复动作以追加事实表达，不直接改状态。
    Exception,
}

impl FulfillmentStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Received => "接收",
            Self::Submitting => "提交中",
            Self::Accepted => "已接单",
            Self::Rejected => "明确拒绝",
            Self::ResultUnknown => "结果未知",
            Self::Fulfilling => "履约中",
            Self::Shipped => "已发货",
            Self::Completed => "已完成",
            Self::Exception => "异常",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "RECEIVED",
            Self::Submitting => "SUBMITTING",
            Self::Accepted => "ACCEPTED",
            Self::Rejected => "REJECTED",
            Self::ResultUnknown => "RESULT_UNKNOWN",
            Self::Fulfilling => "FULFILLING",
            Self::Shipped => "SHIPPED",
            Self::Completed => "COMPLETED",
            Self::Exception => "EXCEPTION",
        }
    }

    /// 判断是否处于终态。
    ///
    /// # 返回
    /// `COMPLETED`、`REJECTED` 或 `EXCEPTION` 时返回 `true`。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Completed | Self::Rejected | Self::Exception)
    }
}

impl DocumentState for FulfillmentStatus {
    /// 返回全部合法后继状态（数据模型 §7.6，禁止运行时扩展）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Received => &[Self::Submitting, Self::Exception],
            Self::Submitting => &[
                Self::Accepted,
                Self::Rejected,
                Self::ResultUnknown,
                Self::Exception,
            ],
            Self::Accepted => &[Self::Fulfilling, Self::Exception],
            Self::Fulfilling => &[Self::Shipped, Self::Completed, Self::Exception],
            Self::Shipped => &[Self::Completed, Self::Exception],
            Self::ResultUnknown => &[Self::Accepted, Self::Rejected, Self::Exception],
            Self::Completed | Self::Rejected | Self::Exception => &[],
        }
    }
}

/// 供应商履约取消进度状态（数据模型 §6.19、§7.6，独立于履约主线的正交状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CancelStatus {
    /// 无。
    None,
    /// 取消中。
    CancelPending,
    /// 已取消：终态。
    Canceled,
    /// 取消失败：终态。
    Failed,
    /// 待人工：终态。
    Manual,
}

impl CancelStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "无",
            Self::CancelPending => "取消中",
            Self::Canceled => "已取消",
            Self::Failed => "取消失败",
            Self::Manual => "待人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::CancelPending => "CANCEL_PENDING",
            Self::Canceled => "CANCELED",
            Self::Failed => "FAILED",
            Self::Manual => "MANUAL",
        }
    }
}

impl DocumentState for CancelStatus {
    /// 返回全部合法后继状态（数据模型 §7.6：NONE→CANCEL_PENDING→CANCELED|FAILED|MANUAL）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::None => &[Self::CancelPending],
            Self::CancelPending => &[Self::Canceled, Self::Failed, Self::Manual],
            Self::Canceled | Self::Failed | Self::Manual => &[],
        }
    }
}

/// 供应商履约退款进度状态（数据模型 §6.19、§7.6，独立于履约主线的正交状态）。
///
/// 主线 `NONE → REFUND_PENDING → PARTIAL → REFUNDED`；`REFUND_PENDING` 可分支到
/// `REFUND_FAILED`/`MANUAL`。多次部分退款表达为 `PARTIAL` 的幂等停留（新退款请求的
/// 在途状态由 `supplier_order_action` 的 REFUND 动作承载），`PARTIAL` 之后的退款失败
/// 进入 `REFUND_FAILED`、人工接手进入 `MANUAL`。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum RefundStatus {
    /// 无。
    None,
    /// 退款中。
    RefundPending,
    /// 部分退款。
    Partial,
    /// 全部退款：终态。
    Refunded,
    /// 退款失败：终态。
    RefundFailed,
    /// 待人工：终态。
    Manual,
}

impl RefundStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::None => "无",
            Self::RefundPending => "退款中",
            Self::Partial => "部分退款",
            Self::Refunded => "全部退款",
            Self::RefundFailed => "退款失败",
            Self::Manual => "待人工",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::None => "NONE",
            Self::RefundPending => "REFUND_PENDING",
            Self::Partial => "PARTIAL",
            Self::Refunded => "REFUNDED",
            Self::RefundFailed => "REFUND_FAILED",
            Self::Manual => "MANUAL",
        }
    }
}

impl DocumentState for RefundStatus {
    /// 返回全部合法后继状态（数据模型 §7.6，禁止运行时扩展）。
    ///
    /// # 返回
    /// 后继状态切片（不含自身；终态返回空）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::None => &[Self::RefundPending],
            Self::RefundPending => &[Self::Partial, Self::Refunded, Self::RefundFailed, Self::Manual],
            Self::Partial => &[Self::Refunded, Self::RefundFailed, Self::Manual],
            Self::Refunded | Self::RefundFailed | Self::Manual => &[],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::errors::Error;

    #[test]
    fn terminal_states_are_absorbing() {
        assert!(FulfillmentStatus::Rejected.allowed_next().is_empty());
        assert!(FulfillmentStatus::Completed.allowed_next().is_empty());
        assert!(FulfillmentStatus::Exception.allowed_next().is_empty());
        assert!(FulfillmentStatus::Rejected.is_terminal());
        assert!(FulfillmentStatus::Completed.is_terminal());

        for (from, to) in [
            (FulfillmentStatus::Completed, FulfillmentStatus::Fulfilling),
            (FulfillmentStatus::Completed, FulfillmentStatus::Exception),
            (FulfillmentStatus::Rejected, FulfillmentStatus::Received),
            (FulfillmentStatus::Exception, FulfillmentStatus::Accepted),
        ] {
            let error = ensure_transition(from, to).unwrap_err();
            assert!(
                matches!(error, Error::InvalidStateTransition { .. }),
                "终态定向断言：{from:?} → {to:?} 必须被拒绝"
            );
        }
    }

    #[test]
    fn exception_branches_and_result_unknown_resolution() {
        for start in [
            FulfillmentStatus::Received,
            FulfillmentStatus::Submitting,
            FulfillmentStatus::Accepted,
            FulfillmentStatus::Fulfilling,
            FulfillmentStatus::Shipped,
            FulfillmentStatus::ResultUnknown,
        ] {
            assert!(
                ensure_transition(start, FulfillmentStatus::Exception).is_ok(),
                "任一可恢复节点可进入 EXCEPTION：{start:?}"
            );
        }

        assert!(ensure_transition(FulfillmentStatus::Submitting, FulfillmentStatus::ResultUnknown).is_ok());
        for to in [
            FulfillmentStatus::Accepted,
            FulfillmentStatus::Rejected,
            FulfillmentStatus::Exception,
        ] {
            assert!(
                ensure_transition(FulfillmentStatus::ResultUnknown, to).is_ok(),
                "RESULT_UNKNOWN 可解析为 {to:?}"
            );
        }
        assert!(ensure_transition(FulfillmentStatus::Submitting, FulfillmentStatus::Rejected).is_ok());
    }

    #[test]
    fn cancel_status_adjacency_is_fixed() {
        assert!(
            ensure_transition(CancelStatus::None, CancelStatus::Canceled).is_err(),
            "跳过 CANCEL_PENDING 非法"
        );
        assert!(
            ensure_transition(CancelStatus::Canceled, CancelStatus::Manual).is_err(),
            "CANCELED 是终态"
        );
        assert!(
            ensure_transition(CancelStatus::Canceled, CancelStatus::None).is_err(),
            "取消进度不得倒退"
        );
    }

    #[test]
    fn refund_status_adjacency_is_fixed() {
        assert!(
            ensure_transition(RefundStatus::None, RefundStatus::Refunded).is_err(),
            "跳过 REFUND_PENDING 非法"
        );
        assert!(
            ensure_transition(RefundStatus::Refunded, RefundStatus::Partial).is_err(),
            "REFUNDED 是终态"
        );
        assert!(
            ensure_transition(RefundStatus::RefundFailed, RefundStatus::None).is_err(),
            "退款进度不得倒退"
        );
    }
}
