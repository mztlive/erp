//! `stock_movement`：库存流水（数据模型 §6.7）。
//!
//! 库存流水是正式事实：不可更新或删除（§6.7），字典含 `occurred_at`/
//! `recorded_at` 等正式事实字段，按 §4.3 组合 `FactBase`；纠错通过冲正流水
//! 表达。`movement_type` 与 `direction` 的方向语义按字典校验：期初/采购入库/
//! 销售退回入库/盘盈为增加，仓发出库/采购退货出库/盘亏/损坏为减少，冲正方向
//! 随原流水（跨聚合判定，P3）。期初流水 `(baseline_date, warehouse_id, sku_id,
//! legacy_import_batch_id)` 唯一、`reversal_of_movement_id` 最多被一个有效全额
//! 冲正事实引用等为数据库/跨聚合约束（P3）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::fact::FactBase;
use crate::common::source::SourceType;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SkuId, StockMovementId, WarehouseId};
use crate::money::Quantity;
use crate::validation::normalize_optional_text;
use crate::validation::normalize_required_text;

/// 来源引用最大长度。
const SOURCE_REFERENCE_MAX_LEN: usize = 256;
/// 记录人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 流水方向（数据模型 §6.7：增加或减少）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MovementDirection {
    /// 增加（入库方向）。
    Increase,
    /// 减少（出库方向）。
    Decrease,
}

impl MovementDirection {
    /// 返回方向的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Increase => "增加",
            Self::Decrease => "减少",
        }
    }

    /// 返回方向的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Increase => "INCREASE",
            Self::Decrease => "DECREASE",
        }
    }
}

/// 流水类型（数据模型 §6.7：期初、采购入库、仓发出库、销售退回入库、
/// 采购退货出库、盘盈、盘亏、损坏、冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum MovementType {
    /// 期初（基准日实盘确认，数量不得从旧商城库存推导）。
    Initial,
    /// 采购入库（合格数量）。
    PurchaseReceiptIn,
    /// 仓发出库。
    WarehouseShipOut,
    /// 销售退回入库。
    SalesReturnIn,
    /// 采购退货出库。
    PurchaseReturnOut,
    /// 盘盈。
    StockGain,
    /// 盘亏。
    StockLoss,
    /// 损坏。
    Damage,
    /// 冲正（引用原流水）。
    Reversal,
}

impl MovementType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Initial => "期初",
            Self::PurchaseReceiptIn => "采购入库",
            Self::WarehouseShipOut => "仓发出库",
            Self::SalesReturnIn => "销售退回入库",
            Self::PurchaseReturnOut => "采购退货出库",
            Self::StockGain => "盘盈",
            Self::StockLoss => "盘亏",
            Self::Damage => "损坏",
            Self::Reversal => "冲正",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Initial => "INITIAL",
            Self::PurchaseReceiptIn => "PURCHASE_RECEIPT_IN",
            Self::WarehouseShipOut => "WAREHOUSE_SHIP_OUT",
            Self::SalesReturnIn => "SALES_RETURN_IN",
            Self::PurchaseReturnOut => "PURCHASE_RETURN_OUT",
            Self::StockGain => "STOCK_GAIN",
            Self::StockLoss => "STOCK_LOSS",
            Self::Damage => "DAMAGE",
            Self::Reversal => "REVERSAL",
        }
    }

    /// 返回类型在库存余额上的方向语义。
    ///
    /// 冲正的方向由原流水决定（跨聚合，返回 `None` 由 P3 判定）。
    ///
    /// # 返回
    /// 增加类返回 `Increase`，减少类返回 `Decrease`，冲正返回 `None`。
    pub fn inherent_direction(self) -> Option<MovementDirection> {
        match self {
            Self::Initial | Self::PurchaseReceiptIn | Self::SalesReturnIn | Self::StockGain => {
                Some(MovementDirection::Increase)
            }
            Self::WarehouseShipOut | Self::PurchaseReturnOut | Self::StockLoss | Self::Damage => {
                Some(MovementDirection::Decrease)
            }
            Self::Reversal => None,
        }
    }
}

/// 库存流水创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct StockMovementData {
    /// 库存维度：仓库。
    pub warehouse_id: WarehouseId,
    /// 库存维度：SKU。
    pub sku_id: SkuId,
    /// 流水类型。
    pub movement_type: MovementType,
    /// 流水方向。
    pub direction: MovementDirection,
    /// 正数数量（方向单独表达）。
    pub quantity: Quantity,
    /// 唯一来源单据标识（跨域多态引用：入库/出库/退货/调整等单据主键）。
    pub source_document_id: String,
    /// 来源单据行标识（可空）。
    pub source_line_id: Option<String>,
    /// 冲正原流水；可空。
    pub reversal_of_movement_id: Option<StockMovementId>,
    /// 聚合内稳定序号（正式事实，§4.3）。
    pub fact_no: String,
    /// 业务实际发生时间。
    pub occurred_at: Instant,
    /// ERP 记录时间。
    pub recorded_at: Instant,
    /// ERP 记录人或系统身份。
    pub recorded_by: String,
    /// 事实来源类型。
    pub source_type: SourceType,
    /// 可追溯的来源单据或消息引用。
    pub source_reference: Option<String>,
    /// 原因代码。
    pub reason_code: Option<String>,
    /// 原因说明文本。
    pub reason_text: Option<String>,
}

