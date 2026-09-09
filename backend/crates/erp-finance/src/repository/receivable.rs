//! 域 D18 `receivable` 仓储：receivable_account、receivable_entry、
//! receipt_allocation、invoice、sales_invoice_allocation。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类；本模块只补充域特有查询、
//! **条件核销进度更新**（原子写入口，不超额核销，P2 计划 §5）与跨集合多步骤
//! 事务写入入口。集合名常量统一从 `indexes::receivable` 导入。
//!
//! 正式事实集合（分录、复核、抵销、分配）过账后不可更新或删除，**不提供软删除
//! 方法**；`receivable_account` 与 `invoice` 是稳定主表类，可软删除与恢复。
//!
//! 筛选/行类型定义在职责子模块，经本模块重新导出并由 `ReceivableExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use mongodb::bson::{doc, Document};
use mongodb::Database;

use super::extensions::ReceivableExt;

mod account;
mod command;
mod entry;
mod invoice;
mod receipt;

pub mod customer_center;

mod snapshot;

mod sales_order_summary;

pub use account::{ReceivableAccountFilter, SettlementBatchResult};
pub use command::{ReceivableListScope, ScopedCustomerReceiptQuery, ScopedInvoiceQuery};
pub use invoice::InvoiceFilter;
pub use receipt::CustomerReceiptFilter;

/// `receivable_entry` 集合名（单一来源：`ReceivableExt` 关联常量）。
const RECEIVABLE_ENTRIES: &str = <mongodb::Database as ReceivableExt>::RECEIVABLE_ENTRIES;

/// D18 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ReceivableExt::receivable()` 访问。
pub struct ReceivableRepository<'a> {
    db: &'a Database,
}

impl<'a> ReceivableRepository<'a> {
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

/// 构建排序文档：字段名经白名单映射，未命中回退 `created_at` 降序。
///
/// # 参数
/// * `sort_by` - 排序字段（白名单内有效）
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
/// * `allowed` - 允许的排序字段名集合（防止透传任意字段名）
///
/// # 返回
/// 返回排序条件文档。
pub(super) fn sort_doc(sort_by: Option<&str>, sort_ascending: bool, allowed: &[&str]) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    let field = sort_by
        .filter(|name| allowed.contains(name))
        .unwrap_or("created_at");
    doc! { field: direction, "id": direction }
}

pub mod money_progress;
