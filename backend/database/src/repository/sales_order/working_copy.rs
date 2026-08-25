//! `sales_order_working_copy` 与 `sales_order_working_copy_line` 仓储。
//!
//! 工作副本承载页面自动保存的可编辑草稿；「同一销售单和编辑目的同时最多一个
//! 有效工作副本」由部分唯一索引 `uk_sales_order_working_copies_active_per_purpose`
//! 保证（理由与回滚方式见 `indexes::sales_order`）。

use entities::ids::SalesChangeOrderId;
use entities::sales_order::{
    SalesOrderId, SalesOrderSubmission, SalesOrderSubmissionLine, SalesOrderWorkingCopy,
    SalesOrderWorkingCopyId, SalesOrderWorkingCopyLine, WorkingCopyStatus, WorkingPurpose,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};

use super::super::{Pagination, QueryFilter, Repository};
use super::{
    SalesOrderRepository, SALES_ORDER_SUBMISSIONS, SALES_ORDER_SUBMISSION_LINES, SALES_ORDER_WORKING_COPIES,
};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// 工作副本列表筛选条件。
#[derive(Debug, Clone)]
pub struct WorkingCopyFilter {
    /// 稳定销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderWorkingCopyId>,
    /// 编辑目的；`None` 表示不筛选。
    pub working_purpose: Option<WorkingPurpose>,
    /// 草稿状态；`None` 表示不筛选。
    pub status: Option<WorkingCopyStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（Service 层白名单校验后传入，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for WorkingCopyFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(working_purpose) = self.working_purpose {
            filter.insert("working_purpose", working_purpose.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for WorkingCopyFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderWorkingCopy> {
    /// 按销售单与编辑目的查找有效工作副本（`Editing`/`Conflict`）。
    ///
    /// 「同一销售单和编辑目的同时最多一个有效工作副本」由部分唯一索引
    /// `uk_sales_order_working_copies_active_per_purpose` 保证（数据模型 §6.5）。
    ///
    /// # 参数
    /// * `sales_order_id` - 稳定销售单
    /// * `working_purpose` - 编辑目的
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回有效工作副本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_active_by_order_and_purpose(
        &self,
        sales_order_id: &SalesOrderId,
        working_purpose: WorkingPurpose,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderWorkingCopy>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "working_purpose": working_purpose.as_str(),
                "status": {
                    "$in": [
                        WorkingCopyStatus::Editing.as_str(),
                        WorkingCopyStatus::Conflict.as_str(),
                    ]
                },
            },
            executor,
        )
        .await
    }

    /// 按销售变更单查找其绑定的工作副本。
    ///
    /// # 参数
    /// * `sales_change_order_id` - 销售变更单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回绑定的销售变更工作副本；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_sales_change_order(
        &self,
        sales_change_order_id: &SalesChangeOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderWorkingCopy>> {
        self.find_one(
            doc! {
                "sales_change_order_id": sales_change_order_id.to_string(),
                "working_purpose": WorkingPurpose::SalesChange.as_str(),
            },
            executor,
        )
        .await
    }

    /// 查找销售变更再次提交时可使用的工作副本。
    ///
    /// 优先返回仍处于有效编辑态的副本；撤回后无有效副本时回退到该变更单已绑定的
    /// 已提交副本，以保持提交序号递增而不复制草稿。
    ///
    /// # 参数
    /// * `sales_order_id` - 原销售单
    /// * `sales_change_order_id` - 销售变更单
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回可再次提交的工作副本；两种来源均不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_resubmittable_sales_change_copy(
        &self,
        sales_order_id: &SalesOrderId,
        sales_change_order_id: &SalesChangeOrderId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderWorkingCopy>> {
        let active = self
            .find_active_by_order_and_purpose(sales_order_id, WorkingPurpose::SalesChange, executor)
            .await?;
        if active.is_some() {
            return Ok(active);
        }
        self.find_by_sales_change_order(sales_change_order_id, executor)
            .await
    }
}

impl<'a> Repository<'a, SalesOrderWorkingCopyLine> {
    /// 列出工作副本的全部明细行（按行号升序）。
    ///
    /// # 参数
    /// * `working_copy_id` - 所属工作副本
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按行号升序的明细行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_lines_by_working_copy(
        &self,
        working_copy_id: &SalesOrderWorkingCopyId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderWorkingCopyLine>> {
        self.find_many_sorted(
            doc! { "working_copy_id": working_copy_id.to_string() },
            doc! { "line_no": 1 },
            executor,
        )
        .await
    }
}

impl<'a> SalesOrderRepository<'a> {
    /// 提交工作副本：把草稿头、行原样复制成不可变提交快照，再锁定草稿。
    ///
    /// 依次写入 `sales_order_submission`、`sales_order_submission_line` 并 CAS
    /// 更新工作副本为 `Submitted`（数据模型 §6.5：提交事务锁定工作副本，禁止
    /// 审批直接读取仍可变的工作副本）。调用方须先在实体上完成
    /// `SalesOrderWorkingCopy::submit()` 状态迁移（本层不做业务判定）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction` 时
    /// 中途失败会留下没有提交头的明细或未锁定的草稿；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `working_copy` - 已迁移到 `Submitted` 的工作副本（成功后内存版本递增）
    /// * `submission` - 不可变提交快照头
    /// * `lines` - 不可变提交快照明细
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）、乐观锁冲突或
    /// MongoDB 写入失败时返回错误。
    pub async fn submit_working_copy(
        &self,
        working_copy: &mut SalesOrderWorkingCopy,
        submission: &SalesOrderSubmission,
        lines: &[SalesOrderSubmissionLine],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderSubmission>(SALES_ORDER_SUBMISSIONS),
            submission,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SalesOrderSubmissionLine>(SALES_ORDER_SUBMISSION_LINES),
            lines.to_vec(),
            executor,
        )
        .await?;
        Repository::new(self.db, SALES_ORDER_WORKING_COPIES)
            .update(working_copy, executor)
            .await
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn working_copy_filter_applies_optional_fields_and_deleted_filter() {
        let filter = WorkingCopyFilter {
            sales_order_id: Some(entities::sales_order::SalesOrderWorkingCopyId::new("wc-1")),
            working_purpose: Some(WorkingPurpose::FirstSubmission),
            status: Some(WorkingCopyStatus::Editing),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "wc-1");
        assert_eq!(document.get_str("working_purpose").unwrap(), "FIRST_SUBMISSION");
        assert_eq!(document.get_str("status").unwrap(), "EDITING");
    }
}
