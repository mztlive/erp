//! `unit_of_measure` 计量单位（数据模型 §6.3，稳定字典）。
//!
//! `quantity_scale` 是该单位允许的数量小数位；固定点数量类型上限为 6 位小数
//! （数据模型 §4.2），因此取值必须落在 `0..=6`。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::catalog::status::EnableStatus;
use crate::common::stable::StableBase;
use crate::errors::{Error, Result};
use crate::ids::UnitOfMeasureId;
use crate::validation::normalize_required_text;

/// 单位代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 单位名称最大长度。
const NAME_MAX_LEN: usize = 64;
/// 单位符号最大长度。
const SYMBOL_MAX_LEN: usize = 32;
/// 允许的最大数量小数位（固定点数量上限，数据模型 §4.2）。
const MAX_QUANTITY_SCALE: u8 = 6;

/// 计量单位创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UnitOfMeasureData {
    /// 稳定单位代码（唯一，创建后不可修改）。
    pub unit_code: String,
    /// 单位名称。
    pub name: String,
    /// 单位符号。
    pub symbol: String,
    /// 允许数量小数位（`0..=6`）。
    pub quantity_scale: u8,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 计量单位更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct UnitOfMeasureUpdate {
    /// 单位名称；`None` 表示不修改。
    pub name: Option<String>,
    /// 单位符号；`None` 表示不修改。
    pub symbol: Option<String>,
    /// 允许数量小数位；`None` 表示不修改。
    pub quantity_scale: Option<u8>,
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 计量单位实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct UnitOfMeasure {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// 稳定单位代码（创建后不可修改）。
    pub unit_code: String,
    /// 单位名称。
    pub name: String,
    /// 单位符号。
    pub symbol: String,
    /// 允许数量小数位。
    pub quantity_scale: u8,
}

impl PartialEq for UnitOfMeasure {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.unit_code == other.unit_code
            && self.name == other.name
            && self.symbol == other.symbol
            && self.quantity_scale == other.quantity_scale
    }
}

impl Eq for UnitOfMeasure {}

impl UnitOfMeasure {
    /// 创建计量单位。
    ///
    /// 完成 unit_code/name/symbol 的校验与规范化（去首尾空白、非空、长度上限），
    /// 并校验 `quantity_scale` 不超出固定点数量的小数位上限。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::UnitOfMeasureId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的单位实体。
    ///
    /// # 错误
    /// 当 unit_code/name/symbol 为空、超长，或 quantity_scale 越界时返回错误。
    pub fn new(id: UnitOfMeasureId, data: UnitOfMeasureData, created_by: impl Into<String>) -> Result<Self> {
        let unit_code =
            normalize_required_text(data.unit_code, "单位代码不能为空", CODE_MAX_LEN, "单位代码过长")?;
        let name = normalize_required_text(data.name, "单位名称不能为空", NAME_MAX_LEN, "单位名称过长")?;
        let symbol =
            normalize_required_text(data.symbol, "单位符号不能为空", SYMBOL_MAX_LEN, "单位符号过长")?;
        ensure_quantity_scale(data.quantity_scale)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            unit_code,
            name,
            symbol,
            quantity_scale: data.quantity_scale,
        })
    }

    /// 更新计量单位。
    ///
    /// 复用 `new` 的校验规则；`unit_code` 是稳定代码，不允许在通用更新中修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败时返回错误。
    pub fn update(&mut self, update: UnitOfMeasureUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(name) = update.name {
            self.name = normalize_required_text(name, "单位名称不能为空", NAME_MAX_LEN, "单位名称过长")?;
        }
        if let Some(symbol) = update.symbol {
            self.symbol =
                normalize_required_text(symbol, "单位符号不能为空", SYMBOL_MAX_LEN, "单位符号过长")?;
        }
        if let Some(quantity_scale) = update.quantity_scale {
            ensure_quantity_scale(quantity_scale)?;
            self.quantity_scale = quantity_scale;
        }
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断单位是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }
}

/// 校验数量小数位不超出固定点数量上限。
///
/// # 参数
/// * `scale` - 允许数量小数位
///
/// # 返回
/// `0..=6` 内返回 `Ok(())`。
///
/// # 错误
/// 超过 6 位时返回错误。
fn ensure_quantity_scale(scale: u8) -> Result<()> {
    if scale > MAX_QUANTITY_SCALE {
        return Err(Error::from("数量小数位最多 6 位"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::UnitOfMeasureId;

    fn data() -> UnitOfMeasureData {
        UnitOfMeasureData {
            unit_code: " KG ".to_string(),
            name: " 千克 ".to_string(),
            symbol: " kg ".to_string(),
            quantity_scale: 3,
            status: EnableStatus::Active,
        }
    }

    /// happy path：字段 trim 规范化，合法小数位通过。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let unit = UnitOfMeasure::new(UnitOfMeasureId::new("uom-1"), data(), "admin-1").unwrap();

        assert_eq!(unit.unit_code, "KG");
        assert_eq!(unit.name, "千克");
        assert_eq!(unit.symbol, "kg");
        assert_eq!(unit.quantity_scale, 3);
        assert!(unit.is_active());
    }

    /// 失败路径：必填空、越界（小数位 > 6）各一条。
    #[test]
    fn new_rejects_empty_and_out_of_range_scale() {
        let empty_code = UnitOfMeasureData {
            unit_code: "  ".to_string(),
            ..data()
        };
        assert!(UnitOfMeasure::new(UnitOfMeasureId::new("uom-1"), empty_code, "admin-1").is_err());

        let over_range = UnitOfMeasureData {
            quantity_scale: 7,
            ..data()
        };
        assert!(UnitOfMeasure::new(UnitOfMeasureId::new("uom-1"), over_range, "admin-1").is_err());
    }

    /// 边界：0 与 6 位小数均合法。
    #[test]
    fn quantity_scale_boundaries_are_accepted() {
        for scale in [0, 6] {
            let unit = UnitOfMeasure::new(
                UnitOfMeasureId::new("uom-1"),
                UnitOfMeasureData {
                    quantity_scale: scale,
                    ..data()
                },
                "admin-1",
            )
            .unwrap();
            assert_eq!(unit.quantity_scale, scale);
        }
    }

    /// update 修改名称/符号/小数位并 touch 审计人；稳定代码不可修改。
    #[test]
    fn update_applies_fields_and_preserves_code() {
        let mut unit = UnitOfMeasure::new(UnitOfMeasureId::new("uom-1"), data(), "admin-1").unwrap();

        unit.update(
            UnitOfMeasureUpdate {
                name: Some(" 克 ".to_string()),
                symbol: Some(" g ".to_string()),
                quantity_scale: Some(0),
                status: Some(EnableStatus::Disabled),
            },
            "admin-2",
        )
        .unwrap();

        assert_eq!(unit.name, "克");
        assert_eq!(unit.symbol, "g");
        assert_eq!(unit.quantity_scale, 0);
        assert!(!unit.is_active());
        assert_eq!(unit.unit_code, "KG");

        let over_range_update = UnitOfMeasureUpdate {
            quantity_scale: Some(7),
            ..Default::default()
        };
        assert!(unit.update(over_range_update, "admin-2").is_err());
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
