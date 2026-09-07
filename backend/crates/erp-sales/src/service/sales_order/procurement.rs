//! Stable sales-line facts required by procurement responsibility resolution.
use crate::entity::sales_order::{LineType, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine};
use crate::{Error, Result};
use erp_core::ids::SkuId;
/// Minimal sales-owned line input; provider rule and authorization types stay outside sales.
pub struct SalesProcurementLineFact {
    /// Stable sales line identity, not a copy/submission line id.
    pub line_key: String,
    /// SKU identity used by responsibility rules.
    pub sku_id: SkuId,
    /// Optional service region from this draft or frozen submission.
    pub service_region: Option<String>,
}
/// 从工作副本行构造稳定采购责任解析输入。
pub fn working_copy_inputs(lines: &[SalesOrderWorkingCopyLine]) -> Result<Vec<SalesProcurementLineFact>> {
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
            Ok(SalesProcurementLineFact {
                line_key: line.sales_order_line_id.to_string(),
                sku_id,
                service_region: line.service_region.clone(),
            })
        })
        .collect()
}

/// 从冻结提交行构造最终生效采购责任解析输入。
pub fn submission_procurement_inputs(
    lines: &[SalesOrderSubmissionLine],
) -> Result<Vec<SalesProcurementLineFact>> {
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
            Ok(SalesProcurementLineFact {
                line_key: line.sales_order_line_id.to_string(),
                sku_id,
                service_region: line.service_region.clone(),
            })
        })
        .collect()
}