/// 库存流水实体（正式事实，数据模型 §6.7）。
///
/// 组合 `FactBase`；库存流水**不可更新或删除**，本实体不提供 `update`，
/// 纠错通过追加冲正流水表达（§4.5.1、§6.7）。`(source_document_id,
/// source_line_id, movement_type)` 对同一业务动作唯一由唯一索引保证；
/// 余额更新与预占联动由 P3 在过账事务中完成（§8.2）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct StockMovement {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub fact: FactBase,
    /// 仓库。
    pub warehouse_id: WarehouseId,
    /// SKU。
    pub sku_id: SkuId,
    /// 流水类型。
    pub movement_type: MovementType,
    /// 流水方向。
    pub direction: MovementDirection,
    /// 正数数量。
    pub quantity: Quantity,
    /// 唯一来源单据标识。
    pub source_document_id: String,
    /// 来源单据行标识。
    pub source_line_id: Option<String>,
    /// 冲正原流水。
    pub reversal_of_movement_id: Option<StockMovementId>,
}

impl PartialEq for StockMovement {
    /// 全字段语义相等（`FactBase` 未派生 `PartialEq`，手工实现）。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.fact.fact_no == other.fact.fact_no
            && self.fact.occurred_at == other.fact.occurred_at
            && self.fact.recorded_at == other.fact.recorded_at
            && self.fact.recorded_by == other.fact.recorded_by
            && self.fact.source_type == other.fact.source_type
            && self.fact.source_reference == other.fact.source_reference
            && self.fact.reason_code == other.fact.reason_code
            && self.fact.reason_text == other.fact.reason_text
            && self.warehouse_id == other.warehouse_id
            && self.sku_id == other.sku_id
            && self.movement_type == other.movement_type
            && self.direction == other.direction
            && self.quantity == other.quantity
            && self.source_document_id == other.source_document_id
            && self.source_line_id == other.source_line_id
            && self.reversal_of_movement_id == other.reversal_of_movement_id
    }
}

impl Eq for StockMovement {}

