//! 业务预定义角色推荐权限表（按角色拆分，权限集合与原单文件一致）。

mod finance;
mod management;
mod operations;
mod procurement;
mod sales;
mod sales_leader;
mod sysadmin;
mod warehouse;

pub(crate) use finance::FINANCE_PERMISSIONS;
pub(crate) use management::MANAGEMENT_PERMISSIONS;
pub(crate) use operations::OPERATIONS_PERMISSIONS;
pub(crate) use procurement::PROCUREMENT_PERMISSIONS;
pub(crate) use sales::SALES_PERMISSIONS;
pub(crate) use sales_leader::SALES_LEADER_PERMISSIONS;
pub(crate) use sysadmin::SYSADMIN_PERMISSIONS;
pub(crate) use warehouse::WAREHOUSE_PERMISSIONS;
