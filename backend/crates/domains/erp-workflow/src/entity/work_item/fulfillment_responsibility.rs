//! 履约工作项冻结责任键的强类型合同。

use erp_core::{Error, Result};

/// 履约责任键。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FulfillmentResponsibilityKey {
    /// 采购责任人负责的采购单。
    PurchaseOrder(String),
    /// 仓库入库责任。
    WarehouseReceipt(String),
    /// 仓库出库责任。
    WarehouseShip(String),
}

impl FulfillmentResponsibilityKey {
    /// 解析既有持久化责任键。
    ///
    /// # 参数
    /// * `value` - 已持久化的责任键字符串。
    ///
    /// # 返回
    /// 精确命中采购单、仓库入库或仓库出库格式时返回对应键。
    ///
    /// # 错误
    /// 未注册前缀、后缀、空对象 ID 或含额外分隔符时返回错误。
    pub fn parse(value: &str) -> Result<Self> {
        if let Some(id) = exact_segment(value, "purchase_order:", "") {
            return Ok(Self::PurchaseOrder(id.to_string()));
        }
        if let Some(id) = exact_segment(value, "warehouse:", ":receipt") {
            return Ok(Self::WarehouseReceipt(id.to_string()));
        }
        if let Some(id) = exact_segment(value, "warehouse:", ":warehouse_ship") {
            return Ok(Self::WarehouseShip(id.to_string()));
        }
        Err(Error::from("履约责任键无效"))
    }

    /// 构造采购单责任键。
    ///
    /// # 参数
    /// * `id` - 采购单 ID；不得为空或包含 `:`。
    ///
    /// # 返回
    /// 返回 `purchase_order:` 前缀的责任键。
    ///
    /// # 错误
    /// `id` 为空、含空白或含 `:` 时返回错误。
    pub fn purchase_order(id: impl Into<String>) -> Result<Self> {
        Self::parse(&format!("purchase_order:{}", id.into()))
    }

    /// 构造仓库入库责任键。
    ///
    /// # 参数
    /// * `id` - 入库对象 ID；不得为空或包含 `:`。
    ///
    /// # 返回
    /// 返回 `warehouse:{id}:receipt` 形式的责任键。
    ///
    /// # 错误
    /// `id` 为空、含空白或含 `:` 时返回错误。
    pub fn warehouse_receipt(id: impl Into<String>) -> Result<Self> {
        Self::parse(&format!("warehouse:{}:receipt", id.into()))
    }

    /// 构造仓库出库责任键。
    ///
    /// # 参数
    /// * `id` - 出库对象 ID；不得为空或包含 `:`。
    ///
    /// # 返回
    /// 返回 `warehouse:{id}:warehouse_ship` 形式的责任键。
    ///
    /// # 错误
    /// `id` 为空、含空白或含 `:` 时返回错误。
    pub fn warehouse_ship(id: impl Into<String>) -> Result<Self> {
        Self::parse(&format!("warehouse:{}:warehouse_ship", id.into()))
    }

    /// 返回兼容既有持久化格式的字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回可被 [`Self::parse`] 还原的责任键。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_persisted(&self) -> String {
        match self {
            Self::PurchaseOrder(id) => format!("purchase_order:{id}"),
            Self::WarehouseReceipt(id) => format!("warehouse:{id}:receipt"),
            Self::WarehouseShip(id) => format!("warehouse:{id}:warehouse_ship"),
        }
    }

    /// 返回责任对象 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回键中的对象 ID，不含前缀或后缀。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn object_id(&self) -> &str {
        match self {
            Self::PurchaseOrder(id) | Self::WarehouseReceipt(id) | Self::WarehouseShip(id) => id,
        }
    }
}

fn exact_segment<'a>(value: &'a str, prefix: &str, suffix: &str) -> Option<&'a str> {
    value
        .strip_prefix(prefix)
        .and_then(|value| value.strip_suffix(suffix))
        .filter(|id| !id.is_empty() && id.trim() == *id && !id.contains(':'))
}

#[cfg(test)]
mod tests {
    use super::FulfillmentResponsibilityKey;

    #[test]
    fn registered_keys_round_trip_existing_wire_format() {
        for key in [
            FulfillmentResponsibilityKey::purchase_order("po-1").unwrap(),
            FulfillmentResponsibilityKey::warehouse_receipt("wh-1").unwrap(),
            FulfillmentResponsibilityKey::warehouse_ship("wh-1").unwrap(),
        ] {
            assert_eq!(FulfillmentResponsibilityKey::parse(&key.as_persisted()).unwrap(), key);
        }
    }

    #[test]
    fn rejects_unknown_empty_or_ambiguous_keys() {
        for value in [
            "purchase_order:",
            "purchase_order:po:1",
            "warehouse:wh-1:unknown",
            "warehouse: wh-1:receipt",
            "role:warehouse",
        ] {
            assert!(FulfillmentResponsibilityKey::parse(value).is_err(), "{value}");
        }
    }
}
