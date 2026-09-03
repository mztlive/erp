//! 发货行服务编排映射（FUL-E01）。
//!
//! DTO/预占到领域规格的转换与系统 ID 注入；编号与归属规则归实体批量工厂。

use entities::fulfillment::DeliveryLineSpec;
use entities::ids::DeliveryLineId;
use entities::inventory::StockReservation;
use id_generator::next_id;

use crate::errors::Result;

use super::{DeliveryLineInput, FulfillmentService};

impl FulfillmentService {
    /// 发货创建指纹算法与密钥版本的 golden 断言锚点。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回本域发货行规则源的稳定标识。
    pub fn delivery_line_rule_source() -> &'static str {
        "entities::fulfillment::DeliveryLineBatch"
    }
}

/// 将创建请求行映射为领域规格（含系统行 ID 注入）。
///
/// # 参数
/// * `inputs` - 服务 DTO 行输入
///
/// # 返回
/// 返回带行 ID 的领域规格（行号由实体工厂分配）。
///
/// # 错误
/// 输入为空时返回校验错误。
pub(super) fn delivery_line_specs(inputs: &[DeliveryLineInput]) -> Result<Vec<DeliveryLineSpec>> {
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
pub(super) fn receipt_reservation_specs(reservations: &[&StockReservation]) -> Vec<DeliveryLineSpec> {
    reservations
        .iter()
        .map(|reservation| DeliveryLineSpec {
            line_id: DeliveryLineId::new(next_id()),
            sales_order_line_id: reservation.sales_order_line_id.clone(),
            quantity: reservation.reserved_quantity,
            stock_reservation_id: Some(reservation.base.id.clone().into()),
            purchase_line_sales_allocation_id: None,
        })
        .collect()
}
