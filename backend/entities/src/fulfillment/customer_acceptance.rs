//! `customer_acceptance` / `customer_acceptance_line`：客户验收单及行（数据模型 §6.7）。
//!
//! 合同 §4.3 签署为 `NO_APPROVAL`：实体只保留业务状态，不得新增审批绑定字段
//! 或审批状态机。
//!
//! 状态机按 §7.5：草稿 → 已过账 → 已冲正（`POSTED` 后不可编辑，误录时新增
//! 反向验收及反向分配，不覆盖原行）。非卡券明细只有累计净有效验收通过数量
//! 达到当前有效履约数量时才算履约完成；短少、拒收和服务不通过只记录结果，
//! 不直接改库存、应收或采购（需要后续处理时创建 `sales_return_case` 或补履约
//! 记录）——均为跨聚合规则，由 P3 完成。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{DocumentState, ensure_transition};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    CustomerAcceptanceId, CustomerAcceptanceLineId, FileAssetId, SalesOrderId, SalesOrderLineId,
};
use crate::money::Quantity;
use crate::validation::normalize_optional_text;
use crate::validation::normalize_required_text;

/// 验收单号最大长度。
const ACCEPTANCE_NO_MAX_LEN: usize = 64;
/// 依据说明最大长度。
const REASON_MAX_LEN: usize = 512;

/// 客户验收单状态（数据模型 §6.7：草稿、已过账、已冲正）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CustomerAcceptanceState {
    /// 草稿。
    Draft,
    /// 已过账（不可编辑，误录走反向验收）。
    Posted,
    /// 已冲正（不可逆终态）。
    Reversed,
}

impl CustomerAcceptanceState {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Draft => "草稿",
            Self::Posted => "已过账",
            Self::Reversed => "已冲正",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Draft => "DRAFT",
            Self::Posted => "POSTED",
            Self::Reversed => "REVERSED",
        }
    }

    /// 判断是否可编辑（仅草稿）。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        matches!(self, Self::Draft)
    }
}

impl DocumentState for CustomerAcceptanceState {
    /// 固定邻接矩阵（§7.5 定向链，`REVERSED` 为不可逆终态）。
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Draft => &[Self::Posted],
            Self::Posted => &[Self::Reversed],
            Self::Reversed => &[],
        }
    }
}

/// 验收结果（数据模型 §6.7：通过、短少、拒收、服务不通过）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AcceptanceResult {
    /// 通过。
    Passed,
    /// 短少。
    Shortage,
    /// 拒收。
    Rejected,
    /// 服务不通过。
    ServiceFailed,
}

impl AcceptanceResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Passed => "通过",
            Self::Shortage => "短少",
            Self::Rejected => "拒收",
            Self::ServiceFailed => "服务不通过",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Passed => "PASSED",
            Self::Shortage => "SHORTAGE",
            Self::Rejected => "REJECTED",
            Self::ServiceFailed => "SERVICE_FAILED",
        }
    }
}

/// 客户验收单创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceData {
    /// 客户验收单号（全局唯一）。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间。
    pub accepted_at: Instant,
    /// 验收结果。
    pub result: AcceptanceResult,
}

/// 客户验收单更新数据（仅草稿可更新）。
#[derive(Debug, Clone, Default, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceUpdate {
    /// 验收结果；`None` 表示不修改。
    pub result: Option<AcceptanceResult>,
}

/// 客户验收单实体（数据模型 §6.7 表头）。
///
/// `reversal_of_acceptance_id` 记录误录验收的反向事实（可空），由冲正动作写入；
/// 已过账/已冲正不设业务软删除（§4.5.1）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CustomerAcceptance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户验收单号。
    pub acceptance_no: String,
    /// 销售单。
    pub sales_order_id: SalesOrderId,
    /// 验收时间。
    pub accepted_at: Instant,
    /// 验收结果。
    pub result: AcceptanceResult,
    /// 当前状态。
    pub status: CustomerAcceptanceState,
    /// 误录验收的反向事实。
    pub reversal_of_acceptance_id: Option<CustomerAcceptanceId>,
}

