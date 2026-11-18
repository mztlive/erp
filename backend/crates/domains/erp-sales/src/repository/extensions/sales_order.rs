//! 域 D13 `sales_order` 仓储访问器。
//!
//! 集合名常量定义为 trait 关联常量（唯一权威来源，conventions §4.3「Repository
//! 与索引共用同一常量」）：`indexes/` 与 `repository/` 均为冻结声明下的私有
//! 子树，模块路径无法互相引用；关联常量随 trait 公开可达，两侧统一取
//! `<mongodb::Database as SalesOrderExt>::SALES_ORDERS` 等值。

use mongodb::Database;

use super::super::sales_order::{
    SalesOrderDomainRepository, SalesOrderFilter, SubmissionFilter, WorkingCopyFilter,
};
use crate::repository::owned::{
    SalesOrderGoodsServiceLineRevisionRepository, SalesOrderLineRepository, SalesOrderRepository,
    SalesOrderRevisionLineRepository, SalesOrderRevisionRepository, SalesOrderSubmissionLineRepository,
    SalesOrderSubmissionRepository, SalesOrderVoucherLineRevisionRepository,
    SalesOrderWorkingCopyLineRepository, SalesOrderWorkingCopyRepository,
};

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

    /// 从当前数据库取得 `sales_order` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_orders(&self) -> SalesOrderRepository<'_>;

    /// 从当前数据库取得 `sales_order_line` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_lines(&self) -> SalesOrderLineRepository<'_>;

    /// 从当前数据库取得 `sales_order_working_copy` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_working_copies(&self) -> SalesOrderWorkingCopyRepository<'_>;

    /// 从当前数据库取得 `sales_order_working_copy_line` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_working_copy_lines(&self) -> SalesOrderWorkingCopyLineRepository<'_>;

    /// 从当前数据库取得 `sales_order_submission` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_submissions(&self) -> SalesOrderSubmissionRepository<'_>;

    /// 从当前数据库取得 `sales_order_submission_line` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_submission_lines(&self) -> SalesOrderSubmissionLineRepository<'_>;

    /// 从当前数据库取得 `sales_order_revision` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_revisions(&self) -> SalesOrderRevisionRepository<'_>;

    /// 从当前数据库取得 `sales_order_revision_line` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_revision_lines(&self) -> SalesOrderRevisionLineRepository<'_>;

    /// 从当前数据库取得 `sales_order_goods_service_line_revision` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_goods_service_line_revisions(&self) -> SalesOrderGoodsServiceLineRevisionRepository<'_>;

    /// 从当前数据库取得 `sales_order_voucher_line_revision` 集合的类型化仓储，供调用方按自身执行器访问。
    fn sales_order_voucher_line_revisions(&self) -> SalesOrderVoucherLineRevisionRepository<'_>;

    /// 取得销售单领域仓储，供调用方使用原执行器处理销售单、工作副本和修订。
    fn sales_order(&self) -> SalesOrderDomainRepository<'_>;
}

impl SalesOrderExt for Database {
    type SalesOrderFilter = SalesOrderFilter;
    type WorkingCopyFilter = WorkingCopyFilter;
    type SubmissionFilter = SubmissionFilter;

    fn sales_orders(&self) -> SalesOrderRepository<'_> {
        SalesOrderRepository::new(self, Self::SALES_ORDERS)
    }

    fn sales_order_lines(&self) -> SalesOrderLineRepository<'_> {
        SalesOrderLineRepository::new(self, Self::SALES_ORDER_LINES)
    }

    fn sales_order_working_copies(&self) -> SalesOrderWorkingCopyRepository<'_> {
        SalesOrderWorkingCopyRepository::new(self, Self::SALES_ORDER_WORKING_COPIES)
    }

    fn sales_order_working_copy_lines(&self) -> SalesOrderWorkingCopyLineRepository<'_> {
        SalesOrderWorkingCopyLineRepository::new(self, Self::SALES_ORDER_WORKING_COPY_LINES)
    }

    fn sales_order_submissions(&self) -> SalesOrderSubmissionRepository<'_> {
        SalesOrderSubmissionRepository::new(self, Self::SALES_ORDER_SUBMISSIONS)
    }

    fn sales_order_submission_lines(&self) -> SalesOrderSubmissionLineRepository<'_> {
        SalesOrderSubmissionLineRepository::new(self, Self::SALES_ORDER_SUBMISSION_LINES)
    }

    fn sales_order_revisions(&self) -> SalesOrderRevisionRepository<'_> {
        SalesOrderRevisionRepository::new(self, Self::SALES_ORDER_REVISIONS)
    }

    fn sales_order_revision_lines(&self) -> SalesOrderRevisionLineRepository<'_> {
        SalesOrderRevisionLineRepository::new(self, Self::SALES_ORDER_REVISION_LINES)
    }

    fn sales_order_goods_service_line_revisions(&self) -> SalesOrderGoodsServiceLineRevisionRepository<'_> {
        SalesOrderGoodsServiceLineRevisionRepository::new(
            self,
            Self::SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS,
        )
    }

    fn sales_order_voucher_line_revisions(&self) -> SalesOrderVoucherLineRevisionRepository<'_> {
        SalesOrderVoucherLineRevisionRepository::new(self, Self::SALES_ORDER_VOUCHER_LINE_REVISIONS)
    }

    fn sales_order(&self) -> SalesOrderDomainRepository<'_> {
        SalesOrderDomainRepository::new(self)
    }
}
