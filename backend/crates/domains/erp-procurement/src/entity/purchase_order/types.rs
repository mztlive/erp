//! 域内共享固定枚举（数据模型 §6.6：采购类型、履约责任、行类型、价格依据）。

use erp_core::delivery_policy::DeliveryMode;
use serde::{Deserialize, Serialize};

/// 由 `(变体 => (中文标签, 稳定代码))` 映射表生成 `label()`/`as_str()`。
///
/// 各状态与固定枚举只声明一行数据的映射表，不再手写两套平行 `match`；
/// 新增变体只加一行，避免标签与代码漂移。生成的方法签名与手写版本一致。
macro_rules! status_display {
    ($type:ty, { $($variant:ident => ($label:literal, $code:literal)),* $(,)? }) => {
        impl $type {
            /// 返回中文展示名。
            ///
            /// # 参数
            /// 无。
            ///
            /// # 返回
            /// 返回面向用户的中文标签。
            ///
            /// # 错误
            /// 不返回错误。
            pub fn label(&self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                }
            }

            /// 返回稳定代码。
            ///
            /// # 参数
            /// 无。
            ///
            /// # 返回
            /// 返回用于持久化与查询的稳定字符串。
            ///
            /// # 错误
            /// 不返回错误。
            pub fn as_str(&self) -> &'static str {
                match self {
                    $(Self::$variant => $code,)*
                }
            }
        }
    };
}

pub(crate) use status_display;

/// 采购类型（§6.6：实物、虚拟、线下服务）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseType {
    /// 实物商品。
    Physical,
    /// 虚拟商品。
    Virtual,
    /// 线下服务。
    Service,
    /// 附加费用：只含费用行，不承担履约（§7.5.1）。
    Expense,
}

status_display!(PurchaseType, {
    Physical => ("实物", "PHYSICAL"),
    Virtual => ("虚拟", "VIRTUAL"),
    Service => ("线下服务", "SERVICE"),
    Expense => ("费用", "EXPENSE"),
});

/// 履约责任（§6.6：入仓、供应商直发、电子交付、线下服务）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FulfillmentResponsibility {
    /// 入仓（自有仓库）。
    Warehouse,
    /// 供应商直发。
    #[serde(rename = "SUPPLIER_DIRECT")]
    SupplierDirect,
    /// 电子交付。
    Electronic,
    /// 线下服务。
    Service,
    /// 无需履约：费用采购单，不生成入库、发货、交付或服务任务。
    #[serde(rename = "NONE")]
    NotApplicable,
}

status_display!(FulfillmentResponsibility, {
    Warehouse => ("入仓", "WAREHOUSE"),
    SupplierDirect => ("供应商直发", "SUPPLIER_DIRECT"),
    Electronic => ("电子交付", "ELECTRONIC"),
    Service => ("线下服务", "SERVICE"),
    NotApplicable => ("无需履约", "NONE"),
});

impl FulfillmentResponsibility {
    /// 返回采购单责任人后续直接执行的履约对象类型。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 供应商直发、电子交付和线下服务返回对应履约对象；入仓由仓库入库经办人负责，返回 `None`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 业务约束
    /// 该映射只描述采购单责任人承担的履约操作，不把仓库入库错误归给采购责任人。
    pub fn owner_fulfillment_object_type(self) -> Option<&'static str> {
        match self {
            Self::Warehouse | Self::NotApplicable => None,
            Self::SupplierDirect => Some("delivery"),
            Self::Electronic => Some("electronic_delivery"),
            Self::Service => Some("service_fulfillment"),
        }
    }
}

/// 采购行类型（§6.6：商品/服务成本或物流费用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum PurchaseLineType {
    /// 商品/服务成本行。
    #[serde(rename = "ITEM_SERVICE")]
    ItemService,
    /// 费用行（物流等附加费用，与商品成本分开计税，数量为空）；沿用历史存储代码。
    #[serde(rename = "LOGISTICS_FEE")]
    LogisticsFee,
}

status_display!(PurchaseLineType, {
    ItemService => ("商品/服务成本", "ITEM_SERVICE"),
    LogisticsFee => ("费用", "LOGISTICS_FEE"),
});

/// 采购单价依据：一件代发价或集采价。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum SupplyPriceBasis {
    /// 一件代发价，价格已含运费。
    Dropship,
    /// 集采价。
    Bulk,
}

status_display!(SupplyPriceBasis, {
    Dropship => ("代发", "DROPSHIP"),
    Bulk => ("集采", "BULK"),
});

impl SupplyPriceBasis {
    /// 返回配送判定使用的供货方式。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 代发返回代发规则，集采返回集采规则。
    ///
    /// # 错误
    /// 无。
    pub fn delivery_mode(self) -> DeliveryMode {
        match self {
            Self::Dropship => DeliveryMode::Dropship,
            Self::Bulk => DeliveryMode::Bulk,
        }
    }
}

#[cfg(test)]
mod tests {

    use super::{FulfillmentResponsibility, PurchaseLineType, PurchaseType, SupplyPriceBasis};

    #[test]
    fn price_basis_codes_are_stable() {
        assert_eq!(SupplyPriceBasis::Bulk.as_str(), "BULK");
        assert_eq!(serde_json::to_string(&SupplyPriceBasis::Dropship).unwrap(), "\"DROPSHIP\"");
        assert_eq!(SupplyPriceBasis::Bulk.label(), "集采");
    }

    #[test]
    fn enums_expose_labels_and_stable_codes() {
        assert_eq!(PurchaseType::Physical.label(), "实物");
        assert_eq!(PurchaseType::Virtual.label(), "虚拟");
        assert_eq!(PurchaseType::Service.label(), "线下服务");
        assert_eq!(FulfillmentResponsibility::SupplierDirect.label(), "供应商直发");
        assert_eq!(FulfillmentResponsibility::Electronic.label(), "电子交付");
        assert_eq!(PurchaseLineType::LogisticsFee.label(), "费用");
        assert_eq!(PurchaseType::Expense.as_str(), "EXPENSE");
        assert_eq!(FulfillmentResponsibility::NotApplicable.as_str(), "NONE");

        assert_eq!(PurchaseType::Physical.as_str(), "PHYSICAL");
        assert_eq!(FulfillmentResponsibility::Warehouse.as_str(), "WAREHOUSE");
        assert_eq!(PurchaseLineType::ItemService.as_str(), "ITEM_SERVICE");
    }

    #[test]
    fn purchase_owner_fulfillment_object_matches_responsibility() {
        assert_eq!(
            FulfillmentResponsibility::SupplierDirect.owner_fulfillment_object_type(),
            Some("delivery")
        );
        assert_eq!(
            FulfillmentResponsibility::Electronic.owner_fulfillment_object_type(),
            Some("electronic_delivery")
        );
        assert_eq!(
            FulfillmentResponsibility::Service.owner_fulfillment_object_type(),
            Some("service_fulfillment")
        );
        assert_eq!(FulfillmentResponsibility::Warehouse.owner_fulfillment_object_type(), None);
    }

    #[test]
    fn enums_serialize_uppercase() {
        assert_eq!(serde_json::to_string(&PurchaseType::Service).unwrap(), "\"SERVICE\"");
        assert_eq!(
            serde_json::to_string(&FulfillmentResponsibility::SupplierDirect).unwrap(),
            "\"SUPPLIER_DIRECT\""
        );
        assert_eq!(serde_json::to_string(&PurchaseLineType::LogisticsFee).unwrap(), "\"LOGISTICS_FEE\"");
    }
}
