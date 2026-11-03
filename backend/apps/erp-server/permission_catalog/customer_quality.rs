//! customer_quality 的接口权限目录。
use super::{PermissionDef, customer, groups, sales_order};

/// 查询当前负责客户经营情况。
pub const QUALITY_CURRENT: PermissionDef =
    customer::LIST.described(groups::CUSTOMER_QUALITY, "查询当前负责客户经营情况");

/// 查询历史负责订单贡献。
pub const QUALITY_HISTORY: PermissionDef =
    sales_order::LIST.described(groups::CUSTOMER_QUALITY, "查询历史负责订单贡献");

/// 导出当前负责客户经营情况。
pub const QUALITY_CURRENT_EXPORT: PermissionDef =
    customer::LIST.described(groups::CUSTOMER_QUALITY, "导出当前负责客户经营情况");

/// 导出历史负责订单贡献。
pub const QUALITY_HISTORY_EXPORT: PermissionDef =
    sales_order::LIST.described(groups::CUSTOMER_QUALITY, "导出历史负责订单贡献");

/// 查询历史归属候选。
pub const QUALITY_HISTORY_DIRECTORY: PermissionDef =
    sales_order::LIST.described(groups::COST_ENTRY, "查询历史归属候选");

/// 本模块可供路由引用的显式权限定义。
pub const ENTRIES: &[(&str, PermissionDef)] = &[
    ("QUALITY_CURRENT", QUALITY_CURRENT),
    ("QUALITY_HISTORY", QUALITY_HISTORY),
    ("QUALITY_CURRENT_EXPORT", QUALITY_CURRENT_EXPORT),
    ("QUALITY_HISTORY_EXPORT", QUALITY_HISTORY_EXPORT),
    ("QUALITY_HISTORY_DIRECTORY", QUALITY_HISTORY_DIRECTORY),
];
