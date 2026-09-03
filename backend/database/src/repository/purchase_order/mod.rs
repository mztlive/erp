//! 域 D15 `purchase_order` 仓储：采购单、采购提交(+行)、采购生效版本(+行)、
//! 采购行→销售行分配与采购变更单(+提交/行)（数据模型 §6.6，页面 W08）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：`update`/
//! `soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本目录按集合拆分投影行、筛选与
//! 域特有查询：
//! - [`order`]：采购主表列表投影与按单号/来源单查询；
//! - [`submission`]：不可变提交列表（财务审核队列）与明细批量取回；
//! - [`revision`]：生效版本与明细批量取回；
//! - [`allocation`]：采购行↔销售行双向批量查询（§6.6 双向查询索引）；
//! - [`change`]：采购变更单与变更提交/明细读取；
//! - [`PurchaseOrderRepository`] 承载跨集合多步骤写入（必须收到事务执行器）。
//!
//! 提交/版本/分配是事实或修订类集合，**不提供软删除方法**；采购主表与采购变更单
//! 是可编辑单据草稿（`StableBase`），可软删除与恢复。

mod allocation;
mod center_facts;
mod change;
mod common;
mod coverage;
mod creation_basis;
mod list_facts;
mod order;
mod revision;
mod submission;

pub use center_facts::{load_purchase_order_center_facts, PurchaseOrderCenterFacts};
pub use coverage::load_procurement_coverage_facts;
pub use creation_basis::load_creation_basis_facts;
pub use list_facts::{load_purchase_order_list_page, PurchaseOrderListFacts};
pub use order::{PurchaseOrderFilter, PurchaseOrderRow};
pub use submission::PurchaseOrderSubmissionFilter;

use entities::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseChangeSubmissionLine, PurchaseOrder,
    PurchaseOrderRevision, PurchaseOrderRevisionLine, PurchaseOrderSubmission, PurchaseOrderSubmissionLine,
};
use mongodb::Database;

use super::extensions::PurchaseOrderExt;
use crate::executor::Executor;
use crate::mongo_ops;
use crate::{Repository, Result};

/// `purchase_order` 集合名（单一来源：`PurchaseOrderExt` 关联常量）。
const PURCHASE_ORDERS: &str = <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDERS;
/// `purchase_order_submission` 集合名。
const PURCHASE_ORDER_SUBMISSIONS: &str = <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSIONS;
/// `purchase_order_submission_line` 集合名。
const PURCHASE_ORDER_SUBMISSION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_SUBMISSION_LINES;
/// `purchase_order_revision` 集合名。
const PURCHASE_ORDER_REVISIONS: &str = <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISIONS;
/// `purchase_order_revision_line` 集合名。
const PURCHASE_ORDER_REVISION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_ORDER_REVISION_LINES;
/// `purchase_change_order` 集合名。
const PURCHASE_CHANGE_ORDERS: &str = <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_ORDERS;
/// `purchase_change_submission` 集合名。
const PURCHASE_CHANGE_SUBMISSIONS: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSIONS;
/// `purchase_change_submission_line` 集合名。
const PURCHASE_CHANGE_SUBMISSION_LINES: &str =
    <mongodb::Database as PurchaseOrderExt>::PURCHASE_CHANGE_SUBMISSION_LINES;

/// D15 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的跨集合写入入口，
/// 由 `PurchaseOrderExt::purchase_order()` 访问。
pub struct PurchaseOrderRepository<'a> {
    db: &'a Database,
}

impl<'a> PurchaseOrderRepository<'a> {
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

    /// 提交采购草稿并形成不可变提交（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_order_submission`、`purchase_order_submission_line`（批量）
    /// 并乐观锁更新采购主表（提交序号、状态、当前提交指针，§6.6：进入待审核时
    /// 头、行冻结），保证「提交 + 明细 + 主表指针」原子可见。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 提交与明细各自自动提交，主表 CAS 失败会留下只有提交没有指针的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `order` - 已执行 `PurchaseOrder::submit_for_review` 的采购主表（带期望版本）
    /// * `submission` - 待写入的不可变提交
    /// * `lines` - 待写入的提交明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当提交序号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、主表版本
    /// 冲突（[`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时返回错误。
    pub async fn create_purchase_submission(
        &self,
        order: &mut PurchaseOrder,
        submission: &PurchaseOrderSubmission,
        lines: &[PurchaseOrderSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseOrderSubmission>(PURCHASE_ORDER_SUBMISSIONS),
            submission,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<PurchaseOrderSubmissionLine>(PURCHASE_ORDER_SUBMISSION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, PURCHASE_ORDERS)
            .update(order, executor)
            .await?;
        Ok(())
    }

    /// 形成采购生效版本（跨集合多步骤写入）。
    ///
    /// 财务审核通过时把已通过提交原样复制为 `purchase_order_revision` 与
    /// `purchase_order_revision_line`（§6.6/§8.1 第 4 条），保证版本与明细原子可见。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时两笔写入各自自动提交，
    /// 中途失败会留下只有版本没有明细的半成品。
    ///
    /// # 参数
    /// * `revision` - 待写入的生效版本
    /// * `lines` - 待写入的版本明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当版本号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB
    /// 写入失败时返回错误。
    pub async fn create_effective_revision(
        &self,
        revision: &PurchaseOrderRevision,
        lines: &[PurchaseOrderRevisionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseOrderRevision>(PURCHASE_ORDER_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<PurchaseOrderRevisionLine>(PURCHASE_ORDER_REVISION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 形成采购变更提交（跨集合多步骤写入）。
    ///
    /// 依次写入 `purchase_change_submission`、`purchase_change_submission_line`
    /// 并乐观锁更新采购变更单（当前目标提交与内容指纹，§6.6），保证
    /// 「变更提交 + 明细 + 变更单指针」原子可见。
    /// **必须收到事务执行器**：传入 `NoTransaction` 时各笔写入各自自动提交，
    /// 主表 CAS 失败会留下只有提交没有指针的半成品。
    ///
    /// # 参数
    /// * `change_order` - 已执行内容更新的采购变更单（带期望版本）
    /// * `submission` - 待写入的变更提交
    /// * `lines` - 待写入的变更提交明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当提交序号唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、变更单
    /// 版本冲突（[`crate::Error::OptimisticLockingError`]）或 MongoDB 写入失败时
    /// 返回错误。
    pub async fn create_change_submission(
        &self,
        change_order: &mut PurchaseChangeOrder,
        submission: &PurchaseChangeSubmission,
        lines: &[PurchaseChangeSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<PurchaseChangeSubmission>(PURCHASE_CHANGE_SUBMISSIONS),
            submission,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<PurchaseChangeSubmissionLine>(PURCHASE_CHANGE_SUBMISSION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, PURCHASE_CHANGE_ORDERS)
            .update(change_order, executor)
            .await?;
        Ok(())
    }
}
