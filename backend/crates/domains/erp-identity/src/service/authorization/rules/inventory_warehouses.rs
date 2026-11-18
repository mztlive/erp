//! 库存的真实仓库目标；复用原动作资格和人员范围，不冒充库存对象。
use erp_permissions::{internal, inventory};

use crate::Permission;

// 给仓库专用内部入口选用登记的原库存动作，沿用已有功能资格与范围口径。
pub(super) fn source(permission: &Permission) -> Option<Permission> {
    [
        (internal::INVENTORY_BALANCE_WAREHOUSES, inventory::STOCK_BALANCE_LIST),
        (internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES, inventory::STOCK_BALANCE_DETAIL),
        (internal::INVENTORY_MOVEMENT_WAREHOUSES, inventory::STOCK_MOVEMENT_LIST),
        (internal::INVENTORY_RESERVATION_WAREHOUSES, inventory::STOCK_RESERVATION_LIST),
        (internal::INVENTORY_ADJUSTMENT_WAREHOUSES, inventory::STOCK_ADJUSTMENT_LIST),
        (internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES, inventory::STOCK_ADJUSTMENT_DETAIL),
        (internal::INVENTORY_CREATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_CREATE),
        (internal::INVENTORY_UPDATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_UPDATE),
    ]
    .into_iter()
    .find_map(|(entry, source)| (entry == *permission).then_some(source))
}
