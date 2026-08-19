//! 域 D13 `sales_order` 仓储（目录形态）：sales_order(+_line)、
//! sales_order_working_copy(+_line)、sales_order_submission(+_line)、
//! sales_order_revision(+_line) 及两个版本行子类型（页面：W05）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；事实类集合
//! （提交/版本/版本行）**不提供软删除/恢复方法**（conventions §4 约定）。
//! 本模块补充域特有查询与跨集合多步骤写入入口；集合名常量统一取
//! `SalesOrderExt` 关联常量（单一权威来源，conventions §4.3）。
//!
//! 数据模型 §6.5 提交/生效两个事务入口在此落地为多步骤方法：
//! - [`SalesOrderRepository::submit_working_copy`]（工作副本 → 提交快照）；
//! - [`SalesOrderRepository::formalize_submission`]（提交 → 正式版本）。
//!
//! 两者都必须收到事务执行器（P2 §2.1）。

mod order;
mod revision;
mod submission;
mod working_copy;

use mongodb::bson::{doc, Document};
use mongodb::Database;
pub use order::SalesOrderFilter;
pub use submission::SubmissionFilter;
pub use working_copy::WorkingCopyFilter;

use super::extensions::SalesOrderExt;

/// `sales_order` 集合名（单一来源：`SalesOrderExt` 关联常量）。
const SALES_ORDERS: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDERS;
/// `sales_order_working_copy` 集合名。
const SALES_ORDER_WORKING_COPIES: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_WORKING_COPIES;
/// `sales_order_submission` 集合名。
const SALES_ORDER_SUBMISSIONS: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_SUBMISSIONS;
/// `sales_order_submission_line` 集合名。
const SALES_ORDER_SUBMISSION_LINES: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_SUBMISSION_LINES;
/// `sales_order_revision` 集合名。
const SALES_ORDER_REVISIONS: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_REVISIONS;
/// `sales_order_revision_line` 集合名。
const SALES_ORDER_REVISION_LINES: &str = <mongodb::Database as SalesOrderExt>::SALES_ORDER_REVISION_LINES;
/// `sales_order_goods_service_line_revision` 集合名。
const SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_GOODS_SERVICE_LINE_REVISIONS;
/// `sales_order_voucher_line_revision` 集合名。
const SALES_ORDER_VOUCHER_LINE_REVISIONS: &str =
    <mongodb::Database as SalesOrderExt>::SALES_ORDER_VOUCHER_LINE_REVISIONS;

/// D13 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口（提交快照化、版本生效、明细替换），由
/// `SalesOrderExt::sales_order()` 访问。
pub struct SalesOrderRepository<'a> {
    db: &'a Database,
}

impl<'a> SalesOrderRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }
}

/// 构建排序文档。
///
/// # 参数
/// * `sort_by` - 排序字段；`None` 时默认 `created_at`
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { sort_by.unwrap_or("created_at"): direction }
}
