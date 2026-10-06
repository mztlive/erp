//! 供应商供给域的状态与来源类型。

use serde::{Deserialize, Serialize};

/// 供给身份的录入来源。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OfferingSourceType {
    /// Excel 批量登记。
    Excel,
    /// 供应商 API 同步。
    Api,
    /// 管理台手工登记。
    Manual,
}

impl OfferingSourceType {
    /// 返回持久化与查询使用的稳定代码。
    ///
    /// # 返回
    /// 返回大写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Excel => "EXCEL",
            Self::Api => "API",
            Self::Manual => "MANUAL",
        }
    }

    /// 返回面向用户的中文标签。
    ///
    /// # 返回
    /// 返回来源标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Excel => "Excel",
            Self::Api => "API",
            Self::Manual => "手工",
        }
    }
}

/// 供给关系状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum OfferingStatus {
    /// 可参与采购选源。
    Active,
    /// 暂时不参与采购选源。
    Paused,
    /// 已停止合作。
    Stopped,
}

impl OfferingStatus {
    /// 返回关系暂停或停供导致履约受阻的正式领域原因。
    /// # 参数
    /// 无；读取当前关系状态。
    /// # 返回
    /// 停供和暂停分别返回停止供应与当前不可供；启用返回空。
    /// # 错误
    /// 无。
    pub fn interruption_reason(self) -> Option<AvailabilityInterruptionReason> {
        match self {
            Self::Active => None,
            Self::Paused => Some(AvailabilityInterruptionReason::SupplyUnavailable),
            Self::Stopped => Some(AvailabilityInterruptionReason::SupplierStopped),
        }
    }

    /// 返回持久化与查询使用的稳定代码。
    ///
    /// # 返回
    /// 返回大写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Paused => "PAUSED",
            Self::Stopped => "STOPPED",
        }
    }

    /// 返回面向用户的中文标签。
    ///
    /// # 返回
    /// 返回状态标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Paused => "暂停",
            Self::Stopped => "停止",
        }
    }
}

/// 商业条款修订对当前销售安全性的影响分类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OfferingRevisionImpact {
    /// 不影响销售安全确认。
    None,
    /// 供给成本发生变化，需要重新确认成本。
    CostChanged,
    /// MOQ、区域、能力、履约说明或有效期发生关键变化。
    CriticalSupplyChanged,
}

/// 实时可供投影导致销售安全暂停的领域原因。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AvailabilityInterruptionReason {
    /// 供应商明确停止供应。
    SupplierStopped,
    /// 当前不可供。
    SupplyUnavailable,
    /// 可供数据超过新鲜度阈值。
    AvailabilityStale,
    /// 状态可供但数量已经耗尽。
    ZeroInventory,
}

/// 供给的实时可供状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum AvailabilityStatus {
    /// 当前可供。
    Available,
    /// 当前不可供。
    Unavailable,
    /// 供应商明确停止供应。
    Stopped,
    /// 来源超过新鲜度阈值。
    Stale,
}

impl AvailabilityStatus {
    /// 返回持久化与查询使用的稳定代码。
    ///
    /// # 返回
    /// 返回大写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "AVAILABLE",
            Self::Unavailable => "UNAVAILABLE",
            Self::Stopped => "STOPPED",
            Self::Stale => "STALE",
        }
    }

    /// 返回面向用户的中文标签。
    ///
    /// # 返回
    /// 返回状态标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::Available => "可供",
            Self::Unavailable => "不可供",
            Self::Stopped => "停止供应",
            Self::Stale => "数据已过期",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{AvailabilityInterruptionReason, AvailabilityStatus, OfferingSourceType, OfferingStatus};

    #[test]
    fn relationship_pause_and_stop_block_supply_even_when_quantity_is_available() {
        assert_eq!(OfferingStatus::Active.interruption_reason(), None);
        assert_eq!(
            OfferingStatus::Paused.interruption_reason(),
            Some(AvailabilityInterruptionReason::SupplyUnavailable)
        );
        assert_eq!(
            OfferingStatus::Stopped.interruption_reason(),
            Some(AvailabilityInterruptionReason::SupplierStopped)
        );
    }

    #[test]
    fn codes_and_labels_are_stable() {
        assert_eq!(OfferingSourceType::Api.as_str(), "API");
        assert_eq!(OfferingStatus::Paused.label(), "暂停");
        assert_eq!(AvailabilityStatus::Stale.as_str(), "STALE");
        assert_eq!(serde_json::to_string(&OfferingStatus::Active).unwrap(), "\"ACTIVE\"");
    }
}