impl CustomerAcceptance {
    /// 创建客户验收单（初始状态为草稿）。
    ///
    /// 完成验收单号规范化。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerAcceptanceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的验收单实体。
    ///
    /// # 错误
    /// 验收单号为空或超长时返回错误。
    pub fn new(id: CustomerAcceptanceId, data: CustomerAcceptanceData) -> Result<Self> {
        let acceptance_no = normalize_required_text(
            data.acceptance_no,
            "客户验收单号不能为空",
            ACCEPTANCE_NO_MAX_LEN,
            "客户验收单号过长",
        )?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            acceptance_no,
            sales_order_id: data.sales_order_id,
            accepted_at: data.accepted_at,
            result: data.result,
            status: CustomerAcceptanceState::Draft,
            reversal_of_acceptance_id: None,
        })
    }

    /// 更新客户验收单（仅草稿）。
    ///
    /// 已过账验收不可编辑（§6.7），误录时新增反向验收及反向分配。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不可编辑时返回错误。
    pub fn update(&mut self, update: CustomerAcceptanceUpdate) -> Result<()> {
        self.ensure_editable()?;
        if let Some(result) = update.result {
            self.result = result;
        }
        Ok(())
    }

    /// 过账验收（草稿 → 已过账）。
    ///
    /// 过账时写 `acceptance_fulfillment_allocation` 的 `APPLY`/`REVERSE` 并重算
    /// 两侧净数量上限——由 P3 在过账事务中完成（§8.2 第 5 条）。
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移（非草稿）时返回错误。
    pub fn mark_posted(&mut self) -> Result<()> {
        ensure_transition(self.status, CustomerAcceptanceState::Posted)?;
        self.status = CustomerAcceptanceState::Posted;
        Ok(())
    }

    /// 冲正验收（已过账 → 已冲正，终态）。
    ///
    /// 误录时新增反向验收及反向分配，不覆盖原行（§6.7）；本方法登记反向
    /// 验收单引用并迁移状态，反向分配由 P3 形成。
    ///
    /// # 参数
    /// * `reversal_of_acceptance_id` - 反向验收单主键
    ///
    /// # 返回
    /// 迁移成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前状态不允许迁移，或反向验收引用与自身相同/为空时返回错误。
    pub fn reverse(&mut self, reversal_of_acceptance_id: CustomerAcceptanceId) -> Result<()> {
        ensure_transition(self.status, CustomerAcceptanceState::Reversed)?;
        if reversal_of_acceptance_id == CustomerAcceptanceId::new(self.base.id.clone()) {
            return Err(Error::from("反向验收不能引用自身"));
        }
        self.reversal_of_acceptance_id = Some(reversal_of_acceptance_id);
        self.status = CustomerAcceptanceState::Reversed;
        Ok(())
    }

    /// 判断当前状态是否可编辑。
    ///
    /// # 返回
    /// 草稿状态返回 `true`。
    pub fn is_editable(&self) -> bool {
        self.status.is_editable()
    }

    /// 校验当前状态可编辑。
    ///
    /// # 返回
    /// 可编辑返回 `Ok(())`。
    ///
    /// # 错误
    /// 已过账/已冲正的验收单不可编辑时返回错误。
    fn ensure_editable(&self) -> Result<()> {
        if !self.is_editable() {
            return Err(Error::from("已过账或已冲正的客户验收单不可编辑"));
        }
        Ok(())
    }
}

/// 客户验收行创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerAcceptanceLineData {
    /// 验收单。
    pub customer_acceptance_id: CustomerAcceptanceId,
    /// 稳定行号（单内从 1 递增）。
    pub line_no: u32,
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
}

/// 客户验收行实体（数据模型 §6.7 行）。
///
/// 三个结果数量均不得为负；短少、拒收和服务不通过只记录结果，不直接改库存、
/// 应收或采购（§6.7，跨聚合动作由 P3 处理）。验收行的通过、短少、拒收数量
/// 必须由其有效分配覆盖且合计守恒——跨聚合校验由 P3 完成。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct CustomerAcceptanceLine {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 验收单。
    pub customer_acceptance_id: CustomerAcceptanceId,
    /// 稳定行号。
    pub line_no: u32,
    /// 验收明细。
    pub sales_order_line_id: SalesOrderLineId,
    /// 通过数量。
    pub accepted_quantity: Quantity,
    /// 短少数量。
    pub short_quantity: Quantity,
    /// 拒收数量。
    pub rejected_quantity: Quantity,
    /// 依据说明。
    pub reason: Option<String>,
    /// 业务凭证。
    pub evidence_attachment_id: Option<FileAssetId>,
}

