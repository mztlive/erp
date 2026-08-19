//! `sales_return_line` 销售退货明细（数据模型 §6.11）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：明细只保留验收数量与质量结果，不得新增
//! 审批绑定字段、审批任务或审批状态机。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{SalesOrderLineId, SalesReturnCaseId, SalesReturnLineId};
use crate::money::Quantity;

/// 退回验收结果（数据模型 §6.11：退回验收；未验收时为空）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum QualityResult {
    /// 验收合格。
    Qualified,
    /// 验收不合格。
    Unqualified,
}

impl QualityResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Qualified => "合格",
            Self::Unqualified => "不合格",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Qualified => "qualified",
            Self::Unqualified => "unqualified",
        }
    }
}

/// 销售退货明细创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesReturnLineData {
    /// 退货/拒收处理单。
    pub sales_return_case_id: SalesReturnCaseId,
    /// 原销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 申请退回数量。
    pub requested_quantity: Quantity,
    /// 实际退回数量（验收后填写）。
    pub received_quantity: Option<Quantity>,
    /// 退回验收结果。
    pub quality_result: Option<QualityResult>,
    /// 可重新入库数量。
    pub restockable_quantity: Option<Quantity>,
}

/// 销售退货明细更新数据（验收信息随处理进度补充）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SalesReturnLineUpdate {
    /// 实际退回数量；`None` 表示不修改。
    pub received_quantity: Option<Quantity>,
    /// 退回验收结果；`None` 表示不修改。
    pub quality_result: Option<QualityResult>,
    /// 可重新入库数量；`None` 表示不修改。
    pub restockable_quantity: Option<Quantity>,
}

/// 销售退货明细实体（行项，数据模型 §6.11）。
///
/// 累计有效退回数量不得超过已履约数量是跨明细约束，由 P3 验收/退货事务校验；
/// 实体层保证申请数量为正、实际退回不超过申请数量、可重新入库数量不超过实际
/// 退回数量，且验收不合格时不得重新入库（退仓后仅仓储确认可重新入库数量形成
/// 库存增加，§6.11）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SalesReturnLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 退货/拒收处理单。
    pub sales_return_case_id: SalesReturnCaseId,
    /// 原销售明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 申请退回数量。
    pub requested_quantity: Quantity,
    /// 实际退回数量。
    pub received_quantity: Option<Quantity>,
    /// 退回验收结果。
    pub quality_result: Option<QualityResult>,
    /// 可重新入库数量。
    pub restockable_quantity: Option<Quantity>,
}

impl SalesReturnLine {
    /// 创建销售退货明细。
    ///
    /// 完成申请数量正数校验与「验收信息」一致性校验：实际退回数量不得超过申请
    /// 数量；可重新入库数量不得超过实际退回数量且必须验收合格。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SalesReturnLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的明细实体。
    ///
    /// # 错误
    /// 当申请数量非正或验收信息不一致时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(id: SalesReturnLineId, data: SalesReturnLineData) -> Result<Self> {
        if data.requested_quantity.to_decimal().is_sign_negative()
            || data.requested_quantity.to_decimal().is_zero()
        {
            return Err(Error::from("申请退回数量必须为正数"));
        }
        validate_receiving(
            data.received_quantity,
            data.quality_result,
            data.restockable_quantity,
            Some(data.requested_quantity),
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            sales_return_case_id: data.sales_return_case_id,
            sales_order_line_id: data.sales_order_line_id,
            requested_quantity: data.requested_quantity,
            received_quantity: data.received_quantity,
            quality_result: data.quality_result,
            restockable_quantity: data.restockable_quantity,
        })
    }

    /// 更新退货明细的验收信息。
    ///
    /// 复用 `new` 的验收一致性校验；申请数量与原销售明细是固定字段。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当验收信息不一致时返回错误。
    pub fn update(&mut self, update: SalesReturnLineUpdate) -> Result<()> {
        let received = update.received_quantity.or(self.received_quantity);
        let quality = update.quality_result.or(self.quality_result);
        let restockable = update.restockable_quantity.or(self.restockable_quantity);
        validate_receiving(received, quality, restockable, Some(self.requested_quantity))?;
        if let Some(received) = update.received_quantity {
            self.received_quantity = Some(received);
        }
        if let Some(quality) = update.quality_result {
            self.quality_result = Some(quality);
        }
        if let Some(restockable) = update.restockable_quantity {
            self.restockable_quantity = Some(restockable);
        }
        Ok(())
    }

    /// 判断验收是否合格。
    ///
    /// # 返回
    /// 验收结果为合格时返回 `true`。
    pub fn is_qualified(&self) -> bool {
        self.quality_result == Some(QualityResult::Qualified)
    }
}

