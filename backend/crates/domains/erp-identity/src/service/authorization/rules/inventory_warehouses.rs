//! 库存的真实仓库目标；复用原动作资格和人员范围，不冒充库存对象。
use crate::Permission;
use crate::entity::access_control::scope_spec::INVENTORY_WAREHOUSE_ENTRIES;

/// 从仓库入口配对查原库存动作，以复用其功能资格和数据范围，不将仓库视作库存记录。
pub(super) fn source(permission: &Permission) -> Option<&'static Permission> {
    INVENTORY_WAREHOUSE_ENTRIES.iter().find_map(|(entry, source)| (entry == permission).then_some(source))
}
