//! 域内共享固定枚举（数据模型 §6.6：采购类型、履约责任、行类型）。

use serde::{Deserialize, Serialize};

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

impl PurchaseType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Physical => "实物",
            Self::Virtual => "虚拟",
            Self::Service => "线下服务",
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
            Self::Service => "SERVICE",
        }
    }
}

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

impl FulfillmentResponsibility {
    /// 返回责任的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Warehouse => "入仓",
            Self::SupplierDirect => "供应商直发",
            Self::Electronic => "电子交付",
            Self::Service => "线下服务",
        }
    }

    /// 返回责任的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Warehouse => "WAREHOUSE",
            Self::SupplierDirect => "SUPPLIER_DIRECT",
            Self::Electronic => "ELECTRONIC",
            Self::Service => "SERVICE",
        }
    }

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

impl PurchaseLineType {
    /// 返回行类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::ItemService => "商品/服务成本",
            Self::LogisticsFee => "物流费用",
        }
    }

    /// 返回行类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::ItemService => "ITEM_SERVICE",
            Self::LogisticsFee => "LOGISTICS_FEE",
        }
    }
}

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
        assert_eq!(
            FulfillmentResponsibility::Warehouse.owner_fulfillment_object_type(),
            None
        );
    }

    #[test]
    fn enums_serialize_uppercase() {
        assert_eq!(
            serde_json::to_string(&PurchaseType::Service).unwrap(),
            "\"SERVICE\""
        );
        assert_eq!(
            serde_json::to_string(&FulfillmentResponsibility::SupplierDirect).unwrap(),
            "\"SUPPLIER_DIRECT\""
        );
        assert_eq!(
            serde_json::to_string(&PurchaseLineType::LogisticsFee).unwrap(),
            "\"LOGISTICS_FEE\""
        );
    }
}
