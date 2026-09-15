//! 域 D14 `sales_review` 仓储访问器。
//!
//! 仅保留销售变更单与变更提交集合。旧采购确认、低毛利确认、卡券审批记录与
//! 变更复核集合已删除。

use mongodb::Database;

use super::super::sales_review::{SalesChangeOrderFilter, SalesReviewRepository};
use crate::repository::owned::{
    SalesChangeOrderRepository, SalesChangeSubmissionLineRepository, SalesChangeSubmissionRepository,
};

/// 域 D14 仓储访问器。
pub trait SalesReviewExt {
    /// `sales_change_order` 集合名。
    const SALES_CHANGE_ORDERS: &'static str = "sales_change_orders";
    /// `sales_change_submission` 集合名。
    const SALES_CHANGE_SUBMISSIONS: &'static str = "sales_change_submissions";
    /// `sales_change_submission_line` 集合名。
    const SALES_CHANGE_SUBMISSION_LINES: &'static str = "sales_change_submission_lines";

    /// 销售变更单列表筛选条件类型。
    type SalesChangeOrderFilter;

    /// 获取 `sales_change_order` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SalesChangeOrderRepository<'_>`。
    fn sales_change_orders(&self) -> SalesChangeOrderRepository<'_>;

    /// 获取 `sales_change_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SalesChangeSubmissionRepository<'_>`。
    fn sales_change_submissions(&self) -> SalesChangeSubmissionRepository<'_>;

    /// 获取 `sales_change_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `SalesChangeSubmissionLineRepository<'_>`。
    fn sales_change_submission_lines(&self) -> SalesChangeSubmissionLineRepository<'_>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SalesReviewRepository` 实例。
    fn sales_review(&self) -> SalesReviewRepository<'_>;
}

impl SalesReviewExt for Database {
    type SalesChangeOrderFilter = SalesChangeOrderFilter;

    fn sales_change_orders(&self) -> SalesChangeOrderRepository<'_> {
        SalesChangeOrderRepository::new(self, Self::SALES_CHANGE_ORDERS)
    }

    fn sales_change_submissions(&self) -> SalesChangeSubmissionRepository<'_> {
        SalesChangeSubmissionRepository::new(self, Self::SALES_CHANGE_SUBMISSIONS)
    }

    fn sales_change_submission_lines(&self) -> SalesChangeSubmissionLineRepository<'_> {
        SalesChangeSubmissionLineRepository::new(self, Self::SALES_CHANGE_SUBMISSION_LINES)
    }

    fn sales_review(&self) -> SalesReviewRepository<'_> {
        SalesReviewRepository::new(self)
    }
}
