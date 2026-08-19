//! `acceptance_fulfillment_allocation`：验收行对履约事实的分配（数据模型 §6.7）。
//!
//! 合同 §4.3 将所属 `CustomerAcceptance` 签署为 `NO_APPROVAL`：分配只表达履约
//! 事实覆盖，不得新增审批绑定字段、审批实例引用或任务归属。
//!
//! 同一验收行可以对应多批履约，同一履约事实可以分批验收；分配是正式事实，
//! 纠错通过追加 `REVERSE` 分配表达（§6.7）。「每个履约事实的净验收数量不得
//! 超过其净成功履约数量」与「验收行的通过、短少、拒收数量必须由其有效分配
//! 覆盖且合计守恒」是跨聚合校验，由 P3 在验收过账事务中完成（§8.2 第 5 条）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{AcceptanceFulfillmentAllocationId, CustomerAcceptanceLineId};
use crate::money::Quantity;

/// 履约事实类型（数据模型 §6.7：发货、电子交付或服务履约事实）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum FulfillmentFactType {
    /// 发货事实（`delivery_line`）。
    Delivery,
    /// 电子交付事实。
    ElectronicDelivery,
    /// 线下服务履约事实。
    ServiceFulfillment,
}

impl FulfillmentFactType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Delivery => "发货",
            Self::ElectronicDelivery => "电子交付",
            Self::ServiceFulfillment => "服务履约",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Delivery => "DELIVERY",
            Self::ElectronicDelivery => "ELECTRONIC_DELIVERY",
            Self::ServiceFulfillment => "SERVICE_FULFILLMENT",
        }
    }
}

/// 分配动作（数据模型 §6.7：`APPLY` 或 `REVERSE`）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum AllocationAction {
    /// 应用：把履约数量验收给验收行。
    Apply,
    /// 反向：冲销原分配。
    Reverse,
}

impl AllocationAction {
    /// 返回动作的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Apply => "应用",
            Self::Reverse => "反向",
        }
    }

    /// 返回动作的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Apply => "APPLY",
            Self::Reverse => "REVERSE",
        }
    }
}

/// 验收履约分配创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct AcceptanceFulfillmentAllocationData {
    /// 验收结果行。
    pub customer_acceptance_line_id: CustomerAcceptanceLineId,
    /// 发货、电子交付或服务履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 履约事实行（发货行/电子交付/服务履约的主键，跨域多态引用）。
    pub fulfillment_line_id: String,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
    /// 反向分配引用的原分配；`APPLY` 必须为空、`REVERSE` 必填。
    pub reverses_allocation_id: Option<AcceptanceFulfillmentAllocationId>,
}

/// 验收履约分配实体（数据模型 §6.7）。
///
/// 分配是正式事实，不设业务软删除（§4.5.1）；同一履约事实的净验收数量不得
/// 超过其净成功履约数量等跨聚合约束由 P3 校验。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct AcceptanceFulfillmentAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 验收结果行。
    pub customer_acceptance_line_id: CustomerAcceptanceLineId,
    /// 履约事实类型。
    pub fulfillment_fact_type: FulfillmentFactType,
    /// 履约事实行（发货行/电子交付/服务履约主键）。
    pub fulfillment_line_id: String,
    /// 分配动作。
    pub allocation_action: AllocationAction,
    /// 正数验收数量。
    pub allocated_quantity: Quantity,
    /// 反向分配引用的原分配。
    pub reverses_allocation_id: Option<AcceptanceFulfillmentAllocationId>,
}

