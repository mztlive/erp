//! 库存调整明细：创建数据、更新值对象与明细实体（数据模型 §6.7 明细）。
//!
//! 数量必须为正数；方向单独表达。明细调整与原因类型的方向一致性
//! （盘盈必增、盘亏/损坏必减）由 [`StockAdjustment::apply_line_updates`](super::StockAdjustment::apply_line_updates)
//! 与 [`StockAdjustmentLine::new_for_reason`] 校验；状态可编辑性由调整单实体把关。

use std::str::FromStr;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId};
use erp_core::money::Quantity;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::super::stock_movement::MovementDirection;
use super::AdjustmentReasonType;

/// 调整明细行主键最大长度。
const LINE_ID_MAX_LEN: usize = 128;

/// 库存调整明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentLineData {
    /// 调整单。
    pub stock_adjustment_id: StockAdjustmentId,
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量（正数）。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

/// 已解析并完成基础校验的调整明细更新值对象。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockAdjustmentLineUpdate {
    /// 明细行主键。
    pub line_id: String,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向；空表示保持原方向。
    pub direction: Option<MovementDirection>,
}

impl StockAdjustmentLineUpdate {
    /// 从服务输入构造调整明细更新值对象。
    ///
    /// # 参数
    /// * `line_id` - 明细行主键
    /// * `quantity` - 定点数量字符串
    /// * `direction` - 可选调整方向
    ///
    /// # 返回
    /// 返回完成主键规范化与数量解析的更新值对象。
    ///
    /// # 错误
    /// 行主键为空/过长，或数量不是正数时返回错误。
    pub fn new(
        line_id: impl Into<String>,
        quantity: &str,
        direction: Option<MovementDirection>,
    ) -> Result<Self> {
        let line_id =
            normalize_required_text(line_id.into(), "明细行主键不能为空", LINE_ID_MAX_LEN, "明细行主键过长")?;
        let quantity = Quantity::from_str(quantity)?;
        ensure_positive_quantity(quantity)?;
        Ok(Self { line_id, quantity, direction })
    }
}

/// 库存调整明细实体（数据模型 §6.7 明细）。
///
/// 数量必须为正数；方向单独表达。明细调整与原因类型的方向一致性
/// （盘盈必增、盘亏/损坏必减）由 [`StockAdjustment::apply_line_updates`] 与
/// [`StockAdjustmentLine::new_for_reason`] 校验；状态可编辑性由调整单实体把关。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct StockAdjustmentLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 调整单。
    pub stock_adjustment_id: StockAdjustmentId,
    /// 调整 SKU。
    pub sku_id: SkuId,
    /// 调整数量。
    pub quantity: Quantity,
    /// 调整方向。
    pub direction: MovementDirection,
}

impl StockAdjustmentLine {
    /// 创建库存调整明细。
    ///
    /// 完成调整数量正数校验。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::StockAdjustmentLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的调整明细实体。
    ///
    /// # 错误
    /// 调整数量非正时返回错误。
    pub fn new(id: StockAdjustmentLineId, data: StockAdjustmentLineData) -> Result<Self> {
        ensure_positive_quantity(data.quantity)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stock_adjustment_id: data.stock_adjustment_id,
            sku_id: data.sku_id,
            quantity: data.quantity,
            direction: data.direction,
        })
    }

    /// 按调整原因创建库存调整明细。
    ///
    /// # 参数
    /// * `id` - 实体主键
    /// * `reason_type` - 调整原因
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回数量与方向均符合原因约束的明细实体。
    ///
    /// # 错误
    /// 数量非正，或方向与调整原因不一致时返回错误。
    pub fn new_for_reason(
        id: StockAdjustmentLineId,
        reason_type: AdjustmentReasonType,
        data: StockAdjustmentLineData,
    ) -> Result<Self> {
        reason_type.ensure_direction(data.direction)?;
        Self::new(id, data)
    }

    /// 应用调整明细数量与可选方向。
    ///
    /// # 参数
    /// * `reason_type` - 调整单当前原因
    /// * `quantity` - 新的正数数量
    /// * `direction` - 新方向；空表示保持现状
    ///
    /// # 返回
    /// 校验并更新成功时返回 `Ok(())`。
    ///
    /// # 错误
    /// 数量非正，或最终方向与调整原因不一致时返回错误。
    pub fn apply_update(
        &mut self,
        reason_type: AdjustmentReasonType,
        quantity: Quantity,
        direction: Option<MovementDirection>,
    ) -> Result<()> {
        ensure_positive_quantity(quantity)?;
        let direction = direction.unwrap_or(self.direction);
        reason_type.ensure_direction(direction)?;
        self.quantity = quantity;
        self.direction = direction;
        Ok(())
    }
}

