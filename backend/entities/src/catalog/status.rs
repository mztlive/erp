//! 域内固定启停状态与状态机（数据模型 §6.3 各表 `status`：启用、停用）。
//!
//! 状态机：`Active ↔ Disabled` 双向迁移（对称状态机，可用
//! [`crate::common::state::assert_adjacency_closed`] 验证闭包）；
//! 数据模型第 7 章未定义本域文档状态机，第 13.3 条要求邻接矩阵固化、
//! 禁止运行时扩展。

use serde::{Deserialize, Serialize};

use crate::common::state::DocumentState;

/// 启用/停用状态（数据模型 §6.3：启用、停用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnableStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
}

impl EnableStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
        }
    }

    /// 判断是否处于启用状态。
    ///
    /// # 返回
    /// 处于 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

impl DocumentState for EnableStatus {
    /// 返回合法后继状态：启用 ↔ 停用 双向可迁移。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Active => &[Self::Disabled],
            Self::Disabled => &[Self::Active],
        }
    }
}

/// SKU 上架状态。
///
/// 上架只决定 SKU 是否进入公司商品池，不替代主数据的启用/停用状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ListingStatus {
    /// 已上架，可在满足供给等其他资格后进入公司商品池。
    Listed,
    /// 已下架。
    #[default]
    Unlisted,
}

impl ListingStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的上架状态标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Listed => "已上架",
            Self::Unlisted => "已下架",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Listed => "listed",
            Self::Unlisted => "unlisted",
        }
    }

    /// 判断是否已上架。
    ///
    /// # 返回
    /// 状态为 `Listed` 时返回 `true`。
    pub fn is_listed(&self) -> bool {
        matches!(self, Self::Listed)
    }
}

/// SPU 从当前启用 SKU 继承得到的上架状态。
///
/// 该状态只做派生展示，不在 `product` 集合重复持久化。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ProductListingStatus {
    /// 当前启用 SKU 全部已上架。
    Listed,
    /// 当前启用 SKU 中仅有部分已上架。
    PartiallyListed,
    /// 当前没有已上架 SKU。
    #[default]
    Unlisted,
}

impl ProductListingStatus {
    /// 按已上架数量与当前启用 SKU 总数计算 SPU 继承状态。
    ///
    /// # 参数
    /// * `listed_sku_count` - 当前已上架 SKU 数
    /// * `sku_count` - 当前启用 SKU 总数
    ///
    /// # 返回
    /// 返回全上架、部分上架或全下架状态。
    pub fn inherited(listed_sku_count: u32, sku_count: u32) -> Self {
        if sku_count > 0 && listed_sku_count == sku_count {
            return Self::Listed;
        }
        if listed_sku_count > 0 {
            return Self::PartiallyListed;
        }
        Self::Unlisted
    }

    /// 返回用于查询与传输的稳定代码。
    ///
    /// # 返回
    /// 返回 `listed`、`partially_listed` 或 `unlisted`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Listed => "listed",
            Self::PartiallyListed => "partially_listed",
            Self::Unlisted => "unlisted",
        }
    }
}

/// 当前启用 SKU 在某一资料维度上的覆盖状态。
///
/// 没有启用 SKU 时按 [`Self::None`] 处理，避免把空集合误判为完整。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SkuCoverageStatus {
    /// 每个当前启用 SKU 都具备目标资料。
    Complete,
    /// 仅部分当前启用 SKU 具备目标资料。
    Partial,
    /// 没有当前启用 SKU 具备目标资料，或商品没有当前启用 SKU。
    #[default]
    None,
}

impl SkuCoverageStatus {
    /// 按已覆盖数量与当前启用 SKU 总数计算覆盖状态。
    ///
    /// # 参数
    /// * `covered_sku_count` - 已具备目标资料的 SKU 数
    /// * `sku_count` - 当前启用 SKU 总数
    ///
    /// # 返回
    /// 返回完整、部分或无覆盖状态。
    pub fn inherited(covered_sku_count: u32, sku_count: u32) -> Self {
        if sku_count > 0 && covered_sku_count == sku_count {
            return Self::Complete;
        }
        if covered_sku_count > 0 {
            return Self::Partial;
        }
        Self::None
    }

    /// 返回用于查询与传输的稳定代码。
    ///
    /// # 返回
    /// 返回 `complete`、`partial` 或 `none`。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Complete => "complete",
            Self::Partial => "partial",
            Self::None => "none",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};

    /// 启用/停用双向迁移与幂等迁移合法，邻接矩阵对称闭合。
    #[test]
    fn enable_status_adjacency_is_closed() {
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);

        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Active).is_ok());
    }

    /// 状态代码与中文标签。
    #[test]
    fn enable_status_exposes_labels_and_codes() {
        assert_eq!(EnableStatus::Active.label(), "启用");
        assert_eq!(EnableStatus::Disabled.label(), "停用");
        assert_eq!(
            serde_json::to_string(&EnableStatus::Active).unwrap(),
            "\"active\""
        );
        assert_eq!(EnableStatus::Disabled.as_str(), "disabled");
        assert!(EnableStatus::Active.is_active());
        assert!(!EnableStatus::Disabled.is_active());
    }

    /// SKU 上架代码稳定，缺省状态为下架。
    #[test]
    fn listing_status_defaults_to_unlisted() {
        assert_eq!(ListingStatus::default(), ListingStatus::Unlisted);
        assert_eq!(ListingStatus::Listed.as_str(), "listed");
        assert_eq!(ListingStatus::Unlisted.label(), "已下架");
        assert!(ListingStatus::Listed.is_listed());
    }

    /// SPU 上架状态完全由当前启用 SKU 的上架数量继承。
    #[test]
    fn product_listing_status_is_inherited_from_sku_counts() {
        assert_eq!(
            ProductListingStatus::inherited(2, 2),
            ProductListingStatus::Listed
        );
        assert_eq!(
            ProductListingStatus::inherited(1, 2),
            ProductListingStatus::PartiallyListed
        );
        assert_eq!(
            ProductListingStatus::inherited(0, 2),
            ProductListingStatus::Unlisted
        );
        assert_eq!(
            ProductListingStatus::inherited(0, 0),
            ProductListingStatus::Unlisted
        );
        assert_eq!(ProductListingStatus::PartiallyListed.as_str(), "partially_listed");
    }

    /// SKU 资料覆盖状态必须区分完整、部分与无覆盖，空集合不能视为完整。
    #[test]
    fn sku_coverage_status_is_inherited_from_counts() {
        assert_eq!(SkuCoverageStatus::inherited(2, 2), SkuCoverageStatus::Complete);
        assert_eq!(SkuCoverageStatus::inherited(1, 2), SkuCoverageStatus::Partial);
        assert_eq!(SkuCoverageStatus::inherited(0, 2), SkuCoverageStatus::None);
        assert_eq!(SkuCoverageStatus::inherited(0, 0), SkuCoverageStatus::None);
        assert_eq!(SkuCoverageStatus::Complete.as_str(), "complete");
    }
}
