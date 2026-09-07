//! 验收行服务编排映射（FUL-E02）。
//!
//! DTO 到领域规格的转换与系统行 ID 注入；行号与数量规则归实体批量工厂。

use crate::entity::fulfillment::CustomerAcceptanceLineSpec;
use erp_core::ids::CustomerAcceptanceLineId;
use id_generator::next_id;

use crate::dto::AcceptanceLineInput;

/// 将创建请求行映射为领域规格（含系统行 ID 注入）。
///
/// # 参数
/// * `inputs` - 服务 DTO 行输入
///
/// # 返回
/// 返回带行 ID 的领域规格（行号与凭证默认由实体工厂分配）。
pub fn acceptance_line_specs(inputs: &[AcceptanceLineInput]) -> Vec<CustomerAcceptanceLineSpec> {
    inputs
        .iter()
        .map(|input| CustomerAcceptanceLineSpec {
            line_id: CustomerAcceptanceLineId::new(next_id()),
            sales_order_line_id: input.sales_order_line_id.clone(),
            accepted_quantity: input.accepted_quantity,
            short_quantity: input.short_quantity,
            rejected_quantity: input.rejected_quantity,
            reason: input.reason.clone(),
        })
        .collect()
}
