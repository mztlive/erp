//! `warehouse` 自有仓库稳定身份（数据模型 §6.3，稳定主表）。
//!
//! `warehouse_code` 唯一（唯一约束跨行，属 P3/索引校验）；
//! 「有库存或有效预占时不得停用」需要库存域数据，属跨聚合校验，留 P3。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::errors::Result;
use crate::ids::WarehouseId;
use crate::validation::normalize_required_text;
use crate::warehouse::status::EnableStatus;

/// 仓库代码最大长度。
const WAREHOUSE_CODE_MAX_LEN: usize = 64;

/// 仓库创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct WarehouseData {
    /// ERP 仓库稳定代码（唯一，创建后不可修改）。
    pub warehouse_code: String,
    /// 启停状态。
    pub status: EnableStatus,
}

/// 仓库更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct WarehouseUpdate {
    /// 启停状态；`None` 表示不修改。
    pub status: Option<EnableStatus>,
}

/// 仓库实体（稳定基础资料，数据模型 §6.3）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct Warehouse {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<EnableStatus>,
    /// ERP 仓库稳定代码（创建后不可修改）。
    pub warehouse_code: String,
}

impl PartialEq for Warehouse {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.warehouse_code == other.warehouse_code
    }
}

impl Eq for Warehouse {}

impl Warehouse {
    /// 创建仓库。
    ///
    /// 完成 warehouse_code 的校验与规范化（去首尾空白、非空、长度上限）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::WarehouseId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的仓库实体。
    ///
    /// # 错误
    /// 当 warehouse_code 为空或超长时返回错误。
    pub fn new(id: WarehouseId, data: WarehouseData, created_by: impl Into<String>) -> Result<Self> {
        let warehouse_code = normalize_required_text(
            data.warehouse_code,
            "仓库代码不能为空",
            WAREHOUSE_CODE_MAX_LEN,
            "仓库代码过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            warehouse_code,
        })
    }

    /// 更新仓库。
    ///
    /// `warehouse_code` 是稳定代码，不允许在通用更新中修改；
    /// 「有库存或有效预占时不得停用」由 P3 服务层校验（数据模型 §6.3）。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    pub fn update(&mut self, update: WarehouseUpdate, updated_by: impl Into<String>) -> Result<()> {
        if let Some(status) = update.status {
            self.stable.status = status;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 判断仓库是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::{assert_adjacency_closed, ensure_transition};
    use crate::ids::WarehouseId;

    fn data() -> WarehouseData {
        WarehouseData {
            warehouse_code: " WH-BJ-001 ".to_string(),
            status: EnableStatus::Active,
        }
    }

    /// happy path：代码 trim 规范化，状态与审计人落位。
    #[test]
    fn new_trims_and_normalizes_fields() {
        let warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();

        assert_eq!(warehouse.warehouse_code, "WH-BJ-001");
        assert!(warehouse.is_active());
        assert_eq!(warehouse.stable.created_by, "admin-1");
    }

    /// 失败路径：必填空与超长各一条。
    #[test]
    fn new_rejects_empty_and_overlong_code() {
        let empty = WarehouseData {
            warehouse_code: "  ".to_string(),
            ..data()
        };
        assert!(Warehouse::new(WarehouseId::new("wh-1"), empty, "admin-1").is_err());

        let overlong = WarehouseData {
            warehouse_code: "w".repeat(65),
            ..data()
        };
        assert!(Warehouse::new(WarehouseId::new("wh-1"), overlong, "admin-1").is_err());
    }

    /// update 修改状态并 touch 审计人；稳定代码不可修改。
    #[test]
    fn update_applies_status_and_preserves_code() {
        let mut warehouse = Warehouse::new(WarehouseId::new("wh-1"), data(), "admin-1").unwrap();

        warehouse
            .update(
                WarehouseUpdate {
                    status: Some(EnableStatus::Disabled),
                },
                "admin-2",
            )
            .unwrap();

        assert!(!warehouse.is_active());
        assert_eq!(warehouse.warehouse_code, "WH-BJ-001");
        assert_eq!(warehouse.stable.updated_by, "admin-2");
    }

    /// 状态机：合法迁移通过，邻接矩阵对称闭合。
    #[test]
    fn status_transitions_follow_document_state() {
        assert!(ensure_transition(EnableStatus::Active, EnableStatus::Disabled).is_ok());
        assert!(ensure_transition(EnableStatus::Disabled, EnableStatus::Active).is_ok());
        assert_adjacency_closed(&[EnableStatus::Active, EnableStatus::Disabled]);
    }
}
