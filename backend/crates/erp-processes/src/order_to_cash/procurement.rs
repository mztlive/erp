//! 销售提交和最终生效阶段的采购责任计划。

use erp_sales::entity::sales_order::{
    BusinessType, SalesOrder, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine,
};

use super::SalesOrderCommandProcess;
use crate::Result;
use crate::procure_to_pay::responsibility::{ProcurementResponsibilityProcess, ResolutionInput};

impl SalesOrderCommandProcess {
    /// 在销售提交审批前严格校验全部实物及服务行的采购责任。
    ///
    /// # 参数
    /// * `order` - 待提交销售单
    /// * `lines` - 当前工作副本行
    ///
    /// # 返回
    /// 非实物服务单直接通过；全部行解析到合格具体负责人时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一行缺少 SKU、规则冲突、无默认调度人或负责人不合格时失败关闭。
    pub(super) async fn ensure_procurement_responsibility_before_submit(
        &self,
        order: &SalesOrder,
        lines: &[SalesOrderWorkingCopyLine],
    ) -> Result<()> {
        if order.business_type != BusinessType::GoodsService {
            return Ok(());
        }
        let inputs = erp_sales::service::sales_order::procurement::working_copy_inputs(lines)?
            .into_iter()
            .map(resolution_input)
            .collect::<Vec<_>>();
        ProcurementResponsibilityProcess::new(self.db.clone(), self.require_rbac()?.clone())
            .resolve_strict(&inputs)
            .await?;
        Ok(())
    }
}

fn resolution_input(
    fact: erp_sales::service::sales_order::procurement::SalesProcurementLineFact,
) -> ResolutionInput {
    ResolutionInput { line_key: fact.line_key, sku_id: fact.sku_id, service_region: fact.service_region }
}
/// Map frozen sales-line facts onto the unchanged procurement resolver input.
pub(super) fn submission_procurement_inputs(
    lines: &[SalesOrderSubmissionLine],
) -> Result<Vec<ResolutionInput>> {
    Ok(erp_sales::service::sales_order::procurement::submission_procurement_inputs(lines)?
        .into_iter()
        .map(resolution_input)
        .collect())
}
