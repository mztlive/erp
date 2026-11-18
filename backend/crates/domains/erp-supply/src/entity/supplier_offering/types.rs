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
    /// 取得供给登记来源的稳定标识，供需要按代码而非展示文案识别取值的调用方使用。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Excel => "EXCEL",
            Self::Api => "API",
            Self::Manual => "MANUAL",
        }
    }

    /// 为供给登记来源提供中文展示名；界面展示时使用，不以文案作为稳定标识。
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
    /// 停供和暂停分别返回停止供应与当前不可供；启用返回空。
    /// 读取当前关系状态。
    pub fn interruption_reason(self) -> Option<AvailabilityInterruptionReason> {
        match self {
            Self::Active => None,
            Self::Paused => Some(AvailabilityInterruptionReason::SupplyUnavailable),
            Self::Stopped => Some(AvailabilityInterruptionReason::SupplierStopped),
        }
    }

    /// 取得供给关系状态的稳定标识，供需要按代码而非展示文案识别取值的调用方使用。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Active => "ACTIVE",
            Self::Paused => "PAUSED",
            Self::Stopped => "STOPPED",
        }
    }

    /// 为供给关系状态提供中文展示名；界面展示时使用，不以文案作为稳定标识。
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
    /// 取得可供状态的稳定标识，供需要按代码而非展示文案识别取值的调用方使用。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Available => "AVAILABLE",
            Self::Unavailable => "UNAVAILABLE",
            Self::Stopped => "STOPPED",
            Self::Stale => "STALE",
        }
    }

    /// 为可供状态提供中文展示名；界面展示时使用，不以文案作为稳定标识。
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
