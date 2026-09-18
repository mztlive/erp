//! Stable sales-line facts required by procurement responsibility resolution.
use erp_core::ids::SkuId;

use crate::entity::sales_order::{LineType, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine};
use crate::{Error, Result};
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
    collect_procurement_facts(
        lines.iter().map(|line| ProcurementLineRef {
            line_key: line.sales_order_line_id.to_string(),
            line_type: line.line_type,
            sku_id: line.sku_id.clone(),
            service_region: line.service_region.clone(),
            line_no: line.line_no,
        }),
        "实物及服务销售单包含非实物服务行",
    )
}

/// 从冻结提交行构造最终生效采购责任解析输入。
pub fn submission_procurement_inputs(
    lines: &[SalesOrderSubmissionLine],
) -> Result<Vec<SalesProcurementLineFact>> {
    collect_procurement_facts(
        lines.iter().map(|line| ProcurementLineRef {
            line_key: line.sales_order_line_id.to_string(),
            line_type: line.line_type,
            sku_id: line.sku_id.clone(),
            service_region: line.service_region.clone(),
            line_no: line.line_no,
        }),
        "实物及服务销售提交包含非实物服务行",
    )
}

/// 采购责任解析所需的行引用（工作副本行与冻结提交行的公共形状）。
struct ProcurementLineRef {
    /// 稳定销售明细身份。
    line_key: String,
    /// 行类型（仅实物及服务行可解析采购责任）。
    line_type: LineType,
    /// 行 SKU（缺失时拒绝）。
    sku_id: Option<SkuId>,
    /// 行服务区域。
    service_region: Option<String>,
    /// 行号（仅用于错误定位）。
    line_no: u32,
}

/// 由行引用收集采购责任解析输入（草稿与提交的公共规则）。
///
/// 实物及服务单只能由实物及服务行解析责任，且每行必须携带 SKU；两种来源的
/// 差异仅在非实物服务行的冲突文案。
///
/// # 参数
/// * `lines` - 行引用
/// * `non_goods_message` - 非实物服务行时的冲突说明
///
/// # 返回
/// 返回与行一一对应的责任解析输入。
///
/// # 错误
/// 含非实物服务行或任一行缺少 SKU 时返回错误。
fn collect_procurement_facts(
    lines: impl IntoIterator<Item = ProcurementLineRef>,
    non_goods_message: &str,
) -> Result<Vec<SalesProcurementLineFact>> {
    lines
        .into_iter()
        .map(|line| {
            if line.line_type != LineType::GoodsService {
                return Err(Error::ConflictError(non_goods_message.to_string()));
            }
            let sku_id = line
                .sku_id
                .ok_or_else(|| Error::ValidationError(format!("第 {} 行缺少 SKU", line.line_no)))?;
            Ok(SalesProcurementLineFact {
                line_key: line.line_key,
                sku_id,
                service_region: line.service_region,
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use erp_core::ids::SalesOrderLineId;

    use super::*;

    #[test]
    fn procurement_facts_reject_non_goods_and_missing_sku() {
        let line = |line_type: LineType, sku_id: Option<SkuId>| ProcurementLineRef {
            line_key: SalesOrderLineId::new("line-1").to_string(),
            line_type,
            sku_id,
            service_region: None,
            line_no: 1,
        };

        assert!(
            collect_procurement_facts([line(LineType::Voucher, Some(SkuId::new("sku-1")))], "含非实物服务行")
                .is_err()
        );
        assert!(collect_procurement_facts([line(LineType::GoodsService, None)], "含非实物服务行").is_err());
        let facts = collect_procurement_facts(
            [line(LineType::GoodsService, Some(SkuId::new("sku-1")))],
            "含非实物服务行",
        )
        .unwrap();
        assert_eq!(facts.len(), 1);
        assert_eq!(facts[0].line_key, "line-1");
    }
}
