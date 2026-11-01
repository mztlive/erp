//! 不对应独立 HTTP 路由、由领域读取政策消费的操作资格。

/// 财务整账读取资格；不代替资源读取动作、来源边界或资金执行资格。
pub const FINANCE_LEDGER_READ: &str = "finance_ledger:read";

/// 供权限目录生成器展示的领域政策资格。
pub const POLICY_PERMISSIONS: &[(&str, &str, &str, &str)] = &[(
    "财务整账",
    "完整财务台账及无来源记录的读取资格；仍检查业务读取动作和来源边界",
    FINANCE_LEDGER_READ,
    "读取完整及未分配财务台账",
)];
