//! supplier_deposit 的内置权限。
use crate::{Permission, PermissionDef, groups, resources};

/// 查询预存款账户。
pub const LIST: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "list");

/// 读取预存款账户明细。
pub const DETAIL: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "detail");

/// 开立供应商预存款账户。
pub const OPEN: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "create");

/// 申请供应商预存款充值。
pub const REQUEST_RECHARGE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "recharge_request");

/// 批准供应商预存款充值。
pub const APPROVE_RECHARGE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "approve_recharge");

/// 登记充值银行付款。
pub const PAY_RECHARGE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "pay_recharge");

/// 核对供应商充值到账。
pub const RECEIVE_RECHARGE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "receive_recharge");

/// 授权有限额自动余额执行。
pub const AUTHORIZE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "authorize");

/// 撤销自动余额执行授权。
pub const REVOKE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "revoke");

/// 登记预存款资金对账。
pub const RECONCILE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "reconcile");

/// 占用执行组预存款。
pub const RESERVE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "reserve");

/// 核实预存款实际扣款。
pub const RECORD_DEBIT: Permission = SETTLE;

/// 核销已核实预存款使用。
pub const SETTLE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "use");

/// 核实供应商预存款返款。
pub const RETURN_USAGE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "refund");

/// 绑定连接资金账号。
pub const BIND: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "bind");

/// 结案预存款资金差异。
pub const RESOLUTION_RESOLVE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "resolve");

/// 核实并释放预存款剩余占用。
pub const RESOLUTION_RELEASE: Permission = Permission::new(resources::SUPPLIER_DEPOSIT, "release");

/// 本模块独立权限及可复用展示说明；接口专属说明由应用提供。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    (
        "LIST",
        PermissionDef {
            permission: &LIST, group: groups::SUPPLIER_DEPOSIT, description: "查询预存款账户"
        },
    ),
    (
        "DETAIL",
        PermissionDef {
            permission: &DETAIL,
            group: groups::SUPPLIER_DEPOSIT,
            description: "读取预存款账户明细",
        },
    ),
    (
        "OPEN",
        PermissionDef {
            permission: &OPEN,
            group: groups::SUPPLIER_DEPOSIT,
            description: "开立供应商预存款账户",
        },
    ),
    (
        "REQUEST_RECHARGE",
        PermissionDef {
            permission: &REQUEST_RECHARGE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "申请供应商预存款充值",
        },
    ),
    (
        "APPROVE_RECHARGE",
        PermissionDef {
            permission: &APPROVE_RECHARGE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "批准供应商预存款充值",
        },
    ),
    (
        "PAY_RECHARGE",
        PermissionDef {
            permission: &PAY_RECHARGE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "登记充值银行付款",
        },
    ),
    (
        "RECEIVE_RECHARGE",
        PermissionDef {
            permission: &RECEIVE_RECHARGE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "核对供应商充值到账",
        },
    ),
    (
        "AUTHORIZE",
        PermissionDef {
            permission: &AUTHORIZE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "授权有限额自动余额执行",
        },
    ),
    (
        "REVOKE",
        PermissionDef {
            permission: &REVOKE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "撤销自动余额执行授权",
        },
    ),
    (
        "RECONCILE",
        PermissionDef {
            permission: &RECONCILE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "登记预存款资金对账",
        },
    ),
    (
        "RESERVE",
        PermissionDef {
            permission: &RESERVE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "占用执行组预存款",
        },
    ),
    (
        "SETTLE",
        PermissionDef {
            permission: &SETTLE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "核销已核实预存款使用",
        },
    ),
    (
        "RETURN_USAGE",
        PermissionDef {
            permission: &RETURN_USAGE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "核实供应商预存款返款",
        },
    ),
    (
        "BIND",
        PermissionDef {
            permission: &BIND, group: groups::SUPPLIER_DEPOSIT, description: "绑定连接资金账号"
        },
    ),
    (
        "RESOLUTION_RESOLVE",
        PermissionDef {
            permission: &RESOLUTION_RESOLVE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "结案预存款资金差异",
        },
    ),
    (
        "RESOLUTION_RELEASE",
        PermissionDef {
            permission: &RESOLUTION_RELEASE,
            group: groups::SUPPLIER_DEPOSIT,
            description: "核实并释放预存款剩余占用",
        },
    ),
];
