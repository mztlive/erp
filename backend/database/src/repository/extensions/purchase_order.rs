//! 域 D15 `purchase_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS` 等值。

use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseLineSalesAllocation,
    PurchaseOrder, PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderSubmission,
    PurchaseOrderSubmissionLine,
};
use mongodb::Database;

use super::super::purchase_order::{
    PurchaseOrderFilter, PurchaseOrderRepository, PurchaseOrderSubmissionFilter,
};
use crate::Repository;

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
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrder>`。
    fn purchase_orders(&self) -> Repository<'_, PurchaseOrder>;

    /// 获取 `purchase_order_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderSubmission>`。
    fn purchase_order_submissions(&self) -> Repository<'_, PurchaseOrderSubmission>;

    /// 获取 `purchase_order_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderSubmissionLine>`。
    fn purchase_order_submission_lines(&self) -> Repository<'_, PurchaseOrderSubmissionLine>;

    /// 获取 `purchase_order_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderRevision>`。
    fn purchase_order_revisions(&self) -> Repository<'_, PurchaseOrderRevision>;

    /// 获取 `purchase_order_revision_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseOrderRevisionLine>`。
    fn purchase_order_revision_lines(&self) -> Repository<'_, PurchaseOrderRevisionLine>;

    /// 获取 `purchase_line_sales_allocation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseLineSalesAllocation>`。
    fn purchase_line_sales_allocations(&self) -> Repository<'_, PurchaseLineSalesAllocation>;

    /// 获取 `purchase_change_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeOrder>`。
    fn purchase_change_orders(&self) -> Repository<'_, PurchaseChangeOrder>;

    /// 获取 `purchase_change_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeSubmission>`。
    fn purchase_change_submissions(&self) -> Repository<'_, PurchaseChangeSubmission>;

    /// 获取 `purchase_change_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::purchase_order::PurchaseChangeSubmissionLine>`。
    fn purchase_change_submission_lines(&self) -> Repository<'_, PurchaseChangeSubmissionLine>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `PurchaseOrderRepository` 实例。
    fn purchase_order(&self) -> PurchaseOrderRepository<'_>;
}

impl PurchaseOrderExt for Database {
    type PurchaseOrderFilter = PurchaseOrderFilter;
    type PurchaseOrderSubmissionFilter = PurchaseOrderSubmissionFilter;

    fn purchase_orders(&self) -> Repository<'_, PurchaseOrder> {
        Repository::new(self, Self::PURCHASE_ORDERS)
    }

    fn purchase_order_submissions(&self) -> Repository<'_, PurchaseOrderSubmission> {
        Repository::new(self, Self::PURCHASE_ORDER_SUBMISSIONS)
    }

    fn purchase_order_submission_lines(&self) -> Repository<'_, PurchaseOrderSubmissionLine> {
        Repository::new(self, Self::PURCHASE_ORDER_SUBMISSION_LINES)
    }

    fn purchase_order_revisions(&self) -> Repository<'_, PurchaseOrderRevision> {
        Repository::new(self, Self::PURCHASE_ORDER_REVISIONS)
    }

    fn purchase_order_revision_lines(&self) -> Repository<'_, PurchaseOrderRevisionLine> {
        Repository::new(self, Self::PURCHASE_ORDER_REVISION_LINES)
    }

    fn purchase_line_sales_allocations(&self) -> Repository<'_, PurchaseLineSalesAllocation> {
        Repository::new(self, Self::PURCHASE_LINE_SALES_ALLOCATIONS)
    }

    fn purchase_change_orders(&self) -> Repository<'_, PurchaseChangeOrder> {
        Repository::new(self, Self::PURCHASE_CHANGE_ORDERS)
    }

    fn purchase_change_submissions(&self) -> Repository<'_, PurchaseChangeSubmission> {
        Repository::new(self, Self::PURCHASE_CHANGE_SUBMISSIONS)
    }

    fn purchase_change_submission_lines(&self) -> Repository<'_, PurchaseChangeSubmissionLine> {
        Repository::new(self, Self::PURCHASE_CHANGE_SUBMISSION_LINES)
    }

    fn purchase_order(&self) -> PurchaseOrderRepository<'_> {
        PurchaseOrderRepository::new(self)
    }
}
