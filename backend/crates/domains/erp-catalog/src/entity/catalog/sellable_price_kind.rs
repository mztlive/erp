//! 公司商品池价格筛选口径。

use serde::{Deserialize, Serialize};

/// 单个价格区间只能匹配一种公司含税参考价。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SellablePriceKind {
    /// 一件代发价；未指定类型时沿用此口径。
    #[default]
    Dropship,
    /// 集采价。
    Bulk,
    /// 市场价。
    Market,
}
