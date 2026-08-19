//! 域 D14 `sales_review` 仓储访问器。
//!
//! 仅保留销售变更单与变更提交集合。旧采购确认、低毛利确认、卡券审批记录与
//! 变更复核集合已删除。

use entities::sales_review::{SalesChangeOrder, SalesChangeSubmission, SalesChangeSubmissionLine};
use mongodb::Database;

use super::super::sales_review::{SalesChangeOrderFilter, SalesReviewRepository};
use crate::Repository;

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
    /// 返回 `Repository<'_, entities::sales_review::SalesChangeOrder>`。
    fn sales_change_orders(&self) -> Repository<'_, SalesChangeOrder>;

    /// 获取 `sales_change_submission` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::SalesChangeSubmission>`。
    fn sales_change_submissions(&self) -> Repository<'_, SalesChangeSubmission>;

    /// 获取 `sales_change_submission_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::SalesChangeSubmissionLine>`。
    fn sales_change_submission_lines(&self) -> Repository<'_, SalesChangeSubmissionLine>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SalesReviewRepository` 实例。
    fn sales_review(&self) -> SalesReviewRepository<'_>;
}

impl SalesReviewExt for Database {
    type SalesChangeOrderFilter = SalesChangeOrderFilter;

    fn sales_change_orders(&self) -> Repository<'_, SalesChangeOrder> {
        Repository::new(self, Self::SALES_CHANGE_ORDERS)
    }

    fn sales_change_submissions(&self) -> Repository<'_, SalesChangeSubmission> {
        Repository::new(self, Self::SALES_CHANGE_SUBMISSIONS)
    }

    fn sales_change_submission_lines(&self) -> Repository<'_, SalesChangeSubmissionLine> {
        Repository::new(self, Self::SALES_CHANGE_SUBMISSION_LINES)
    }

    fn sales_review(&self) -> SalesReviewRepository<'_> {
        SalesReviewRepository::new(self)
    }
}
