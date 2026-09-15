//! 域 D08 `customer` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as CustomerExt>::CUSTOMER_ACCOUNTS` 等值。

use mongodb::Database;

use super::super::customer::{CustomerAccountFilter, CustomerAssignmentFilter};
use crate::repository::owned::{
    CustomerAccountRepository, CustomerAssignmentRepository, CustomerProfileCommandRepository,
};

/// 域 D08 仓储访问器。
pub trait CustomerExt {
    /// `customer_account` 集合名。
    const CUSTOMER_ACCOUNTS: &'static str = "customer_accounts";
    /// `customer_assignment` 集合名。
    const CUSTOMER_ASSIGNMENTS: &'static str = "customer_assignments";
    /// `customer_profile_command` 根级命令去重结果集合名。
    const CUSTOMER_PROFILE_COMMANDS: &'static str = "customer_profile_commands";

    /// 客户角色列表筛选条件类型（定义见 `repository::customer`）。
    type CustomerAccountFilter;
    /// 客户归属列表筛选条件类型（定义见 `repository::customer`）。
    type CustomerAssignmentFilter;

    /// 获取 `customer_account` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `CustomerAccountRepository<'_>`。
    fn customer_accounts(&self) -> CustomerAccountRepository<'_>;

    /// 获取 `customer_assignment` 集合的 Repository（按有效期的归属事实行）。
    ///
    /// # 返回
    /// 返回 `CustomerAssignmentRepository<'_>`。
    fn customer_assignments(&self) -> CustomerAssignmentRepository<'_>;

    /// 获取客户资料根级命令去重仓储。
    ///
    /// # 返回
    /// 返回 `CustomerProfileCommandRepository<'_>`。
    fn customer_profile_commands(&self) -> CustomerProfileCommandRepository<'_>;
}

impl CustomerExt for Database {
    type CustomerAccountFilter = CustomerAccountFilter;
    type CustomerAssignmentFilter = CustomerAssignmentFilter;

    fn customer_accounts(&self) -> CustomerAccountRepository<'_> {
        CustomerAccountRepository::new(self, Self::CUSTOMER_ACCOUNTS)
    }

    fn customer_assignments(&self) -> CustomerAssignmentRepository<'_> {
        CustomerAssignmentRepository::new(self, Self::CUSTOMER_ASSIGNMENTS)
    }

    fn customer_profile_commands(&self) -> CustomerProfileCommandRepository<'_> {
        CustomerProfileCommandRepository::new(self, Self::CUSTOMER_PROFILE_COMMANDS)
    }
}
