//! 域 D30 `mall_after_sales` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3）：
//! `indexes/` 与 `repository/` 两侧统一取
//! `<mongodb::Database as MallAfterSalesExt>::MALL_REFUNDS` 等值，禁止字面量重复。
//!
//! 退款、余额恢复及其行/分配是正式事实（§4.5 不设业务软删除），只暴露只读追加
//! 仓储，不暴露带软删除方法的通用 `Repository`；`mall_after_sales_request`(+`_line`)
//! 是售后申请稳定头，走通用 `Repository`。
//!
//! 已知冻结实体缺陷（P1，待地基修订）：`MallAfterSalesRequest` 同时声明扁平化的
//! `BaseModel.created_at` 与同名域字段 `created_at: Instant`（商城申请时间），
//! serde 无法往返（BSON 序列化后反序列化报「missing field created_at」），
//! `mall_after_sales_request` 头表暂不可持久化；实体修订（域字段改名）前
//! 头表只能走投影查询，`mall_after_sales_requests()` 访问器保持完整签名，
//! 待修订后即可使用。

use entities::mall_after_sales::{MallAfterSalesRequest, MallAfterSalesRequestLine};
use mongodb::Database;

use super::super::mall_after_sales::{
    MallAfterSalesRepository, MallAfterSalesRequestFilter, MallBalanceRestorationAllocationRepository,
    MallBalanceRestorationRepository, MallRefundAllocationRepository, MallRefundLineRepository,
    MallRefundRepository,
};
use crate::Repository;

/// 域 D30 仓储访问器。
pub trait MallAfterSalesExt {
    /// `mall_after_sales_request` 集合名。
    const MALL_AFTER_SALES_REQUESTS: &'static str = "mall_after_sales_requests";
    /// `mall_after_sales_request_line` 集合名。
    const MALL_AFTER_SALES_REQUEST_LINES: &'static str = "mall_after_sales_request_lines";
    /// `mall_refund` 集合名。
    const MALL_REFUNDS: &'static str = "mall_refunds";
    /// `mall_refund_line` 集合名。
    const MALL_REFUND_LINES: &'static str = "mall_refund_lines";
    /// `mall_refund_allocation` 集合名。
    const MALL_REFUND_ALLOCATIONS: &'static str = "mall_refund_allocations";
    /// `mall_balance_restoration` 集合名。
    const MALL_BALANCE_RESTORATIONS: &'static str = "mall_balance_restorations";
    /// `mall_balance_restoration_allocation` 集合名。
    const MALL_BALANCE_RESTORATION_ALLOCATIONS: &'static str = "mall_balance_restoration_allocations";

    /// 售后请求列表筛选条件类型（定义见 `repository::mall_after_sales`）。
    type MallAfterSalesRequestFilter;

    /// 获取 `mall_after_sales_request` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_after_sales::MallAfterSalesRequest>`。
    fn mall_after_sales_requests(&self) -> Repository<'_, MallAfterSalesRequest>;

    /// 获取 `mall_after_sales_request_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::mall_after_sales::MallAfterSalesRequestLine>`。
    fn mall_after_sales_request_lines(&self) -> Repository<'_, MallAfterSalesRequestLine>;

    /// 获取 `mall_refund` 集合的只读追加仓储。
    ///
    /// 退款头是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallRefundRepository` 实例。
    fn mall_refunds(&self) -> MallRefundRepository<'_>;

    /// 获取 `mall_refund_line` 集合的只读追加仓储。
    ///
    /// 退款行是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallRefundLineRepository` 实例。
    fn mall_refund_lines(&self) -> MallRefundLineRepository<'_>;

    /// 获取 `mall_refund_allocation` 集合的只读追加仓储。
    ///
    /// 退款分配是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallRefundAllocationRepository` 实例。
    fn mall_refund_allocations(&self) -> MallRefundAllocationRepository<'_>;

    /// 获取 `mall_balance_restoration` 集合的只读追加仓储。
    ///
    /// 余额恢复头是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallBalanceRestorationRepository` 实例。
    fn mall_balance_restorations(&self) -> MallBalanceRestorationRepository<'_>;

    /// 获取 `mall_balance_restoration_allocation` 集合的只读追加仓储。
    ///
    /// 余额恢复分配是不可变正式事实（§4.5），不提供更新、软删除与恢复。
    ///
    /// # 返回
    /// 返回 `MallBalanceRestorationAllocationRepository` 实例。
    fn mall_balance_restoration_allocations(&self) -> MallBalanceRestorationAllocationRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `MallAfterSalesRepository` 实例。
    fn mall_after_sales(&self) -> MallAfterSalesRepository<'_>;
}

impl MallAfterSalesExt for Database {
    type MallAfterSalesRequestFilter = MallAfterSalesRequestFilter;

    fn mall_after_sales_requests(&self) -> Repository<'_, MallAfterSalesRequest> {
        Repository::new(self, Self::MALL_AFTER_SALES_REQUESTS)
    }

    fn mall_after_sales_request_lines(&self) -> Repository<'_, MallAfterSalesRequestLine> {
        Repository::new(self, Self::MALL_AFTER_SALES_REQUEST_LINES)
    }

    fn mall_refunds(&self) -> MallRefundRepository<'_> {
        MallRefundRepository::new(self)
    }

    fn mall_refund_lines(&self) -> MallRefundLineRepository<'_> {
        MallRefundLineRepository::new(self)
    }

    fn mall_refund_allocations(&self) -> MallRefundAllocationRepository<'_> {
        MallRefundAllocationRepository::new(self)
    }

    fn mall_balance_restorations(&self) -> MallBalanceRestorationRepository<'_> {
        MallBalanceRestorationRepository::new(self)
    }

    fn mall_balance_restoration_allocations(&self) -> MallBalanceRestorationAllocationRepository<'_> {
        MallBalanceRestorationAllocationRepository::new(self)
    }

    fn mall_after_sales(&self) -> MallAfterSalesRepository<'_> {
        MallAfterSalesRepository::new(self)
    }
}
