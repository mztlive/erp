//! 域 D15 `purchase_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS` 等值。

use mongodb::Database;

use super::super::purchase_order::{
    PurchaseOrderDomainRepository, PurchaseOrderFilter, PurchaseOrderSubmissionFilter,
};
use crate::repository::owned::{
    PurchaseChangeOrderRepository, PurchaseChangeSubmissionLineRepository,
    PurchaseChangeSubmissionRepository, PurchaseLineSalesAllocationRepository, PurchaseOrderRepository,
    PurchaseOrderRevisionLineRepository, PurchaseOrderRevisionRepository,
    PurchaseOrderSubmissionLineRepository, PurchaseOrderSubmissionRepository,
};

/// 域 D15 仓储访问器。
pub trait PurchaseOrderExt: Sized {
    /// `purchase_order` 集合名。
    const PURCHASE_ORDERS: &'static str = "purchase_orders";
    /// `purchase_order_submission` 集合名。
    const PURCHASE_ORDER_SUBMISSIONS: &'static str = "purchase_order_submissions";
    /// `purchase_order_submission_line` 集合名。
    const PURCHASE_ORDER_SUBMISSION_LINES: &'static str = "purchase_order_submission_lines";
    /// `purchase_order_revision` 集合名。
    const PURCHASE_ORDER_REVISIONS: &'static str = "purchase_order_revisions";
    /// `purchase_order_revision_line` 集合名。
    const PURCHASE_ORDER_REVISION_LINES: &'static str = "purchase_order_revision_lines";
    /// `purchase_line_sales_allocation` 集合名。
    const PURCHASE_LINE_SALES_ALLOCATIONS: &'static str = "purchase_line_sales_allocations";
    /// `purchase_change_order` 集合名。
    const PURCHASE_CHANGE_ORDERS: &'static str = "purchase_change_orders";
    /// `purchase_change_submission` 集合名。
    const PURCHASE_CHANGE_SUBMISSIONS: &'static str = "purchase_change_submissions";
    /// `purchase_change_submission_line` 集合名。
    const PURCHASE_CHANGE_SUBMISSION_LINES: &'static str = "purchase_change_submission_lines";

    /// 采购单列表筛选条件类型（定义见 `repository::purchase_order`）。
    type PurchaseOrderFilter;

    /// 采购提交列表筛选条件类型（定义见 `repository::purchase_order`）。
    type PurchaseOrderSubmissionFilter;

    /// 获取 `purchase_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderRepository<'_>`。
    fn purchase_orders(&self) -> PurchaseOrderRepository<'_>;

    /// 获取 `purchase_order_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderSubmissionRepository<'_>`。
    fn purchase_order_submissions(&self) -> PurchaseOrderSubmissionRepository<'_>;

    /// 获取 `purchase_order_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderSubmissionLineRepository<'_>`。
    fn purchase_order_submission_lines(&self) -> PurchaseOrderSubmissionLineRepository<'_>;

    /// 获取 `purchase_order_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderRevisionRepository<'_>`。
    fn purchase_order_revisions(&self) -> PurchaseOrderRevisionRepository<'_>;

    /// 获取 `purchase_order_revision_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderRevisionLineRepository<'_>`。
    fn purchase_order_revision_lines(&self) -> PurchaseOrderRevisionLineRepository<'_>;

    /// 获取 `purchase_line_sales_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseLineSalesAllocationRepository<'_>`。
    fn purchase_line_sales_allocations(&self) -> PurchaseLineSalesAllocationRepository<'_>;

    /// 获取 `purchase_change_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseChangeOrderRepository<'_>`。
    fn purchase_change_orders(&self) -> PurchaseChangeOrderRepository<'_>;

    /// 获取 `purchase_change_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseChangeSubmissionRepository<'_>`。
    fn purchase_change_submissions(&self) -> PurchaseChangeSubmissionRepository<'_>;

    /// 获取 `purchase_change_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `PurchaseChangeSubmissionLineRepository<'_>`。
    fn purchase_change_submission_lines(&self) -> PurchaseChangeSubmissionLineRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderDomainRepository` 实例。
    fn purchase_order(&self) -> PurchaseOrderDomainRepository<'_>;
}

impl PurchaseOrderExt for Database {
    type PurchaseOrderFilter = PurchaseOrderFilter;
    type PurchaseOrderSubmissionFilter = PurchaseOrderSubmissionFilter;

    fn purchase_orders(&self) -> PurchaseOrderRepository<'_> {
        PurchaseOrderRepository::new(self, Self::PURCHASE_ORDERS)
    }

    fn purchase_order_submissions(&self) -> PurchaseOrderSubmissionRepository<'_> {
        PurchaseOrderSubmissionRepository::new(self, Self::PURCHASE_ORDER_SUBMISSIONS)
    }

    fn purchase_order_submission_lines(&self) -> PurchaseOrderSubmissionLineRepository<'_> {
        PurchaseOrderSubmissionLineRepository::new(self, Self::PURCHASE_ORDER_SUBMISSION_LINES)
    }

    fn purchase_order_revisions(&self) -> PurchaseOrderRevisionRepository<'_> {
        PurchaseOrderRevisionRepository::new(self, Self::PURCHASE_ORDER_REVISIONS)
    }

    fn purchase_order_revision_lines(&self) -> PurchaseOrderRevisionLineRepository<'_> {
        PurchaseOrderRevisionLineRepository::new(self, Self::PURCHASE_ORDER_REVISION_LINES)
    }

    fn purchase_line_sales_allocations(&self) -> PurchaseLineSalesAllocationRepository<'_> {
        PurchaseLineSalesAllocationRepository::new(self, Self::PURCHASE_LINE_SALES_ALLOCATIONS)
    }

    fn purchase_change_orders(&self) -> PurchaseChangeOrderRepository<'_> {
        PurchaseChangeOrderRepository::new(self, Self::PURCHASE_CHANGE_ORDERS)
    }

    fn purchase_change_submissions(&self) -> PurchaseChangeSubmissionRepository<'_> {
        PurchaseChangeSubmissionRepository::new(self, Self::PURCHASE_CHANGE_SUBMISSIONS)
    }

    fn purchase_change_submission_lines(&self) -> PurchaseChangeSubmissionLineRepository<'_> {
        PurchaseChangeSubmissionLineRepository::new(self, Self::PURCHASE_CHANGE_SUBMISSION_LINES)
    }

    fn purchase_order(&self) -> PurchaseOrderDomainRepository<'_> {
        PurchaseOrderDomainRepository::new(self)
    }
}
