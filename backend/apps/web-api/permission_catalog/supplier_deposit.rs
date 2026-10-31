//! supplier_deposit 的接口权限目录。
use super::{PermissionDef, groups};

/// 查询预存款账户。
pub const LIST: PermissionDef = PermissionDef {
    code: "supplier_deposit:list",
    group: groups::SUPPLIER_DEPOSIT,
    description: "查询预存款账户",
};

/// 读取预存款账户明细。
pub const DETAIL: PermissionDef = PermissionDef {
    code: "supplier_deposit:detail",
    group: groups::SUPPLIER_DEPOSIT,
    description: "读取预存款账户明细",
};

/// 开立供应商预存款账户。
pub const OPEN: PermissionDef = PermissionDef {
    code: "supplier_deposit:create",
    group: groups::SUPPLIER_DEPOSIT,
    description: "开立供应商预存款账户",
};

/// 申请供应商预存款充值。
pub const REQUEST_RECHARGE: PermissionDef = PermissionDef {
    code: "supplier_deposit:recharge_request",
    group: groups::SUPPLIER_DEPOSIT,
    description: "申请供应商预存款充值",
};

/// 批准供应商预存款充值。
pub const APPROVE_RECHARGE: PermissionDef = PermissionDef {
    code: "supplier_deposit:approve_recharge",
    group: groups::SUPPLIER_DEPOSIT,
    description: "批准供应商预存款充值",
};

/// 登记充值银行付款。
pub const PAY_RECHARGE: PermissionDef = PermissionDef {
    code: "supplier_deposit:pay_recharge",
    group: groups::SUPPLIER_DEPOSIT,
    description: "登记充值银行付款",
};

/// 核对供应商充值到账。
pub const RECEIVE_RECHARGE: PermissionDef = PermissionDef {
    code: "supplier_deposit:receive_recharge",
    group: groups::SUPPLIER_DEPOSIT,
    description: "核对供应商充值到账",
};

/// 授权有限额自动余额执行。
pub const AUTHORIZE: PermissionDef = PermissionDef {
    code: "supplier_deposit:authorize",
    group: groups::SUPPLIER_DEPOSIT,
    description: "授权有限额自动余额执行",
};

/// 撤销自动余额执行授权。
pub const REVOKE: PermissionDef = PermissionDef {
    code: "supplier_deposit:revoke",
    group: groups::SUPPLIER_DEPOSIT,
    description: "撤销自动余额执行授权",
};

/// 登记预存款资金对账。
pub const RECONCILE: PermissionDef = PermissionDef {
    code: "supplier_deposit:reconcile",
    group: groups::SUPPLIER_DEPOSIT,
    description: "登记预存款资金对账",
};

/// 占用执行组预存款。
pub const RESERVE: PermissionDef = PermissionDef {
    code: "supplier_deposit:reserve",
    group: groups::SUPPLIER_DEPOSIT,
    description: "占用执行组预存款",
};

/// 核实预存款实际扣款。
pub const RECORD_DEBIT: PermissionDef = SETTLE.described(groups::SUPPLIER_DEPOSIT, "核实预存款实际扣款");

/// 核销已核实预存款使用。
pub const SETTLE: PermissionDef = PermissionDef {
    code: "supplier_deposit:use",
    group: groups::SUPPLIER_DEPOSIT,
    description: "核销已核实预存款使用",
};

/// 核实供应商预存款返款。
pub const RETURN_USAGE: PermissionDef = PermissionDef {
    code: "supplier_deposit:refund",
    group: groups::SUPPLIER_DEPOSIT,
    description: "核实供应商预存款返款",
};

/// 绑定连接资金账号。
pub const BIND: PermissionDef = PermissionDef {
    code: "supplier_deposit:bind",
    group: groups::SUPPLIER_DEPOSIT,
    description: "绑定连接资金账号",
};

/// 结案预存款资金差异。
pub const RESOLUTION_RESOLVE: PermissionDef = PermissionDef {
    code: "supplier_deposit:resolve",
    group: groups::SUPPLIER_DEPOSIT,
    description: "结案预存款资金差异",
};

/// 核实并释放预存款剩余占用。
pub const RESOLUTION_RELEASE: PermissionDef = PermissionDef {
    code: "supplier_deposit:release",
    group: groups::SUPPLIER_DEPOSIT,
    description: "核实并释放预存款剩余占用",
};

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("LIST", LIST),
    ("DETAIL", DETAIL),
    ("OPEN", OPEN),
    ("REQUEST_RECHARGE", REQUEST_RECHARGE),
    ("APPROVE_RECHARGE", APPROVE_RECHARGE),
    ("PAY_RECHARGE", PAY_RECHARGE),
    ("RECEIVE_RECHARGE", RECEIVE_RECHARGE),
    ("AUTHORIZE", AUTHORIZE),
    ("REVOKE", REVOKE),
    ("RECONCILE", RECONCILE),
    ("RESERVE", RESERVE),
    ("RECORD_DEBIT", RECORD_DEBIT),
    ("SETTLE", SETTLE),
    ("RETURN_USAGE", RETURN_USAGE),
    ("BIND", BIND),
    ("RESOLUTION_RESOLVE", RESOLUTION_RESOLVE),
    ("RESOLUTION_RELEASE", RESOLUTION_RELEASE),
];
