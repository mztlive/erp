//! `purchase_return_line` 采购退货明细（数据模型 §6.11）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：明细只保留业务数量与仓库，不得新增审批绑定
//! 字段或审批状态机。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{PurchaseOrderRevisionLineId, PurchaseReturnLineId, PurchaseReturnOrderId, WarehouseId};
use crate::money::Quantity;

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
    /// * `id` - 实体主键（`entities::ids::PurchaseReturnLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 当退货数量非正时返回错误。
    pub fn new(id: PurchaseReturnLineId, data: PurchaseReturnLineData) -> Result<Self> {
        if data.return_quantity.to_decimal().is_sign_negative() || data.return_quantity.to_decimal().is_zero()
        {
            return Err(Error::from("退货数量必须为正数"));
        }

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
            if return_quantity.to_decimal().is_sign_negative() || return_quantity.to_decimal().is_zero() {
                return Err(Error::from("退货数量必须为正数"));
            }
            self.return_quantity = return_quantity;
        }
        if let Some(warehouse_id) = update.warehouse_id {
            self.warehouse_id = Some(warehouse_id);
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

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
    fn new_rejects_non_positive_quantity() {
        let non_positive = PurchaseReturnLineData {
            return_quantity: Quantity::from_str("0.000000").unwrap(),
            ..data()
        };
        assert!(PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-2"), non_positive).is_err());

        let negative = PurchaseReturnLineData {
            return_quantity: Quantity::from_str("-1.000000").unwrap(),
            ..data()
        };
        assert!(PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-3"), negative).is_err());
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
        assert_eq!(
            line.purchase_order_revision_line_id,
            PurchaseOrderRevisionLineId::new("po-1-r1-l1")
        );
    }

    /// 采购退货明细无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn purchase_return_line_has_no_approval_binding_or_state_machine() {
        let line = PurchaseReturnLine::new(PurchaseReturnLineId::new("prl-1"), data()).unwrap();
        let value = serde_json::to_value(&line).unwrap();
        let object = value.as_object().expect("采购退货明细序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));

        let production = include_str!("purchase_return_line.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
        assert!(!production.contains("WorkItem"));
    }
}
