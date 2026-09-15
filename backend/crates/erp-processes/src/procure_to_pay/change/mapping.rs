//! 采购变更冻结提交前的来源销售和付款事实组合。
use erp_procurement::dto::purchase_order::{SavePurchaseOrderLine, SubmitPurchaseChangeRequest};
use erp_procurement::entity::purchase_order::{
    PurchaseChangeOrder, PurchaseChangeSubmission, PurchaseOrder, PurchaseOrderRevision,
};
use erp_sales::repository::SalesOrderExt;
use persistence_core::NoTransaction;

use super::super::PurchaseOrderProcess;
use crate::{Error, Result};
impl PurchaseOrderProcess {
    /// 将采购变更目标行绑定到来源销售单当前版本行。
    ///
    /// # 参数
    /// * `order` - 原采购单，用于定位来源销售单
    /// * `lines` - 变更目标完整行请求
    ///
    /// # 返回
    /// 返回稳定销售行与销售当前版本行均已刷新的目标行。
    ///
    /// # 错误
    /// 来源销售单、当前销售版本或稳定销售行缺失，以及仓储查询失败时返回错误。
    ///
    /// # 关键业务约束
    /// 不再沿历史采购提交反查销售提交行；分配数量固定等于变更后的采购数量。
    pub(super) async fn enrich_change_lines_with_current_sales_revision(
        &self,
        order: &PurchaseOrder,
        lines: &[SavePurchaseOrderLine],
    ) -> Result<Vec<SavePurchaseOrderLine>> {
        let sales_order = self
            .db
            .sales_orders()
            .find_by_id(&order.sales_order_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("来源销售单不存在".to_string()))?;
        let revision_id = sales_order
            .stable
            .current_revision_id
            .as_ref()
            .ok_or_else(|| Error::BusinessLogicError("来源销售单缺少当前版本".to_string()))?;
        let revision_lines = self
            .db
            .sales_order_revision_lines()
            .list_lines_by_revision(
                &erp_core::ids::SalesOrderRevisionId::new(revision_id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let by_stable_id = revision_lines
            .into_iter()
            .map(|line| {
                (
                    line.sales_order_line_id.to_string(),
                    erp_procurement::ports::change::CurrentSalesRevisionLineFact {
                        revision_line_id: line.base.id,
                    },
                )
            })
            .collect::<std::collections::HashMap<_, _>>();
        Ok(erp_procurement::service::purchase_order::change::mapping::enrich_change_lines(
            lines,
            &by_stable_id,
        )?)
    }
    /// 保留原行校验/金额、付款代码解析、提交序号查询及 ID 分配的顺序。
    pub(super) async fn build_change_submission(
        &self,
        change: &PurchaseChangeOrder,
        order: &PurchaseOrder,
        base_revision: &PurchaseOrderRevision,
        _supplier_name: &str,
        req: &SubmitPurchaseChangeRequest,
    ) -> Result<PurchaseChangeSubmission> {
        let header = erp_procurement::service::purchase_order::change::mapping::prepare_submission_header(
            base_revision,
            req,
        )?;
        let payment_term_snapshot = self.payment_term_snapshot(&header.payment_term_code).await?;
        Ok(self
            .domain()
            .build_change_submission(change, order, base_revision, header, payment_term_snapshot)
            .await?)
    }
}
