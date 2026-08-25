//! 域 D29 共享枚举：事实类型、数据来源、处理状态、归集状态与成本口径（数据模型 §6.17）。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

/// 商城关键事实类型（数据模型 §6.17：五类成功结果事实）。
///
/// 商城只发送五类成功结果事实，不发送处理中事实（§9.4）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FactType {
    /// 支付已经成功。
    PaymentSucceeded,
    /// 订单已经取消。
    OrderCanceled,
    /// 退款已经成功。
    RefundSucceeded,
    /// 商城员工订单已经完成。
    OrderCompleted,
    /// 原卡余额已经恢复。
    CardBalanceRestored,
}

impl FactType {
    /// 返回事实类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PaymentSucceeded => "支付成功",
            Self::OrderCanceled => "订单取消",
            Self::RefundSucceeded => "退款成功",
            Self::OrderCompleted => "订单完成",
            Self::CardBalanceRestored => "卡券余额恢复",
        }
    }

    /// 返回事实类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PaymentSucceeded => "PAYMENT_SUCCEEDED",
            Self::OrderCanceled => "ORDER_CANCELED",
            Self::RefundSucceeded => "REFUND_SUCCEEDED",
            Self::OrderCompleted => "ORDER_COMPLETED",
            Self::CardBalanceRestored => "CARD_BALANCE_RESTORED",
        }
    }

    /// 判断该事实类型是否必须携带商城售后请求 ID。
    ///
    /// 数据模型 §6.17：取消、退款和余额恢复必须携带商城售后请求 ID。
    ///
    /// # 返回
    /// 取消/退款/余额恢复返回 `true`。
    pub fn requires_after_sales_request(&self) -> bool {
        matches!(
            self,
            Self::OrderCanceled | Self::RefundSucceeded | Self::CardBalanceRestored
        )
    }

    /// 判断该事实类型是否必须关联原支付事实。
    ///
    /// 数据模型 §6.17：取消、退款、完成和余额恢复必须关联原支付；支付成功本身没有。
    ///
    /// # 返回
    /// 除 `PaymentSucceeded` 外的类型返回 `true`。
    pub fn requires_original_payment(&self) -> bool {
        !matches!(self, Self::PaymentSucceeded)
    }

    /// 判断事实是否必须由商城售后域接收。
    ///
    /// # 返回
    /// 退款成功或卡券余额恢复返回 `true`，支付、取消与完成返回 `false`。
    pub fn is_after_sales_result(self) -> bool {
        matches!(self, Self::RefundSucceeded | Self::CardBalanceRestored)
    }

    /// 判断事实是否为支付成功事实。
    ///
    /// # 返回
    /// `PaymentSucceeded` 返回 `true`。
    pub fn is_payment_succeeded(self) -> bool {
        self == Self::PaymentSucceeded
    }
}

/// 关键事实数据来源（数据模型 §6.17：实时或历史回填）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DataSource {
    /// 实时回流。
    Realtime,
    /// 历史回填。
    HistoryBackfill,
}

impl DataSource {
    /// 返回数据来源的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Realtime => "实时",
            Self::HistoryBackfill => "历史回填",
        }
    }

    /// 返回数据来源的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Realtime => "realtime",
            Self::HistoryBackfill => "history_backfill",
        }
    }
}

/// 订单取消范围（数据模型 §6.17：整单或明细）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CancelScope {
    /// 整单取消。
    WholeOrder,
    /// 明细级取消。
    LineItem,
}

impl CancelScope {
    /// 返回取消范围的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::WholeOrder => "整单",
            Self::LineItem => "明细",
        }
    }

    /// 返回取消范围的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::WholeOrder => "whole_order",
            Self::LineItem => "line_item",
        }
    }
}

/// 履约链归属（数据模型 §6.17：`LEGACY_MANUAL` 或 `ERP_AUTOMATED`）。
///
/// 以支付成功事实的 `occurred_at` 与切换 `T` 比较派生，不以 ERP 接收或回填时间判断。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentChain {
    /// `T` 前支付：只记账，不自动创建供应商订单。
    LegacyManual,
    /// `T` 及以后支付：满足映射条件后进入供应商下单。
    ErpAutomated,
}