/// 校验验收信息一致性。
///
/// 规则（数据模型 §6.11）：实际退回数量不得超过申请数量；可重新入库数量不得
/// 超过实际退回数量，且必须验收合格；验收不合格不得重新入库。
///
/// # 参数
/// * `received` - 实际退回数量
/// * `quality` - 验收结果
/// * `restockable` - 可重新入库数量
/// * `requested` - 申请退回数量
///
/// # 返回
/// 一致返回 `Ok(())`。
///
/// # 错误
/// 验收信息不完整或数量关系矛盾时返回错误。
fn validate_receiving(
    received: Option<Quantity>,
    quality: Option<QualityResult>,
    restockable: Option<Quantity>,
    requested: Option<Quantity>,
) -> Result<()> {
    if let Some(received) = received {
        if received.to_decimal().is_sign_negative() {
            return Err(Error::from("实际退回数量不得为负"));
        }
        if let Some(requested) = requested {
            if received > requested {
                return Err(Error::from("实际退回数量不得超过申请数量"));
            }
        }
    }
    if let (Some(restockable), Some(received)) = (restockable, received) {
        if restockable > received {
            return Err(Error::from("可重新入库数量不得超过实际退回数量"));
        }
        if quality != Some(QualityResult::Qualified) {
            return Err(Error::from("仅验收合格的商品可以重新入库"));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::str::FromStr;

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn data() -> SalesReturnLineData {
        SalesReturnLineData {
            sales_return_case_id: SalesReturnCaseId::new("src-1"),
            sales_order_line_id: SalesOrderLineId::new("so-1-l1"),
            requested_quantity: qty("10.000000"),
            received_quantity: None,
            quality_result: None,
            restockable_quantity: None,
        }
    }

    #[test]
    fn new_accepts_pending_acceptance_line() {
        let line = SalesReturnLine::new(SalesReturnLineId::new("srl-1"), data()).unwrap();
        assert_eq!(line.requested_quantity, qty("10.000000"));
        assert!(line.received_quantity.is_none());
        assert!(!line.is_qualified());
    }

    #[test]
    fn new_rejects_non_positive_request_and_inconsistent_acceptance() {
        let non_positive = SalesReturnLineData {
            requested_quantity: qty("0.000000"),
            ..data()
        };
        assert!(SalesReturnLine::new(SalesReturnLineId::new("srl-2"), non_positive).is_err());

        let over_received = SalesReturnLineData {
            received_quantity: Some(qty("11.000000")),
            ..data()
        };
        assert!(SalesReturnLine::new(SalesReturnLineId::new("srl-3"), over_received).is_err());

        let restockable_without_quality = SalesReturnLineData {
            received_quantity: Some(qty("8.000000")),
            quality_result: None,
            restockable_quantity: Some(qty("8.000000")),
            ..data()
        };
        assert!(SalesReturnLine::new(SalesReturnLineId::new("srl-4"), restockable_without_quality).is_err());

        let restockable_over_received = SalesReturnLineData {
            received_quantity: Some(qty("5.000000")),
            quality_result: Some(QualityResult::Qualified),
            restockable_quantity: Some(qty("6.000000")),
            ..data()
        };
        assert!(SalesReturnLine::new(SalesReturnLineId::new("srl-5"), restockable_over_received).is_err());

        let rejected_restock = SalesReturnLineData {
            received_quantity: Some(qty("5.000000")),
            quality_result: Some(QualityResult::Unqualified),
            restockable_quantity: Some(qty("5.000000")),
            ..data()
        };
        assert!(SalesReturnLine::new(SalesReturnLineId::new("srl-6"), rejected_restock).is_err());
    }

    #[test]
    fn update_applies_acceptance_progress() {
        let mut line = SalesReturnLine::new(SalesReturnLineId::new("srl-1"), data()).unwrap();

        line.update(SalesReturnLineUpdate {
            received_quantity: Some(qty("8.000000")),
            quality_result: Some(QualityResult::Qualified),
            restockable_quantity: Some(qty("8.000000")),
        })
        .unwrap();
        assert_eq!(line.received_quantity, Some(qty("8.000000")));
        assert!(line.is_qualified());
        assert_eq!(line.requested_quantity, qty("10.000000"), "关键字段不改");

        assert!(
            line.update(SalesReturnLineUpdate {
                received_quantity: Some(qty("12.000000")),
                ..Default::default()
            })
            .is_err()
        );
    }

    /// 销售退货明细无审批约束：不得出现绑定字段或任务字段。
    #[test]
    fn sales_return_line_has_no_approval_binding_or_work_item() {
        let line = SalesReturnLine::new(SalesReturnLineId::new("srl-1"), data()).unwrap();
        let value = serde_json::to_value(&line).unwrap();
        let object = value.as_object().expect("明细序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("work_item_id"));
        assert!(!object.contains_key("pending_allocations"));

        let production = include_str!("sales_return_line.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("WorkItem"));
    }
}
