use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use erp_core::common::state::ensure_transition;
use erp_core::ids::{SalesOrderId, SalesOrderLineId};
use erp_core::{Error, Result};

use super::LineStatus;

/// 稳定明细行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderLineData {
    /// 单内稳定行号（从 1 递增，变更不复用历史行号）。
    pub line_no: u32,
}

/// 稳定明细行实体（数据模型 §6.4：`(sales_order_id, line_no)` 唯一）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesOrderLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属销售单。
    pub sales_order_id: SalesOrderId,
    /// 单内稳定行号。
    pub line_no: u32,
    /// 行状态。
    pub line_status: LineStatus,
}

impl SalesOrderLine {
    /// 创建稳定明细行（初始 `Active`）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`erp_core::ids::SalesOrderLineId`）
    /// * `sales_order_id` - 所属销售单
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细行实体。
    ///
    /// # 错误
    /// 行号为零（越界）时返回错误。
    pub fn new(id: SalesOrderLineId, sales_order_id: SalesOrderId, data: SalesOrderLineData) -> Result<Self> {
        if data.line_no == 0 {
            return Err(Error::from("行号必须为正整数"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_order_id,
            line_no: data.line_no,
            line_status: LineStatus::Active,
        })
    }

    /// 将行标记为被后续版本移除（终态）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 行已移除时返回 [`Error::InvalidStateTransition`]。
    pub fn remove(&mut self) -> Result<()> {
        ensure_transition(self.line_status, LineStatus::Removed)?;
        self.line_status = LineStatus::Removed;
        Ok(())
    }
}
