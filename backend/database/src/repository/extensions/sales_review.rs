//! 域 D14 `sales_review` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SalesReviewExt>::SALES_ORDER_REVIEWS` 等值。

use entities::sales_review::{
    LowMarginManagerConfirmation, ProcurementConfirmation, ProcurementConfirmationLine, SalesChangeOrder,
    SalesChangeReview, SalesChangeSubmission, SalesChangeSubmissionLine, SalesOrderReview,
};
use mongodb::Database;

use super::super::sales_review::{
    ProcurementConfirmationFilter, SalesChangeOrderFilter, SalesOrderReviewFilter, SalesReviewRepository,
};
use crate::Repository;

/// 域 D14 仓储访问器。
pub trait SalesReviewExt {
    /// `sales_order_review` 集合名。
    const SALES_ORDER_REVIEWS: &'static str = "sales_order_reviews";
    /// `low_margin_manager_confirmation` 集合名。
    const LOW_MARGIN_MANAGER_CONFIRMATIONS: &'static str = "low_margin_manager_confirmations";
    /// `procurement_confirmation` 集合名。
    const PROCUREMENT_CONFIRMATIONS: &'static str = "procurement_confirmations";
    /// `procurement_confirmation_line` 集合名。
    const PROCUREMENT_CONFIRMATION_LINES: &'static str = "procurement_confirmation_lines";
    /// `sales_change_order` 集合名。
    const SALES_CHANGE_ORDERS: &'static str = "sales_change_orders";
    /// `sales_change_submission` 集合名。
    const SALES_CHANGE_SUBMISSIONS: &'static str = "sales_change_submissions";
    /// `sales_change_submission_line` 集合名。
    const SALES_CHANGE_SUBMISSION_LINES: &'static str = "sales_change_submission_lines";
    /// `sales_change_review` 集合名。
    const SALES_CHANGE_REVIEWS: &'static str = "sales_change_reviews";

    /// 审批记录列表筛选条件类型（定义见 `repository::sales_review`）。
    type SalesOrderReviewFilter;

    /// 采购确认列表筛选条件类型（定义见 `repository::sales_review`）。
    type ProcurementConfirmationFilter;

    /// 销售变更单列表筛选条件类型（定义见 `repository::sales_review`）。
    type SalesChangeOrderFilter;

    /// 获取 `sales_order_review` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::SalesOrderReview>`。
    fn sales_order_reviews(&self) -> Repository<'_, SalesOrderReview>;

    /// 获取低毛利上级确认事实仓储。
    fn low_margin_manager_confirmations(&self) -> Repository<'_, LowMarginManagerConfirmation>;

    /// 获取 `procurement_confirmation` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::ProcurementConfirmation>`。
    fn procurement_confirmations(&self) -> Repository<'_, ProcurementConfirmation>;

    /// 获取 `procurement_confirmation_line` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::ProcurementConfirmationLine>`。
    fn procurement_confirmation_lines(&self) -> Repository<'_, ProcurementConfirmationLine>;

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

    /// 获取 `sales_change_review` 集合的 Repository。
    ///
    /// # 返回
    /// 返回 `Repository<'_, entities::sales_review::SalesChangeReview>`。
    fn sales_change_reviews(&self) -> Repository<'_, SalesChangeReview>;

    /// 获取承载跨集合事务写入的域专用仓储。
    ///
    /// # 返回
    /// 返回 `SalesReviewRepository` 实例。
    fn sales_review(&self) -> SalesReviewRepository<'_>;
}

impl SalesReviewExt for Database {
    type SalesOrderReviewFilter = SalesOrderReviewFilter;
    type ProcurementConfirmationFilter = ProcurementConfirmationFilter;
    type SalesChangeOrderFilter = SalesChangeOrderFilter;

    fn sales_order_reviews(&self) -> Repository<'_, SalesOrderReview> {
        Repository::new(self, Self::SALES_ORDER_REVIEWS)
    }

    fn low_margin_manager_confirmations(&self) -> Repository<'_, LowMarginManagerConfirmation> {
        Repository::new(self, Self::LOW_MARGIN_MANAGER_CONFIRMATIONS)
    }

    fn procurement_confirmations(&self) -> Repository<'_, ProcurementConfirmation> {
        Repository::new(self, Self::PROCUREMENT_CONFIRMATIONS)
    }

    fn procurement_confirmation_lines(&self) -> Repository<'_, ProcurementConfirmationLine> {
        Repository::new(self, Self::PROCUREMENT_CONFIRMATION_LINES)
    }

    fn sales_change_orders(&self) -> Repository<'_, SalesChangeOrder> {
        Repository::new(self, Self::SALES_CHANGE_ORDERS)
    }

    fn sales_change_submissions(&self) -> Repository<'_, SalesChangeSubmission> {
        Repository::new(self, Self::SALES_CHANGE_SUBMISSIONS)
    }

    fn sales_change_submission_lines(&self) -> Repository<'_, SalesChangeSubmissionLine> {
        Repository::new(self, Self::SALES_CHANGE_SUBMISSION_LINES)
    }

    fn sales_change_reviews(&self) -> Repository<'_, SalesChangeReview> {
        Repository::new(self, Self::SALES_CHANGE_REVIEWS)
    }

    fn sales_review(&self) -> SalesReviewRepository<'_> {
        SalesReviewRepository::new(self)
    }
}
