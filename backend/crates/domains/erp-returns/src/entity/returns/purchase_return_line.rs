//! `purchase_return_line` 采购退货明细（数据模型 §6.11）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：明细只保留业务数量与仓库，不得新增审批绑定
//! 字段或审批状态机。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{PurchaseOrderRevisionLineId, PurchaseReturnLineId, PurchaseReturnOrderId, WarehouseId};
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 采购退货明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseReturnLineData {
    /// 采购退货单。
    pub purchase_return_order_id: PurchaseReturnOrderId,
    /// 原采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 退货数量。
    pub return_quantity: Quantity,
    /// 公司仓退货时必填的仓库（模式校验在 P3 结合退货单判定）。
    pub warehouse_id: Option<WarehouseId>,
}

/// 采购退货明细更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct PurchaseReturnLineUpdate {
    /// 退货数量；`None` 表示不修改。
    pub return_quantity: Option<Quantity>,
    /// 仓库；`None` 表示不修改。
    pub warehouse_id: Option<WarehouseId>,
}

/// 采购退货明细实体（行项，数据模型 §6.11）。
///
/// 公司仓退货时 `warehouse_id` 必填、客户直退供应商不写自有库存需要退货单的
/// `return_mode` 才能判定，属跨实体约束，由 P3 退货事务校验；实体层保证退货
/// 数量为正数。已退货/完成的退货单行项不应再修改，由 P3 按退货单状态约束。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseReturnLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 采购退货单。
    pub purchase_return_order_id: PurchaseReturnOrderId,
    /// 原采购明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 退货数量。
    pub return_quantity: Quantity,
    /// 仓库。
    pub warehouse_id: Option<WarehouseId>,
}

impl PurchaseReturnLine {
    /// 创建采购退货明细。
    ///
    /// 完成退货数量正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::PurchaseReturnLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 当退货数量非正时返回错误。
    pub fn new(id: PurchaseReturnLineId, data: PurchaseReturnLineData) -> Result<Self> {
        validate_return_quantity(data.return_quantity)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_return_order_id: data.purchase_return_order_id,
            purchase_order_revision_line_id: data.purchase_order_revision_line_id,
            return_quantity: data.return_quantity,
            warehouse_id: data.warehouse_id,
        })
    }

    /// 更新采购退货明细。
    ///
    /// 复用 `new` 的校验规则；原采购明细与退货单是固定字段。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当退货数量非正时返回错误。
    pub fn update(&mut self, update: PurchaseReturnLineUpdate) -> Result<()> {
        if let Some(return_quantity) = update.return_quantity {
            validate_return_quantity(return_quantity)?;
            self.return_quantity = return_quantity;
        }
        if let Some(warehouse_id) = update.warehouse_id {
            self.warehouse_id = Some(warehouse_id);
        }
        Ok(())
    }
}

fn validate_return_quantity(quantity: Quantity) -> Result<()> {
    if quantity.to_decimal().is_sign_negative() || quantity.to_decimal().is_zero() {
        return Err(Error::from("退货数量必须为正数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn data() -> PurchaseReturnLineData {
        PurchaseReturnLineData {
            purchase_return_order_id: PurchaseReturnOrderId::new("pro-1"),
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("po-1-r1-l1"),
            return_quantity: Quantity::from_str("5.000000").unwrap(),
            warehouse_id: Some(WarehouseId::new("wh-1")),
        }
    }

    #[test]
    fn new_accepts_valid_line() {
        let line = PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-1"), data()).unwrap();
        assert_eq!(line.return_quantity, Quantity::from_str("5.000000").unwrap());
        assert_eq!(line.warehouse_id, Some(WarehouseId::new("wh-1")));
    }

    #[test]
    fn new_and_update_reject_non_positive_quantity_without_changes() {
        let mut line = PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-1"), data()).unwrap();
        line.base = BaseModel::fake();
        let before = line.clone();
        for value in ["0.000000", "-0.000000", "-0.000001", "-1.000000"] {
            let quantity = Quantity::from_str(value).unwrap();
            let invalid = PurchaseReturnLineData { return_quantity: quantity, ..data() };
            assert!(matches!(
                PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-2"), invalid),
                Err(Error::LogicError(message)) if message == "退货数量必须为正数"
            ));
            assert!(matches!(
                line.update(PurchaseReturnLineUpdate {
                    return_quantity: Some(quantity),
                    warehouse_id: Some(WarehouseId::new("wh-2")),
                }),
                Err(Error::LogicError(message)) if message == "退货数量必须为正数"
            ));
            assert_eq!(line, before);
        }
    }

    #[test]
    fn smallest_positive_quantity_and_empty_updates_preserve_fields() {
        let minimum = Quantity::from_str("0.000001").unwrap();
        let input = PurchaseReturnLineData { return_quantity: minimum, warehouse_id: None, ..data() };
        let mut line = PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-1"), input).unwrap();
        line.base = BaseModel::fake();
        assert_eq!(line.return_quantity, minimum);
        assert_eq!(line.warehouse_id, None);
        let before = line.clone();
        line.update(PurchaseReturnLineUpdate::default()).unwrap();
        assert_eq!(line, before);

        line.update(PurchaseReturnLineUpdate {
            return_quantity: Some(Quantity::from_str("5.000000").unwrap()),
            warehouse_id: Some(WarehouseId::new("wh-2")),
        })
        .unwrap();
        assert_eq!(line.return_quantity, Quantity::from_str("5.000000").unwrap());
        line.update(PurchaseReturnLineUpdate { return_quantity: Some(minimum), ..Default::default() })
            .unwrap();
        assert_eq!(line.return_quantity, minimum);
        assert_eq!(line.warehouse_id, Some(WarehouseId::new("wh-2")));
        assert_eq!(line.purchase_return_order_id, before.purchase_return_order_id);
        assert_eq!(line.purchase_order_revision_line_id, before.purchase_order_revision_line_id);
        let before = line.clone();
        line.update(PurchaseReturnLineUpdate::default()).unwrap();
        assert_eq!(line, before);
    }

    #[test]
    fn update_changes_quantity_and_warehouse() {
        let mut line = PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-1"), data()).unwrap();

        line.update(PurchaseReturnLineUpdate {
            return_quantity: Some(Quantity::from_str("3.000000").unwrap()),
            warehouse_id: None,
        })
        .unwrap();
        assert_eq!(line.return_quantity, Quantity::from_str("3.000000").unwrap());
        assert_eq!(line.warehouse_id, Some(WarehouseId::new("wh-1")), "None 不修改");

        line.update(PurchaseReturnLineUpdate {
            return_quantity: None,
            warehouse_id: Some(WarehouseId::new("wh-2")),
        })
        .unwrap();
        assert_eq!(line.warehouse_id, Some(WarehouseId::new("wh-2")));
        assert_eq!(line.purchase_order_revision_line_id, PurchaseOrderRevisionLineId::new("po-1-r1-l1"));
    }
}
