//! 域内共享固定枚举（数据模型 §6.6：采购类型、履约责任、行类型）。

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
            /// # 返回
            /// 返回面向用户的中文标签。
            pub fn label(&self) -> &'static str {
                match self {
                    $(Self::$variant => $label,)*
                }
            }

            /// 返回稳定代码。
            ///
            /// # 返回
            /// 返回用于持久化与查询的稳定字符串。
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
}

status_display!(PurchaseType, {
    Physical => ("实物", "PHYSICAL"),
    Virtual => ("虚拟", "VIRTUAL"),
    Service => ("线下服务", "SERVICE"),
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
}

status_display!(FulfillmentResponsibility, {
    Warehouse => ("入仓", "WAREHOUSE"),
    SupplierDirect => ("供应商直发", "SUPPLIER_DIRECT"),
    Electronic => ("电子交付", "ELECTRONIC"),
    Service => ("线下服务", "SERVICE"),
});

impl FulfillmentResponsibility {
    /// 返回采购单责任人后续直接执行的履约对象类型。
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
            Self::Warehouse => None,
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
    /// 物流费用行（与商品成本分开计税，数量为空）。
    #[serde(rename = "LOGISTICS_FEE")]
    LogisticsFee,
}

status_display!(PurchaseLineType, {
    ItemService => ("商品/服务成本", "ITEM_SERVICE"),
    LogisticsFee => ("物流费用", "LOGISTICS_FEE"),
});

#[cfg(test)]
mod tests {

    use super::{FulfillmentResponsibility, PurchaseLineType, PurchaseType};

    #[test]
    fn enums_expose_labels_and_stable_codes() {
        assert_eq!(PurchaseType::Physical.label(), "实物");
        assert_eq!(PurchaseType::Virtual.label(), "虚拟");
        assert_eq!(PurchaseType::Service.label(), "线下服务");
        assert_eq!(FulfillmentResponsibility::SupplierDirect.label(), "供应商直发");
        assert_eq!(FulfillmentResponsibility::Electronic.label(), "电子交付");
        assert_eq!(PurchaseLineType::LogisticsFee.label(), "物流费用");

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