impl CustomerAcceptanceLine {
    /// 创建客户验收行。
    ///
    /// 完成行号、依据说明规范化与数量非负校验。行级约束
    /// `(customer_acceptance_id, line_no)` 唯一由唯一索引保证；验收单已过账后
    /// 行不可再变更由 P3 按表头状态把关（§6.7）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::CustomerAcceptanceLineId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的验收行实体。
    ///
    /// # 错误
    /// 行号小于 1、结果数量为负或依据说明超长时返回错误。
    pub fn new(id: CustomerAcceptanceLineId, data: CustomerAcceptanceLineData) -> Result<Self> {
        let reason = normalize_optional_text(data.reason, "依据说明", REASON_MAX_LEN)?;
        if data.line_no < 1 {
            return Err(Error::from("行号必须从 1 开始"));
        }
        if data.accepted_quantity.to_decimal() < rust_decimal::Decimal::ZERO
            || data.short_quantity.to_decimal() < rust_decimal::Decimal::ZERO
            || data.rejected_quantity.to_decimal() < rust_decimal::Decimal::ZERO
        {
            return Err(Error::from("验收行结果数量不得为负"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            customer_acceptance_id: data.customer_acceptance_id,
            line_no: data.line_no,
            sales_order_line_id: data.sales_order_line_id,
            accepted_quantity: data.accepted_quantity,
            short_quantity: data.short_quantity,
            rejected_quantity: data.rejected_quantity,
            reason,
            evidence_attachment_id: data.evidence_attachment_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::CustomerAcceptanceId;
    use std::str::FromStr;

    fn data() -> CustomerAcceptanceData {
        CustomerAcceptanceData {
            acceptance_no: " CA-2026-001 ".to_string(),
            sales_order_id: SalesOrderId::new("so-1"),
            accepted_at: Instant::from_unix_secs(1_700_000_000),
            result: AcceptanceResult::Passed,
        }
    }

    fn line_data() -> CustomerAcceptanceLineData {
        CustomerAcceptanceLineData {
            customer_acceptance_id: CustomerAcceptanceId::new("acceptance-1"),
            line_no: 1,
            sales_order_line_id: SalesOrderLineId::new("so-line-1"),
            accepted_quantity: Quantity::from_str("9").unwrap(),
            short_quantity: Quantity::from_str("1").unwrap(),
            rejected_quantity: Quantity::from_str("0").unwrap(),
            reason: Some(" 部分短少 ".to_string()),
            evidence_attachment_id: None,
        }
    }

    /// happy path：单号规范化、初始草稿、过账与冲正（登记反向验收）全链路。
    #[test]
    fn new_normalizes_no_and_drives_state_machine() {
        let mut acceptance =
            CustomerAcceptance::new(CustomerAcceptanceId::new("acceptance-1"), data()).unwrap();
        assert_eq!(acceptance.acceptance_no, "CA-2026-001");
        assert_eq!(acceptance.status, CustomerAcceptanceState::Draft);
        assert!(acceptance.reversal_of_acceptance_id.is_none());

        acceptance.mark_posted().unwrap();
        assert_eq!(acceptance.status, CustomerAcceptanceState::Posted);
        acceptance
            .reverse(CustomerAcceptanceId::new("acceptance-2"))
            .unwrap();
        assert_eq!(acceptance.status, CustomerAcceptanceState::Reversed);
        assert_eq!(
            acceptance.reversal_of_acceptance_id.unwrap().as_ref(),
            "acceptance-2"
        );
    }

    /// 失败路径：必填空（单号空白）、超长、反向验收引用自身。
    #[test]
    fn new_rejects_invalid_inputs() {
        let blank_no = CustomerAcceptanceData {
            acceptance_no: "  ".to_string(),
            ..data()
        };
        assert!(CustomerAcceptance::new(CustomerAcceptanceId::new("a2"), blank_no).is_err());

        let overlong_no = CustomerAcceptanceData {
            acceptance_no: "x".repeat(65),
            ..data()
        };
        assert!(CustomerAcceptance::new(CustomerAcceptanceId::new("a3"), overlong_no).is_err());

        let mut acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("a4"), data()).unwrap();
        assert!(
            acceptance.reverse(CustomerAcceptanceId::new("a4")).is_err(),
            "不能引用自身"
        );
        assert!(
            acceptance.reverse(CustomerAcceptanceId::new("other")).is_err(),
            "草稿不能冲正"
        );
    }

    /// 状态机：合法/非法/终态定向断言（含幂等迁移）。
    #[test]
    fn state_machine_directed_edges() {
        let mut acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("a5"), data()).unwrap();
        assert!(
            acceptance
                .update(CustomerAcceptanceUpdate {
                    result: Some(AcceptanceResult::Shortage),
                })
                .is_ok()
        );
        assert_eq!(acceptance.result, AcceptanceResult::Shortage);
        acceptance.mark_posted().unwrap();
        // from == to 幂等迁移恒合法（state.rs 契约）；POSTED 不可编辑由 update 把关。
        assert!(acceptance.mark_posted().is_ok());
        assert!(
            acceptance
                .update(CustomerAcceptanceUpdate { result: None })
                .is_err(),
            "已过账不可编辑"
        );
        assert!(acceptance.reverse(CustomerAcceptanceId::new("a6")).is_ok());
        assert!(
            acceptance.reverse(CustomerAcceptanceId::new("a7")).is_ok(),
            "REVERSED 幂等迁移合法，且无法迁移到其他状态"
        );

        assert!(ensure_transition(CustomerAcceptanceState::Draft, CustomerAcceptanceState::Posted).is_ok());
        assert!(
            ensure_transition(CustomerAcceptanceState::Posted, CustomerAcceptanceState::Reversed).is_ok()
        );
        assert!(
            ensure_transition(CustomerAcceptanceState::Draft, CustomerAcceptanceState::Reversed).is_err()
        );
        assert!(
            ensure_transition(CustomerAcceptanceState::Reversed, CustomerAcceptanceState::Posted).is_err()
        );
        assert!(ensure_transition(CustomerAcceptanceState::Draft, CustomerAcceptanceState::Draft).is_ok());
    }

    /// happy path：验收行创建成功，说明规范化。
    #[test]
    fn line_new_succeeds() {
        let line = CustomerAcceptanceLine::new(CustomerAcceptanceLineId::new("cl-1"), line_data()).unwrap();
        assert_eq!(line.reason.as_deref(), Some("部分短少"));
        assert_eq!(line.accepted_quantity, Quantity::from_str("9").unwrap());
    }

    /// 失败路径：负数量、行号越界、说明超长。
    #[test]
    fn line_rejects_quantity_violations() {
        let negative = CustomerAcceptanceLineData {
            short_quantity: Quantity::from_str("-1").unwrap(),
            ..line_data()
        };
        assert!(CustomerAcceptanceLine::new(CustomerAcceptanceLineId::new("cl-2"), negative).is_err());

        let zero_line_no = CustomerAcceptanceLineData {
            line_no: 0,
            ..line_data()
        };
        assert!(CustomerAcceptanceLine::new(CustomerAcceptanceLineId::new("cl-3"), zero_line_no).is_err());

        let overlong_reason = CustomerAcceptanceLineData {
            reason: Some("x".repeat(513)),
            ..line_data()
        };
        assert!(CustomerAcceptanceLine::new(CustomerAcceptanceLineId::new("cl-4"), overlong_reason).is_err());
    }

    /// 序列化：结果枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&AcceptanceResult::ServiceFailed).unwrap(),
            "\"SERVICE_FAILED\""
        );
        assert_eq!(
            serde_json::to_string(&CustomerAcceptanceState::Posted).unwrap(),
            "\"POSTED\""
        );
        assert_eq!(AcceptanceResult::Shortage.label(), "短少");

        let mut acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("a8"), data()).unwrap();
        acceptance.mark_posted().unwrap();
        let roundtrip: CustomerAcceptance =
            bson::from_document(bson::to_document(&acceptance).unwrap()).unwrap();
        assert_eq!(roundtrip, acceptance);
    }

    /// 客户验收单无审批约束：不得出现绑定字段或审批状态机。
    #[test]
    fn customer_acceptance_has_no_approval_binding_or_state_machine() {
        let acceptance = CustomerAcceptance::new(CustomerAcceptanceId::new("acceptance-1"), data()).unwrap();
        let value = serde_json::to_value(&acceptance).unwrap();
        let object = value.as_object().expect("验收单序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_subject_version"));
        assert!(!object.contains_key("pending_allocations"));
        assert_eq!(acceptance.status, CustomerAcceptanceState::Draft);
        assert_eq!(CustomerAcceptanceState::Draft.as_str(), "DRAFT");
        assert_eq!(CustomerAcceptanceState::Posted.as_str(), "POSTED");
        assert_eq!(CustomerAcceptanceState::Reversed.as_str(), "REVERSED");

        let production = include_str!("customer_acceptance.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("approval_subject_version"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("PENDING_REVIEW"));
    }
}