/// 校验调整数量为正数。
///
/// # 参数
/// * `quantity` - 待校验数量
///
/// # 返回
/// 数量为正时返回 `Ok(())`。
///
/// # 错误
/// 数量为零或负数时返回错误。
fn ensure_positive_quantity(quantity: Quantity) -> Result<()> {
    if quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
        return Err(Error::from("调整数量必须为正数"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use erp_core::ids::{SkuId, StockAdjustmentId, StockAdjustmentLineId, WarehouseId};

    use super::super::{StockAdjustment, StockAdjustmentData};
    use super::*;

    fn data() -> StockAdjustmentData {
        StockAdjustmentData {
            adjustment_no: " ADJ-2026-001 ".to_string(),
            warehouse_id: WarehouseId::new("wh-1"),
            reason_type: AdjustmentReasonType::StockLoss,
            prepared_by: " operator-1 ".to_string(),
            note: None,
            occurred_at: None,
        }
    }

    fn line_data() -> StockAdjustmentLineData {
        StockAdjustmentLineData {
            stock_adjustment_id: StockAdjustmentId::new("adj-1"),
            sku_id: SkuId::new("sku-1"),
            quantity: Quantity::from_str("2").unwrap(),
            direction: MovementDirection::Decrease,
        }
    }

    /// happy path：调整明细创建成功。
    #[test]
    fn line_new_succeeds() {
        let line = StockAdjustmentLine::new(StockAdjustmentLineId::new("al-1"), line_data()).unwrap();
        assert_eq!(line.quantity, Quantity::from_str("2").unwrap());
        assert_eq!(line.direction, MovementDirection::Decrease);
    }

    /// 失败路径：数量越界（非正）。
    #[test]
    fn line_rejects_quantity_violations() {
        let zero_quantity =
            StockAdjustmentLineData { quantity: Quantity::from_str("0").unwrap(), ..line_data() };
        assert!(StockAdjustmentLine::new(StockAdjustmentLineId::new("al-2"), zero_quantity).is_err());

        let negative = StockAdjustmentLineData { quantity: Quantity::from_str("-1").unwrap(), ..line_data() };
        assert!(StockAdjustmentLine::new(StockAdjustmentLineId::new("al-3"), negative).is_err());
    }

    /// 明细更新：整组校验失败不产生部分修改，完整合法输入一次应用。
    #[test]
    fn line_updates_are_validated_atomically() {
        let mut adjustment =
            StockAdjustment::new(StockAdjustmentId::new("adj-1"), data(), "creator-1").unwrap();
        adjustment.base.version = 3;
        assert!(adjustment.matches_version(3));
        assert!(!adjustment.matches_version(2));

        let mut lines = vec![
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new("al-1"),
                adjustment.reason_type,
                line_data(),
            )
            .unwrap(),
            StockAdjustmentLine::new_for_reason(
                StockAdjustmentLineId::new("al-2"),
                adjustment.reason_type,
                StockAdjustmentLineData { sku_id: SkuId::new("sku-2"), ..line_data() },
            )
            .unwrap(),
        ];
        let original = lines.clone();
        let mut gain_adjustment = adjustment.clone();
        gain_adjustment.reason_type = AdjustmentReasonType::StockGain;
        assert!(gain_adjustment.apply_line_updates(&mut lines, &[], false).is_err());
        assert_eq!(lines, original, "原因变更必须重验全部既有方向");

        let gain_updates = vec![
            StockAdjustmentLineUpdate::new("al-1", "3", Some(MovementDirection::Increase)).unwrap(),
            StockAdjustmentLineUpdate::new("al-2", "4", Some(MovementDirection::Increase)).unwrap(),
        ];
        assert!(gain_adjustment.apply_line_updates(&mut lines, &gain_updates[..1], false).is_err());
        assert_eq!(lines, original, "未更新行的方向仍须与新原因一致");
        gain_adjustment.apply_line_updates(&mut lines, &gain_updates, true).unwrap();
        assert!(lines.iter().all(|line| line.direction == MovementDirection::Increase));
        assert_eq!(lines[0].quantity.to_string(), "3");
        assert_eq!(lines[1].quantity.to_string(), "4");
        lines.clone_from(&original);

        let incomplete = vec![StockAdjustmentLineUpdate::new("al-1", "3", None).unwrap()];
        assert!(adjustment.apply_line_updates(&mut lines, &incomplete, true).is_err());
        assert_eq!(lines, original, "完整性失败不得留下部分更新");

        let wrong_direction =
            vec![StockAdjustmentLineUpdate::new("al-1", "3", Some(MovementDirection::Increase)).unwrap()];
        assert!(adjustment.apply_line_updates(&mut lines, &wrong_direction, false).is_err());
        assert_eq!(lines, original, "方向失败不得留下部分更新");

        let updates = vec![
            StockAdjustmentLineUpdate::new("al-1", "3", None).unwrap(),
            StockAdjustmentLineUpdate::new("al-2", "4", Some(MovementDirection::Decrease)).unwrap(),
        ];
        let changed = adjustment.apply_line_updates(&mut lines, &updates, true).unwrap();
        assert_eq!(changed.len(), 2);
        assert_eq!(lines[0].quantity, Quantity::from_str("3").unwrap());
        assert_eq!(lines[1].quantity, Quantity::from_str("4").unwrap());
    }
}
