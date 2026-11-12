//! 不对应独立路由的领域政策资格。
use crate::{Permission, PermissionDef, groups, resources};

/// 财务整账读取资格；仍需检查业务读取动作及来源边界。
pub const FINANCE_LEDGER_READ: Permission = Permission::new(resources::FINANCE_LEDGER, "read");

/// 非路由权限的公共展示信息。
pub const ENTRIES: &[(&str, PermissionDef)] = &[(
    "FINANCE_LEDGER_READ",
    PermissionDef {
        permission: &FINANCE_LEDGER_READ,
        group: groups::FINANCE_LEDGER,
        description: "读取完整及未分配财务台账",
    },
)];
