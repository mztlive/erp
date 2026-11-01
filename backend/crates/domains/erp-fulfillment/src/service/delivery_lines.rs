//! 发货行服务编排映射（FUL-E01）。
//!
//! DTO/预占到领域规格的转换与系统 ID 注入；编号与归属规则归实体批量工厂。

use erp_core::ids::DeliveryLineId;
use id_generator::next_id;

use crate::Result;
use crate::dto::DeliveryLineInput;
use crate::entity::facts::ReceiptReservationLineFact;
use crate::entity::fulfillment::DeliveryLineSpec;

/// 将创建请求行映射为领域规格（含系统行 ID 注入）。
///
/// # 参数
/// * `inputs` - 服务 DTO 行输入
///
/// # 返回
/// 返回带行 ID 的领域规格（行号由实体工厂分配）。
///
/// # 错误
/// 不校验输入，始终返回 `Ok`；空切片得到空规格列表。
pub fn delivery_line_specs(inputs: &[DeliveryLineInput]) -> Result<Vec<DeliveryLineSpec>> {
    Ok(inputs
        .iter()
        .map(|input| DeliveryLineSpec {
            line_id: DeliveryLineId::new(next_id()),
            sales_order_line_id: input.sales_order_line_id.clone(),
            quantity: input.quantity,
            stock_reservation_id: input.stock_reservation_id.clone(),
            purchase_line_sales_allocation_id: input.purchase_line_sales_allocation_id.clone(),
        })
        .collect())
}

/// 将入库预占投影为仓发行领域规格（含系统行 ID 注入）。
///
/// # 参数
/// * `reservations` - 本次入库形成的销售预占
///
/// # 返回
/// 返回带行 ID 的仓发行规格（行号由实体工厂分配）。
///
/// # 错误
/// 不返回错误。
pub fn receipt_reservation_specs(reservations: &[ReceiptReservationLineFact]) -> Vec<DeliveryLineSpec> {
    reservations
        .iter()
        .map(|reservation| DeliveryLineSpec {
            line_id: DeliveryLineId::new(next_id()),
            sales_order_line_id: reservation.sales_order_line_id.clone(),
            quantity: reservation.reserved_quantity,
            stock_reservation_id: Some(reservation.reservation_id.clone()),
            purchase_line_sales_allocation_id: None,
        })
        .collect()
}
