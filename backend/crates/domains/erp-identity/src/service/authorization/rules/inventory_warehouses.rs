//! 库存的真实仓库目标；复用原动作资格和人员范围，不冒充库存对象。
use crate::Permission;
use crate::entity::access_control::scope_spec::INVENTORY_WAREHOUSE_ENTRIES;

pub(super) fn source(permission: &Permission) -> Option<&'static Permission> {
    INVENTORY_WAREHOUSE_ENTRIES.iter().find_map(|(entry, source)| (entry == permission).then_some(source))
}
