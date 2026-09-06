//! 商品业务类型（数据模型 §6.3：`PHYSICAL`、`VIRTUAL`、`OFFLINE_SERVICE`、`VOUCHER`）。

use serde::{Deserialize, Serialize};

/// 商品业务类型。
///
/// `product.product_kind` 是必填的独立稳定业务属性，创建后不可变、不得由分类派生
/// （数据模型 §6.3）；`product_category.product_kind` 只用于兼容性校验和筛选，
/// 不是公司商品类型的事实来源。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "UPPERCASE")]
pub enum ProductKind {
    /// 实物商品。
    Physical,
    /// 虚拟商品。
    Virtual,
    /// 线下服务。
    #[serde(rename = "OFFLINE_SERVICE")]
    OfflineService,
    /// 卡券类目。
    Voucher,
}

impl ProductKind {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Physical => "实物",
            Self::Virtual => "虚拟",
            Self::OfflineService => "线下服务",
            Self::Voucher => "卡券",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Physical => "PHYSICAL",
            Self::Virtual => "VIRTUAL",
            Self::OfflineService => "OFFLINE_SERVICE",
            Self::Voucher => "VOUCHER",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// serde 形态与中文标签。
    #[test]
    fn product_kind_exposes_labels_and_stable_codes() {
        assert_eq!(
            serde_json::to_string(&ProductKind::OfflineService).unwrap(),
            "\"OFFLINE_SERVICE\""
        );
        assert_eq!(
            serde_json::to_string(&ProductKind::Voucher).unwrap(),
            "\"VOUCHER\""
        );
        assert_eq!(ProductKind::Physical.label(), "实物");
        assert_eq!(ProductKind::Virtual.label(), "虚拟");
        assert_eq!(ProductKind::OfflineService.label(), "线下服务");
        assert_eq!(ProductKind::Voucher.label(), "卡券");
        assert_eq!(ProductKind::Voucher.as_str(), "VOUCHER");
    }
}
