//! customer_quality 的内置权限。
use crate::{Permission, customer, sales_order};

/// 查询当前负责客户经营情况。
pub const QUALITY_CURRENT: Permission = customer::LIST;

/// 查询历史负责订单贡献。
pub const QUALITY_HISTORY: Permission = sales_order::LIST;

/// 导出当前负责客户经营情况。
pub const QUALITY_CURRENT_EXPORT: Permission = customer::LIST;

/// 导出历史负责订单贡献。
pub const QUALITY_HISTORY_EXPORT: Permission = sales_order::LIST;

/// 查询历史归属候选。
pub const QUALITY_HISTORY_DIRECTORY: Permission = sales_order::LIST;

/// 本模块全部复用其他模块的权限定义。
pub const ENTRIES: &[(&str, crate::PermissionDef)] = &[];
