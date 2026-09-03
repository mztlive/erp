//! 入库行服务编排映射（FUL-E03）。
//!
//! DTO 到领域规格的转换与系统行 ID 注入；编号与质量派生归实体批量工厂。

use entities::fulfillment::PurchaseReceiptLineSpec;
use entities::ids::PurchaseReceiptLineId;
use id_generator::next_id;

use super::PurchaseReceiptLineInput;

/// 将创建请求行映射为领域规格（含系统行 ID 注入）。
///
/// # 参数
/// * `inputs` - 服务 DTO 行输入
///
/// # 返回
/// 返回带行 ID 的领域规格（行号与质量结果由实体工厂派生）。
pub(super) fn receipt_line_specs(inputs: &[PurchaseReceiptLineInput]) -> Vec<PurchaseReceiptLineSpec> {
    inputs
        .iter()
        .map(|input| PurchaseReceiptLineSpec {
            line_id: PurchaseReceiptLineId::new(next_id()),
            purchase_order_revision_line_id: input.purchase_order_revision_line_id.clone(),
            received_quantity: input.received_quantity,
            qualified_quantity: input.qualified_quantity,
            rejected_quantity: input.rejected_quantity,
        })
        .collect()
}
