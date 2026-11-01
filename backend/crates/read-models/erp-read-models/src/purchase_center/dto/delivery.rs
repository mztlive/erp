//! 采购调整交付安排时可选的销售履约期限与收件地址。

use erp_core::money::Quantity;
use serde::Serialize;

/// 采购各行可调整的交付安排范围。
#[derive(Debug, Clone, Default, Serialize, PartialEq, Eq)]
pub struct PurchaseDeliveryOptionsView {
    /// 采购当前内容引用、且仍在销售当前版本中的商品行。
    pub lines: Vec<PurchaseDeliveryLineOptionView>,
}

/// 一个销售稳定行的交付安排范围。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseDeliveryLineOptionView {
    /// 销售稳定行。
    pub sales_order_line_id: String,
    /// 销售当前版本行号。
    pub line_no: u32,
    /// 销售当前版本品名。
    pub item_name: String,
    /// 销售当前版本履约期限（上海业务日），预计交付日不得晚于此日。
    pub sales_due_date: String,
    /// 本采购须按收件地址分配数量：供应商直发或服务履约且销售指定了收件资料。
    pub requires_addresses: bool,
    /// 扣除其他采购后的收件地址剩余量；仓库履约时为空，不暴露客户地址。
    pub addresses: Vec<PurchaseDeliveryAddressOptionView>,
    /// 采购改单可填数量范围；采购尚未生效时为空。
    pub quantity_bound: Option<PurchaseQuantityBoundView>,
}

/// 采购改单一行的可填数量范围（第一期 §6.3 采购改单执行规则）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseQuantityBoundView {
    /// 采购当前生效数量。
    pub base_quantity: Quantity,
    /// 销售当前版本目标数量。
    pub sales_quantity: Quantity,
    /// 本采购之外其他采购与库存的覆盖数量。
    pub other_covered_quantity: Quantity,
    /// 本采购已入库、发货或交付的数量。
    pub fulfilled_quantity: Quantity,
    /// 可填最小数量。
    pub min_quantity: Quantity,
    /// 可填最大数量。
    pub max_quantity: Quantity,
    /// 是否因销售归零或减量必须调减。
    pub quantity_affected: bool,
}

/// 一个可分配的收件地址。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct PurchaseDeliveryAddressOptionView {
    /// 收货人。
    pub recipient: String,
    /// 联系电话。
    pub phone: String,
    /// 收货地区代码。
    pub region_code: String,
    /// 收货地区完整名称。
    pub region_name: String,
    /// 详细地址。
    pub address: String,
    /// 本采购最多可分配数量。
    pub available_quantity: Quantity,
}
