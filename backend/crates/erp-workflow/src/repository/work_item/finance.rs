use crate::entity::work_item::{
    FinanceResponsibilityOperation, FinanceResponsibilityRule, WorkItem, WorkItemType,
};
use crate::repository::owned::{FinanceResponsibilityRuleRepository, WorkItemRepository};
use mongodb::bson::doc;

use persistence_core::Executor;
use persistence_core::Result;

impl<'a> WorkItemRepository<'a> {
    /// 查询应付子账全部付款执行任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `payable_account_id` - 应付子账稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_payment_execution_by_payable_newest_first(
        &self,
        payable_account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "payable_account",
                "business_object_id": payable_account_id,
                "work_item_type": WorkItemType::SupplierPaymentExecution.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }

    /// 查询应收子账全部销项开票执行任务并把最新任务排在前面。
    ///
    /// # 参数
    /// * `receivable_account_id` - 应收子账稳定身份
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回按更新时间、创建时间倒序排列的全部生命周期任务。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_sales_invoice_execution_by_receivable_newest_first(
        &self,
        receivable_account_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<WorkItem>> {
        self.find_many_sorted(
            doc! {
                "business_object_type": "receivable_account",
                "business_object_id": receivable_account_id,
                "work_item_type": WorkItemType::SalesInvoiceExecution.as_str(),
            },
            doc! { "updated_at": -1, "created_at": -1 },
            executor,
        )
        .await
    }
}

impl<'a> FinanceResponsibilityRuleRepository<'a> {
    /// 查询全部未删除财务责任规则。
    ///
    /// # 返回
    /// 返回按业务、匹配层级和创建时间稳定排序的规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_finance_responsibility_rules(
        &self,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceResponsibilityRule>> {
        self.find_many_sorted(
            doc! {},
            doc! { "operation": 1, "scope": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }

    /// 查询指定业务全部启用财务责任规则。
    ///
    /// # 参数
    /// * `operation` - 供应商付款或销项开票
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前未删除且启用的规则。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_active_finance_responsibility_rules(
        &self,
        operation: FinanceResponsibilityOperation,
        executor: &mut dyn Executor,
    ) -> Result<Vec<FinanceResponsibilityRule>> {
        self.find_many_sorted(
            doc! {
                "operation": operation.as_str(),
                "status": crate::entity::work_item::EnableStatus::Active.as_str(),
            },
            doc! { "scope": 1, "created_at": 1, "id": 1 },
            executor,
        )
        .await
    }
}
