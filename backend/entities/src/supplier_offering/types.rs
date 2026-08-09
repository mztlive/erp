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
    use super::{AvailabilityStatus, OfferingSourceType, OfferingStatus};

    #[test]
    fn codes_and_labels_are_stable() {
        assert_eq!(OfferingSourceType::Api.as_str(), "API");
        assert_eq!(OfferingStatus::Paused.label(), "暂停");
        assert_eq!(AvailabilityStatus::Stale.as_str(), "STALE");
        assert_eq!(
            serde_json::to_string(&OfferingStatus::Active).unwrap(),
            "\"ACTIVE\""
        );
    }
}
