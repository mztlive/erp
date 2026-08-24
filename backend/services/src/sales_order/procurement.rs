//! 销售提交和最终生效阶段的采购责任计划。

use entities::sales_order::{
    BusinessType, LineType, SalesOrder, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine,
};

use super::SalesOrderService;
use crate::errors::{Error, Result};
use crate::procurement_responsibility::{ProcurementResponsibilityService, ResolutionInput};

impl SalesOrderService {
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
        let inputs = working_copy_inputs(lines)?;
        ProcurementResponsibilityService::new(self.db.clone(), self.require_rbac()?.clone())
            .resolve_strict(&inputs)
            .await?;
        Ok(())
    }
}

/// 从工作副本行构造稳定采购责任解析输入。
fn working_copy_inputs(lines: &[SalesOrderWorkingCopyLine]) -> Result<Vec<ResolutionInput>> {
    lines
        .iter()
        .map(|line| {
            if line.line_type != LineType::GoodsService {
                return Err(Error::ConflictError(
                    "实物及服务销售单包含非实物服务行".to_string(),
                ));
            }
            let sku_id = line
                .sku_id
                .clone()
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
            Ok(ResolutionInput {
                line_key: line.sales_order_line_id.to_string(),
                sku_id,
                service_region: line.service_region.clone(),
            })
        })
        .collect()
}

/// 从冻结提交行构造最终生效采购责任解析输入。
pub(super) fn submission_procurement_inputs(
    lines: &[SalesOrderSubmissionLine],
) -> Result<Vec<ResolutionInput>> {
    lines
        .iter()
        .map(|line| {
            if line.line_type != LineType::GoodsService {
                return Err(Error::ConflictError(
                    "实物及服务销售提交包含非实物服务行".to_string(),
                ));
            }
            let sku_id = line
                .sku_id
                .clone()
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
            Ok(ResolutionInput {
                line_key: line.sales_order_line_id.to_string(),
                sku_id,
                service_region: line.service_region.clone(),
            })
        })
        .collect()
}