impl FulfillmentChain {
    /// 返回履约链的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::LegacyManual => "原人工履约链",
            Self::ErpAutomated => "ERP 自动履约链",
        }
    }

    /// 返回履约链的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::LegacyManual => "LEGACY_MANUAL",
            Self::ErpAutomated => "ERP_AUTOMATED",
        }
    }
}

/// 归集进度状态（数据模型 §6.17：待归集、已归集、差异）。
///
/// 固定邻接：待归集 → 已归集 | 差异；差异 → 待归集（条件补齐后重新归集，
/// 仍引用原业务事实键）；已归集为终态（后到的不同版本保存为差异，不覆盖原事实）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttributionStatus {
    /// 待归集：基础资料、卡实例或成本条件暂缺。
    PendingAttribution,
    /// 已归集。
    Attributed,
    /// 差异：版本冲突或归集失败，需人工处理。
    Difference,
}

impl AttributionStatus {
    /// 返回归集状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingAttribution => "待归集",
            Self::Attributed => "已归集",
            Self::Difference => "差异",
        }
    }

    /// 返回归集状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingAttribution => "pending_attribution",
            Self::Attributed => "attributed",
            Self::Difference => "difference",
        }
    }
}

impl DocumentState for AttributionStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::PendingAttribution => &[Self::Attributed, Self::Difference],
            Self::Attributed => &[],
            Self::Difference => &[Self::PendingAttribution],
        }
    }
}

/// 事实处理状态（数据模型 §6.17：已保存、待归集、已归集、差异、拒绝）。
///
/// 固定邻接：已保存 → 待归集 | 差异 | 拒绝；待归集 → 已归集 | 差异；
/// 差异 → 待归集 | 拒绝；已归集与拒绝为终态。归集条件缺失时保留事实并进入差异，
/// 不拒收、不复制第二份事实（§6.17）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProcessingStatus {
    /// 已保存：通过完整性校验的原始事实已落库。
    Saved,
    /// 待归集：等待商品、卡实例、成本和供应商归集。
    PendingAttribution,
    /// 已归集。
    Attributed,
    /// 差异：归集条件缺失或版本冲突。
    Difference,
    /// 拒绝：未通过基本完整性校验。
    Rejected,
}

impl ProcessingStatus {
    /// 返回处理状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Saved => "已保存",
            Self::PendingAttribution => "待归集",
            Self::Attributed => "已归集",
            Self::Difference => "差异",
            Self::Rejected => "拒绝",
        }
    }

    /// 返回处理状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Saved => "saved",
            Self::PendingAttribution => "pending_attribution",
            Self::Attributed => "attributed",
            Self::Difference => "difference",
            Self::Rejected => "rejected",
        }
    }
}

impl DocumentState for ProcessingStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Saved => &[Self::PendingAttribution, Self::Difference, Self::Rejected],
            Self::PendingAttribution => &[Self::Attributed, Self::Difference],
            Self::Difference => &[Self::PendingAttribution, Self::Rejected],
            Self::Attributed => &[],
            Self::Rejected => &[],
        }
    }
}

/// 成本口径（数据模型 §6.17 消费成本评估、§6.17 回填：`ACTUAL`、`STANDARD`、`NONE`）。
///
/// 每笔消费独立标记；`NONE` 消费进入消费额和覆盖率分母，不进入任何利润指标（§9.3）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CostBasis {
    /// 实际成本：商城订单快照成本或供应商实际结算。
    Actual,
    /// 标准成本：消费时点供给版本价。
    Standard,
    /// 无成本：不记录成本金额。
    None,
}

impl CostBasis {
    /// 返回成本口径的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Actual => "实际成本",
            Self::Standard => "标准成本",
            Self::None => "无成本",
        }
    }

    /// 返回成本口径的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Actual => "ACTUAL",
            Self::Standard => "STANDARD",
            Self::None => "NONE",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::FactType;

    #[test]
    fn fact_type_routes_after_sales_and_payment_semantics() {
        assert!(FactType::RefundSucceeded.is_after_sales_result());
        assert!(FactType::CardBalanceRestored.is_after_sales_result());
        assert!(!FactType::OrderCanceled.is_after_sales_result());
        assert!(FactType::PaymentSucceeded.is_payment_succeeded());
        assert!(!FactType::OrderCompleted.is_payment_succeeded());
    }
}