impl StockMovement {
    /// 创建库存流水（正式事实，创建后不可变）。
    ///
    /// 完成数量正数校验、来源引用规范化与方向语义校验（§6.7：期初/采购入库/
    /// 销售退回入库/盘盈为增加，仓发出库/采购退货出库/盘亏/损坏为减少；
    /// 冲正方向随原流水，由 P3 判定）；校验 `recorded_at` 不早于
    /// `occurred_at`、冲正流水不引用自身。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::StockMovementId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的库存流水实体。
    ///
    /// # 错误
    /// 数量非正、方向与类型不一致、来源单据标识为空、记录时间早于发生时间
    /// 或冲正引用自身时返回错误。
    pub fn new(id: StockMovementId, data: StockMovementData) -> Result<Self> {
        if data.quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("库存流水数量必须为正数"));
        }
        if let Some(expected) = data.movement_type.inherent_direction() {
            if expected != data.direction {
                return Err(Error::from(format!(
                    "流水类型 {} 的方向必须为 {}",
                    data.movement_type.as_str(),
                    expected.as_str()
                )));
            }
        }
        let source_document_id = normalize_required_text(
            data.source_document_id,
            "来源单据标识不能为空",
            SOURCE_REFERENCE_MAX_LEN,
            "来源单据标识过长",
        )?;
        let source_line_id =
            normalize_optional_text(data.source_line_id, "来源单据行标识", SOURCE_REFERENCE_MAX_LEN)?;
        let recorded_by =
            normalize_required_text(data.recorded_by, "记录人不能为空", ACTOR_MAX_LEN, "记录人过长")?;
        if data.recorded_at < data.occurred_at {
            return Err(Error::from("记录时间不得早于业务发生时间"));
        }
        if data.reversal_of_movement_id.as_ref() == Some(&id) {
            return Err(Error::from("冲正流水不能引用自身"));
        }
        let source_reference = data
            .source_reference
            .map(|value| value.trim().chars().take(SOURCE_REFERENCE_MAX_LEN).collect())
            .filter(|value: &String| !value.is_empty());

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            fact: FactBase::new(
                data.fact_no,
                data.occurred_at,
                data.recorded_at,
                recorded_by,
                crate::common::fact::FactSource {
                    source_type: data.source_type,
                    source_reference,
                    reason_code: data.reason_code,
                    reason_text: data.reason_text,
                },
            ),
            warehouse_id: data.warehouse_id,
            sku_id: data.sku_id,
            movement_type: data.movement_type,
            direction: data.direction,
            quantity: data.quantity,
            source_document_id,
            source_line_id,
            reversal_of_movement_id: data.reversal_of_movement_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::StockMovementId;
    use std::str::FromStr;

    fn data() -> StockMovementData {
        StockMovementData {
            warehouse_id: WarehouseId::new("wh-1"),
            sku_id: SkuId::new("sku-1"),
            movement_type: MovementType::PurchaseReceiptIn,
            direction: MovementDirection::Increase,
            quantity: Quantity::from_str("10").unwrap(),
            source_document_id: " receipt-1 ".to_string(),
            source_line_id: Some(" line-1 ".to_string()),
            reversal_of_movement_id: None,
            fact_no: "F-100".to_string(),
            occurred_at: Instant::from_unix_secs(1_700_000_000),
            recorded_at: Instant::from_unix_secs(1_700_000_100),
            recorded_by: " operator-1 ".to_string(),
            source_type: SourceType::Erp,
            source_reference: Some(" msg-1 ".to_string()),
            reason_code: None,
            reason_text: None,
        }
    }

    /// happy path：字段规范化、方向语义与 FactBase 组合。
    #[test]
    fn new_normalizes_fields_and_validates_direction() {
        let movement = StockMovement::new(StockMovementId::new("m-1"), data()).unwrap();
        assert_eq!(movement.source_document_id, "receipt-1");
        assert_eq!(movement.source_line_id.as_deref(), Some("line-1"));
        assert_eq!(movement.fact.recorded_by, "operator-1");
        assert_eq!(movement.fact.source_reference.as_deref(), Some("msg-1"));
        assert_eq!(movement.fact.occurred_at.unix_secs(), 1_700_000_000);
        assert_eq!(
            movement.movement_type.inherent_direction(),
            Some(MovementDirection::Increase)
        );
    }

    /// 失败路径：方向与类型不一致、数量越界、引用自身、时间倒挂。
    #[test]
    fn new_rejects_invalid_inputs() {
        let wrong_direction = StockMovementData {
            direction: MovementDirection::Decrease,
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-2"), wrong_direction).is_err());

        let zero_quantity = StockMovementData {
            quantity: Quantity::from_str("0").unwrap(),
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-3"), zero_quantity).is_err());

        let blank_source = StockMovementData {
            source_document_id: "   ".to_string(),
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-4"), blank_source).is_err());

        let self_reversal = StockMovementData {
            reversal_of_movement_id: Some(StockMovementId::new("m-5")),
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-5"), self_reversal).is_err());

        let reversed_time = StockMovementData {
            recorded_at: Instant::from_unix_secs(1_699_999_999),
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-6"), reversed_time).is_err());
    }

    /// 方向语义：全部类型的固有方向与冲正自由方向。
    #[test]
    fn movement_type_direction_semantics() {
        assert_eq!(
            MovementType::Initial.inherent_direction(),
            Some(MovementDirection::Increase)
        );
        assert_eq!(
            MovementType::SalesReturnIn.inherent_direction(),
            Some(MovementDirection::Increase)
        );
        assert_eq!(
            MovementType::StockGain.inherent_direction(),
            Some(MovementDirection::Increase)
        );
        assert_eq!(
            MovementType::WarehouseShipOut.inherent_direction(),
            Some(MovementDirection::Decrease)
        );
        assert_eq!(
            MovementType::PurchaseReturnOut.inherent_direction(),
            Some(MovementDirection::Decrease)
        );
        assert_eq!(
            MovementType::StockLoss.inherent_direction(),
            Some(MovementDirection::Decrease)
        );
        assert_eq!(
            MovementType::Damage.inherent_direction(),
            Some(MovementDirection::Decrease)
        );
        assert_eq!(
            MovementType::Reversal.inherent_direction(),
            None,
            "冲正方向随原流水（P3）"
        );

        // 冲正流水两种方向均可构造，由 P3 对照原流水校验。
        let reversal_increase = StockMovementData {
            movement_type: MovementType::Reversal,
            direction: MovementDirection::Increase,
            source_document_id: "adjust-1".to_string(),
            source_line_id: None,
            ..data()
        };
        assert!(StockMovement::new(StockMovementId::new("m-7"), reversal_increase).is_ok());
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返（不可变事实无 update）。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&MovementType::WarehouseShipOut).unwrap(),
            "\"WAREHOUSE_SHIP_OUT\""
        );
        assert_eq!(
            serde_json::to_string(&MovementDirection::Decrease).unwrap(),
            "\"DECREASE\""
        );
        assert_eq!(MovementType::StockLoss.label(), "盘亏");

        let movement = StockMovement::new(StockMovementId::new("m-8"), data()).unwrap();
        let roundtrip: StockMovement =
            bson::deserialize_from_document(bson::serialize_to_document(&movement).unwrap()).unwrap();
        assert_eq!(roundtrip, movement);
    }
}
