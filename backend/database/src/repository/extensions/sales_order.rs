//! 域 D13 `sales_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SalesOrderExt>::SALES_ORDERS` 等值。

use entities::sales_order::{
    SalesOrder, SalesOrderGoodsServiceLineRevision, SalesOrderLine, SalesOrderRevision,
    SalesOrderRevisionLine, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderVoucherLineRevision,
    SalesOrderWorkingCopy, SalesOrderWorkingCopyLine,
};
use mongodb::Database;

use super::super::sales_order::{
    SalesOrderFilter, SalesOrderRepository, SubmissionFilter, WorkingCopyFilter,
};
use crate::Repository;

/// 域 D13 仓储访问器。
pub trait SalesOrderExt {
    /// `sales_order` 集合名。
    const SALES_ORDERS: &'static str = "sales_orders";
    /// `sales_order_line` 集合名。
    const SALES_ORDER_LINES: &'static str = "sales_order_lines";
    /// `sales_order_working_copy` 集合名。
    const SALES_ORDER_WORKING_COPIES: &'static str = "sales_order_working_copies";
    /// `sales_order_working_copy_line` 集合名。
    const SALES_ORDER_WORKING_COPY_LINES: &'static str = "sales_order_working_copy_lines";
    /// `sales_order_submission` 集合名。
    const SALES_ORDER_SUBMISSIONS: &'static str = "sales_order_submissions";
    /// `sales_order_submission_line` 集合名。
    const SALES_ORDER_SUBMISSION_LINES: &'static str = "sales_order_submission_lines";
    /// `sales_order_revision` 集合名。
    const SALES_ORDER_REVISIONS: &'static str = "sales_order_revisions";
    /// `sales_order_revision_line` 集合名。
    const SALES_ORDER_REVISION_LINES: &'static str = "sales_order_revision_lines";
    /// `sales_order_goods_service_line_revision` 集合名。
    const SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS: &'static str = "sales_order_goods_service_line_revisions";
    /// `sales_order_voucher_line_revision` 集合名。
    const SALES_ORDER_VOUCHER_LINE_REVISIONS: &'static str = "sales_order_voucher_line_revisions";

    /// 销售单列表筛选条件类型（定义见 `repository::sales_order`）。
    type SalesOrderFilter;

    /// 工作副本列表筛选条件类型（定义见 `repository::sales_order`）。
    type WorkingCopyFilter;

    /// 提交历史列表筛选条件类型（定义见 `repository::sales_order`）。
    type SubmissionFilter;

    /// 获取 `sales_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrder>`。
    fn sales_orders(&self) -> Repository<'_, SalesOrder>;

    /// 获取 `sales_order_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderLine>`。
    fn sales_order_lines(&self) -> Repository<'_, SalesOrderLine>;

    /// 获取 `sales_order_working_copy` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderWorkingCopy>`。
    fn sales_order_working_copies(&self) -> Repository<'_, SalesOrderWorkingCopy>;

    /// 获取 `sales_order_working_copy_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderWorkingCopyLine>`。
    fn sales_order_working_copy_lines(&self) -> Repository<'_, SalesOrderWorkingCopyLine>;

    /// 获取 `sales_order_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderSubmission>`。
    fn sales_order_submissions(&self) -> Repository<'_, SalesOrderSubmission>;

    /// 获取 `sales_order_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderSubmissionLine>`。
    fn sales_order_submission_lines(&self) -> Repository<'_, SalesOrderSubmissionLine>;

    /// 获取 `sales_order_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderRevision>`。
    fn sales_order_revisions(&self) -> Repository<'_, SalesOrderRevision>;

    /// 获取 `sales_order_revision_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderRevisionLine>`。
    fn sales_order_revision_lines(&self) -> Repository<'_, SalesOrderRevisionLine>;

    /// 获取 `sales_order_goods_service_line_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderGoodsServiceLineRevision>`。
    fn sales_order_goods_service_line_revisions(&self) -> Repository<'_, SalesOrderGoodsServiceLineRevision>;

    /// 获取 `sales_order_voucher_line_revision` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_order::SalesOrderVoucherLineRevision>`。
    fn sales_order_voucher_line_revisions(&self) -> Repository<'_, SalesOrderVoucherLineRevision>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SalesOrderRepository` 实例。
    fn sales_order(&self) -> SalesOrderRepository<'_>;
}

impl SalesOrderExt for Database {
    type SalesOrderFilter = SalesOrderFilter;
    type WorkingCopyFilter = WorkingCopyFilter;
    type SubmissionFilter = SubmissionFilter;

    fn sales_orders(&self) -> Repository<'_, SalesOrder> {
        Repository::new(self, Self::SALES_ORDERS)
    }

    fn sales_order_lines(&self) -> Repository<'_, SalesOrderLine> {
        Repository::new(self, Self::SALES_ORDER_LINES)
    }

    fn sales_order_working_copies(&self) -> Repository<'_, SalesOrderWorkingCopy> {
        Repository::new(self, Self::SALES_ORDER_WORKING_COPIES)
    }

    fn sales_order_working_copy_lines(&self) -> Repository<'_, SalesOrderWorkingCopyLine> {
        Repository::new(self, Self::SALES_ORDER_WORKING_COPY_LINES)
    }

    fn sales_order_submissions(&self) -> Repository<'_, SalesOrderSubmission> {
        Repository::new(self, Self::SALES_ORDER_SUBMISSIONS)
    }

    fn sales_order_submission_lines(&self) -> Repository<'_, SalesOrderSubmissionLine> {
        Repository::new(self, Self::SALES_ORDER_SUBMISSION_LINES)
    }

    fn sales_order_revisions(&self) -> Repository<'_, SalesOrderRevision> {
        Repository::new(self, Self::SALES_ORDER_REVISIONS)
    }

    fn sales_order_revision_lines(&self) -> Repository<'_, SalesOrderRevisionLine> {
        Repository::new(self, Self::SALES_ORDER_REVISION_LINES)
    }

    fn sales_order_goods_service_line_revisions(&self) -> Repository<'_, SalesOrderGoodsServiceLineRevision> {
        Repository::new(self, Self::SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS)
    }

    fn sales_order_voucher_line_revisions(&self) -> Repository<'_, SalesOrderVoucherLineRevision> {
        Repository::new(self, Self::SALES_ORDER_VOUCHER_LINE_REVISIONS)
    }

    fn sales_order(&self) -> SalesOrderRepository<'_> {
        SalesOrderRepository::new(self)
    }
}
