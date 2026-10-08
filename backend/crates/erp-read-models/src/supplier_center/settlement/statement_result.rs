//! 在组合层为原命令容器替换结算单展示类型，不改变正式结果语义。

use erp_supply::dto::supplier_settlement::{
    SettlementDraftCommandResult, SubmitSettlementReviewResult, SupplierSettlementStatementView,
};

use super::display_dto::SettlementStatementDisplayView;
use super::dto::SettlementReviewDecisionResult;

/// 可附加结算单名称的领域结果；具体名称读取只发生在读模型。
pub trait SettlementStatementResult {
    /// 带名称投影的原结果容器。
    type Display;

    /// 读取原结果中的结算单事实。
    ///
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回结算单借用。
    /// # 错误
    /// 无。
    fn statement(&self) -> &SupplierSettlementStatementView;

    /// 用名称视图替换结果中的结算单，保留其它正式事实。
    ///
    /// # 参数
    /// * `statement` - 同一结算单的名称投影。
    /// # 返回
    /// 返回原结果容器的展示类型。
    /// # 错误
    /// 无。
    fn with_statement(self, statement: SettlementStatementDisplayView) -> Self::Display;
}

impl SettlementStatementResult for SupplierSettlementStatementView {
    type Display = SettlementStatementDisplayView;

    fn statement(&self) -> &SupplierSettlementStatementView {
        self
    }

    fn with_statement(self, statement: SettlementStatementDisplayView) -> Self::Display {
        statement
    }
}

impl SettlementStatementResult for SettlementDraftCommandResult {
    type Display = SettlementDraftCommandResult<SettlementStatementDisplayView>;

    fn statement(&self) -> &SupplierSettlementStatementView {
        &self.statement
    }

    fn with_statement(self, statement: SettlementStatementDisplayView) -> Self::Display {
        SettlementDraftCommandResult {
            result_status: self.result_status,
            message: self.message,
            request_id: self.request_id,
            statement,
            item_count: self.item_count,
            difference_count: self.difference_count,
        }
    }
}

impl SettlementStatementResult for SubmitSettlementReviewResult {
    type Display = SubmitSettlementReviewResult<SettlementStatementDisplayView>;

    fn statement(&self) -> &SupplierSettlementStatementView {
        &self.statement
    }

    fn with_statement(self, statement: SettlementStatementDisplayView) -> Self::Display {
        SubmitSettlementReviewResult {
            result_status: self.result_status,
            message: self.message,
            operation_id: self.operation_id,
            statement,
            work_item_id: self.work_item_id,
        }
    }
}

impl SettlementStatementResult for SettlementReviewDecisionResult {
    type Display = SettlementReviewDecisionResult<SettlementStatementDisplayView>;

    fn statement(&self) -> &SupplierSettlementStatementView {
        &self.statement
    }

    fn with_statement(self, statement: SettlementStatementDisplayView) -> Self::Display {
        SettlementReviewDecisionResult {
            result_status: self.result_status,
            message: self.message,
            operation_id: self.operation_id,
            statement,
            work_item_id: self.work_item_id,
            work_item_status: self.work_item_status,
            task_version: self.task_version,
            payable_no: self.payable_no,
            payable_account_id: self.payable_account_id,
            cost_delta_gross: self.cost_delta_gross,
        }
    }
}