impl AcceptanceFulfillmentAllocation {
    /// 创建验收履约分配。
    ///
    /// 完成分配数量正数校验与动作-引用一致性校验：`APPLY` 不得携带反向引用，
    /// `REVERSE` 必须携带被冲销的原分配（§6.7）。`fulfillment_line_id` 按
    /// `fulfillment_fact_type` 对应发货行/电子交付/服务履约主键；因三类事实
    /// 没有统一 ID newtype，此处以字符串承载（地基修订候选：公共的
    /// `FulfillmentLineRef` 值对象），P3 按类型解析并校验存在性。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::AcceptanceFulfillmentAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 分配数量非正、履约事实引用为空，或动作与反向引用不一致时返回错误。
    pub fn new(
        id: AcceptanceFulfillmentAllocationId,
        data: AcceptanceFulfillmentAllocationData,
    ) -> Result<Self> {
        if data.allocated_quantity.to_decimal() <= rust_decimal::Decimal::ZERO {
            return Err(Error::from("分配数量必须为正数"));
        }
        let fulfillment_line_id = data.fulfillment_line_id.trim().to_string();
        if fulfillment_line_id.is_empty() {
            return Err(Error::from("履约事实引用不能为空"));
        }
        if data.allocation_action == AllocationAction::Reverse && data.reverses_allocation_id.is_none() {
            return Err(Error::from("反向分配必须引用原分配"));
        }
        if data.allocation_action == AllocationAction::Apply && data.reverses_allocation_id.is_some() {
            return Err(Error::from("应用分配不得携带反向引用"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            customer_acceptance_line_id: data.customer_acceptance_line_id,
            fulfillment_fact_type: data.fulfillment_fact_type,
            fulfillment_line_id,
            allocation_action: data.allocation_action,
            allocated_quantity: data.allocated_quantity,
            reverses_allocation_id: data.reverses_allocation_id,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::AcceptanceFulfillmentAllocationId;
    use std::str::FromStr;

    fn apply_data() -> AcceptanceFulfillmentAllocationData {
        AcceptanceFulfillmentAllocationData {
            customer_acceptance_line_id: CustomerAcceptanceLineId::new("acceptance-line-1"),
            fulfillment_fact_type: FulfillmentFactType::Delivery,
            fulfillment_line_id: " dl-1 ".to_string(),
            allocation_action: AllocationAction::Apply,
            allocated_quantity: Quantity::from_str("2").unwrap(),
            reverses_allocation_id: None,
        }
    }

    /// happy path：应用分配创建成功并规范化引用。
    #[test]
    fn new_apply_succeeds() {
        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-1"),
            apply_data(),
        )
        .unwrap();
        assert_eq!(allocation.fulfillment_line_id, "dl-1");
        assert_eq!(allocation.allocated_quantity, Quantity::from_str("2").unwrap());
        assert_eq!(allocation.allocation_action, AllocationAction::Apply);
    }

    /// happy path：反向分配创建成功。
    #[test]
    fn new_reverse_succeeds() {
        let data = AcceptanceFulfillmentAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: Some(AcceptanceFulfillmentAllocationId::new("allocation-1")),
            ..apply_data()
        };
        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-2"),
            data,
        )
        .unwrap();
        assert_eq!(allocation.allocation_action, AllocationAction::Reverse);
    }

    /// 失败路径：数量越界、引用为空、动作与反向引用不一致。
    #[test]
    fn new_rejects_invalid_inputs() {
        let zero_quantity = AcceptanceFulfillmentAllocationData {
            allocated_quantity: Quantity::from_str("0").unwrap(),
            ..apply_data()
        };
        assert!(
            AcceptanceFulfillmentAllocation::new(AcceptanceFulfillmentAllocationId::new("a1"), zero_quantity)
                .is_err()
        );

        let blank_line = AcceptanceFulfillmentAllocationData {
            fulfillment_line_id: "   ".to_string(),
            ..apply_data()
        };
        assert!(
            AcceptanceFulfillmentAllocation::new(AcceptanceFulfillmentAllocationId::new("a2"), blank_line)
                .is_err()
        );

        let reverse_without_reference = AcceptanceFulfillmentAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..apply_data()
        };
        assert!(
            AcceptanceFulfillmentAllocation::new(
                AcceptanceFulfillmentAllocationId::new("a3"),
                reverse_without_reference
            )
            .is_err()
        );

        let apply_with_reference = AcceptanceFulfillmentAllocationData {
            reverses_allocation_id: Some(AcceptanceFulfillmentAllocationId::new("allocation-9")),
            ..apply_data()
        };
        assert!(
            AcceptanceFulfillmentAllocation::new(
                AcceptanceFulfillmentAllocationId::new("a4"),
                apply_with_reference
            )
            .is_err()
        );
    }

    /// 序列化：枚举稳定代码；实体 BSON 往返。
    #[test]
    fn serde_shapes_and_bson_roundtrip() {
        assert_eq!(
            serde_json::to_string(&FulfillmentFactType::ElectronicDelivery).unwrap(),
            "\"ELECTRONIC_DELIVERY\""
        );
        assert_eq!(
            serde_json::to_string(&AllocationAction::Reverse).unwrap(),
            "\"REVERSE\""
        );
        assert_eq!(FulfillmentFactType::ServiceFulfillment.label(), "服务履约");

        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-5"),
            apply_data(),
        )
        .unwrap();
        let roundtrip: AcceptanceFulfillmentAllocation =
            bson::from_document(bson::to_document(&allocation).unwrap()).unwrap();
        assert_eq!(roundtrip, allocation);
    }

    /// 验收分配无审批约束：不得出现绑定字段、实例或任务归属。
    #[test]
    fn allocation_has_no_approval_binding_or_work_item() {
        let allocation = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-1"),
            apply_data(),
        )
        .unwrap();
        let value = serde_json::to_value(&allocation).unwrap();
        let object = value.as_object().expect("分配序列化为对象");
        assert!(!object.contains_key("approval_binding"));
        assert!(!object.contains_key("approval_instance_id"));
        assert!(!object.contains_key("work_item_id"));
        assert!(!object.contains_key("approval_subject_version"));

        let production = include_str!("acceptance_fulfillment_allocation.rs")
            .split("#[cfg(test)]")
            .next()
            .expect("生产代码");
        assert!(!production.contains("IN_APPROVAL"));
        assert!(!production.contains("fn start_approval"));
        assert!(!production.contains("ApprovalDefinitionBinding"));
        assert!(!production.contains("WorkItem"));
        assert!(!production.contains("approval_instance"));
    }
}
