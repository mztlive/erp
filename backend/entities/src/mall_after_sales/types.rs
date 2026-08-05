//! 域 D30 共享枚举：售后请求类型、售后状态机、明细行状态与退款分配动作
//! （数据模型 §6.18）。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

/// 售后请求类型（数据模型 §6.18：取消或退款）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterSalesRequestType {
    /// 取消。
    Cancel,
    /// 退款。
    Refund,
}

impl AfterSalesRequestType {
    /// 返回请求类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Cancel => "取消",
            Self::Refund => "退款",
        }
    }

    /// 返回请求类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Cancel => "cancel",
            Self::Refund => "refund",
        }
    }
}

/// 售后请求状态（数据模型 §6.18：已接收、供应商处理中、部分完成、退款处理中、
/// 待人工、已关闭）。
///
/// 固定邻接：已接收 → 供应商处理中 | 退款处理中 | 待人工；供应商处理中 →
/// 部分完成 | 退款处理中 | 待人工；部分完成 → 供应商处理中 | 退款处理中 |
/// 待人工 | 已关闭；退款处理中 → 待人工 | 已关闭；待人工 → 供应商处理中 |
/// 退款处理中 | 已关闭；已关闭为不可逆终态。
///
/// 关闭条件从适用事实派生（§6.18）：商城取消/退款结果已到达、卡券来源已完成余额
/// 恢复或微信来源已完成退款，且适用供应商退款、成本冲减和应付冲减均完成；
/// 任一适用环节未完成时不得手工直接标记已关闭。该派生依赖聚合查询，
/// 由 P3 落实（P3 条目：§6.18 关闭条件派生）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterSalesRequestStatus {
    /// 已接收：商城售后申请已幂等接收。
    Received,
    /// 供应商处理中：正在驱动供应商售后动作。
    SupplierProcessing,
    /// 部分完成：多个供应商部分处理完成。
    PartiallyCompleted,
    /// 退款处理中：商城退款/余额恢复/微信退款处理中。
    RefundProcessing,
    /// 待人工：需人工处理（异常或结果未知）。
    ManualNeeded,
    /// 已关闭：全部适用环节完成（不可逆终态）。
    Closed,
}

impl AfterSalesRequestStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Received => "已接收",
            Self::SupplierProcessing => "供应商处理中",
            Self::PartiallyCompleted => "部分完成",
            Self::RefundProcessing => "退款处理中",
            Self::ManualNeeded => "待人工",
            Self::Closed => "已关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Received => "received",
            Self::SupplierProcessing => "supplier_processing",
            Self::PartiallyCompleted => "partially_completed",
            Self::RefundProcessing => "refund_processing",
            Self::ManualNeeded => "manual_needed",
            Self::Closed => "closed",
        }
    }
}

impl DocumentState for AfterSalesRequestStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Received => &[
                Self::SupplierProcessing,
                Self::RefundProcessing,
                Self::ManualNeeded,
            ],
            Self::SupplierProcessing => &[
                Self::PartiallyCompleted,
                Self::RefundProcessing,
                Self::ManualNeeded,
            ],
            Self::PartiallyCompleted => &[
                Self::SupplierProcessing,
                Self::RefundProcessing,
                Self::ManualNeeded,
                Self::Closed,
            ],
            Self::RefundProcessing => &[Self::ManualNeeded, Self::Closed],
            Self::ManualNeeded => &[Self::SupplierProcessing, Self::RefundProcessing, Self::Closed],
            Self::Closed => &[],
        }
    }
}

/// 售后明细行状态（数据模型 §6.18：待处理、供应商接受、供应商拒绝、退款处理中、
/// 已完成、待人工）。
///
/// 固定枚举：数据模型未定义行级状态迁移邻接矩阵，行状态推进由 P3 服务按供应商
/// 动作结果与事实回流派生写入（P3 条目：§6.18 行状态派生）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AfterSalesLineStatus {
    /// 待处理。
    Pending,
    /// 供应商接受。
    SupplierAccepted,
    /// 供应商拒绝。
    SupplierRejected,
    /// 退款处理中。
    RefundProcessing,
    /// 已完成。
    Completed,
    /// 待人工。
    ManualNeeded,
}

impl AfterSalesLineStatus {
    /// 返回行状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::SupplierAccepted => "供应商接受",
            Self::SupplierRejected => "供应商拒绝",
            Self::RefundProcessing => "退款处理中",
            Self::Completed => "已完成",
            Self::ManualNeeded => "待人工",
        }
    }

    /// 返回行状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::SupplierAccepted => "supplier_accepted",
            Self::SupplierRejected => "supplier_rejected",
            Self::RefundProcessing => "refund_processing",
            Self::Completed => "completed",
            Self::ManualNeeded => "manual_needed",
        }
    }
}

/// 退款分配动作（数据模型 §6.18：`APPLY` 或 `REVERSE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllocationAction {
    /// 正向分配：冲减原消费与支付来源分摊。
    Apply,
    /// 反向分配：等额冲销错误的 `APPLY` 分配。
    Reverse,
}

impl AllocationAction {
    /// 返回动作的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Apply => "正向分配",
            Self::Reverse => "反向冲销",
        }
    }

    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apply => "APPLY",
            Self::Reverse => "REVERSE",
        }
    }

    /// 判断是否为反向冲销。
    ///
    /// # 返回
    /// 动作为 `Reverse` 时返回 `true`。
    pub fn is_reverse(&self) -> bool {
        matches!(self, Self::Reverse)
    }
}
