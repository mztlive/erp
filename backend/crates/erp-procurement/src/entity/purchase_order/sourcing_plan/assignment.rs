use std::collections::HashSet;
use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::money::Quantity;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::plan::zero_quantity;

/// 销售供给来源。
#[derive(Debug, Clone, Copy, Default, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplySourceType {
    /// 供应商采购。
    #[default]
    Purchase,
    /// 公司现有库存。
    ExistingStock,
}

/// 已类型化的选源分配行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingAssignment {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 本行选用的精确创建依据；现有库存来源绑定库存余额依据。
    pub basis_id: String,
    /// 供给来源。
    pub source_type: SupplySourceType,
    /// 仓库履约采购的目标收货仓；其他供给来源必须为空。
    pub target_warehouse_id: Option<String>,
    /// 本次分配数量。
    pub quantity: Quantity,
    /// 采购确认的预计交付日。
    pub expected_delivery_date: BusinessDate,
}

impl SourcingAssignment {
    /// 由原始请求文本规范化并校验一条选源分配行。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 稳定销售行原始文本
    /// * `basis_id` - 精确创建依据原始文本
    /// * `source_type` - 供给来源
    /// * `target_warehouse_id` - 可选目标收货仓原始文本
    /// * `quantity` - 本次分配数量原始文本
    /// * `expected_delivery_date` - 预计交付日原始文本
    ///
    /// # 返回
    /// 返回去除首尾空白、可选目标仓已归一且数量与日期已类型化的选源行。
    ///
    /// # 错误
    /// 销售行或依据空白、数量或预计交付日非法、现有库存另行指定目标仓、
    /// 数量不大于零时返回领域错误。
    ///
    /// # 关键业务约束
    /// 不做重复检查与排序，集合级规则由 [`SourcingAssignmentSet::normalize`]
    /// 承担；现有库存的仓库由所选库存余额确定，不得由客户端指定。
    pub fn parse(
        sales_order_line_id: &str,
        basis_id: &str,
        source_type: SupplySourceType,
        target_warehouse_id: Option<&str>,
        quantity: &str,
        expected_delivery_date: &str,
    ) -> Result<Self> {
        let sales_order_line_id = sales_order_line_id.trim().to_string();
        let basis_id = basis_id.trim().to_string();
        if sales_order_line_id.is_empty() {
            return Err(Error::from("销售行不能为空"));
        }
        if basis_id.is_empty() {
            return Err(Error::from("履约方案不能为空"));
        }
        let quantity = Quantity::from_str(quantity.trim())
            .map_err(|error| Error::from(format!("本次分配数量非法: {error}")))?;
        let expected_delivery_date = BusinessDate::from_str(expected_delivery_date.trim())
            .map_err(|error| Error::from(format!("预计交付日非法: {error}")))?;
        let target_warehouse_id =
            target_warehouse_id.map(str::trim).filter(|value| !value.is_empty()).map(str::to_string);
        if source_type == SupplySourceType::ExistingStock && target_warehouse_id.is_some() {
            return Err(Error::from("现有库存由所选库存余额确定仓库，不能另行指定目标仓"));
        }
        if quantity <= zero_quantity() {
            return Err(Error::from("本次分配数量必须大于 0"));
        }
        Ok(Self {
            sales_order_line_id,
            basis_id,
            source_type,
            target_warehouse_id,
            quantity,
            expected_delivery_date,
        })
    }
}

/// 已规范化并稳定排序的选源分配集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SourcingAssignmentSet {
    /// 规范化后的逐行分配，按稳定销售行与依据升序排列。
    assignments: Vec<SourcingAssignment>,
}

impl SourcingAssignmentSet {
    /// 校验并稳定排序选源分配集合。
    ///
    /// # 参数
    /// * `assignments` - 已逐行类型化的选源行
    ///
    /// # 返回
    /// 返回同销售行同依据不重复且按销售行、依据升序排列的集合。
    ///
    /// # 错误
    /// 同一稳定销售行重复使用同一依据时返回领域错误。
    ///
    /// # 关键业务约束
    /// 同一稳定销售行可按不同依据拆分，但同一依据只能出现一次；排序与
    /// [`super::super::creation_basis::normalize_requested_lines`] 同序，保证命令
    /// 指纹与建单行序稳定。
    pub fn normalize(assignments: &[SourcingAssignment]) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut normalized = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            if !seen.insert((assignment.sales_order_line_id.clone(), assignment.basis_id.clone())) {
                return Err(Error::from("同一销售行不能重复使用同一履约方案"));
            }
            normalized.push(assignment.clone());
        }
        normalized.sort_by(|left, right| {
            left.sales_order_line_id
                .cmp(&right.sales_order_line_id)
                .then_with(|| left.basis_id.cmp(&right.basis_id))
        });
        Ok(Self { assignments: normalized })
    }

    /// 返回规范化后的逐行分配。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回已去重并稳定排序的选源行切片。
    ///
    /// # 错误
    /// 无。
    pub fn assignments(&self) -> &[SourcingAssignment] {
        &self.assignments
    }
}
