//! `acceptance_fulfillment_allocation`：验收行对履约事实的分配（数据模型 §6.7）。
//!
//! 合同 §4.3 将所属 `CustomerAcceptance` 签署为 `NO_APPROVAL`：分配只表达履约
//! 事实覆盖，不得新增审批绑定字段、审批实例引用或任务归属。
//!
//! 同一验收行可以对应多批履约，同一履约事实可以分批验收；分配是正式事实，
//! 纠错通过追加 `REVERSE` 分配表达（§6.7）。Service 在验收事务内加载事实与
//! 全部分配；净数量、剩余可验收量和追加上限由本实体执行确定性校验（§8.2
//! 第 5 条）。

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
/// 分配是正式事实，不设业务软删除（§4.5.1）；Service 加载同一履约事实的
/// 全部分配后，由本实体计算净验收数量并校验不超过净成功履约数量。
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

    /// 计算指定履约事实的净验收分配数量。
    ///
    /// # 参数
    /// * `allocations` - 已加载的验收分配集合
    /// * `fulfillment_line_id` - 履约事实行主键
    ///
    /// # 返回
    /// 返回 `APPLY - REVERSE` 的净数量。
    ///
    /// # 错误
    /// 反向分配导致净数量为负，或结果超出统一数量精度范围时返回错误。
    pub fn net_quantity_for_fact(
        allocations: &[AcceptanceFulfillmentAllocation],
        fulfillment_line_id: &str,
    ) -> Result<Quantity> {
        let net = allocations
            .iter()
            .filter(|allocation| allocation.fulfillment_line_id == fulfillment_line_id)
            .fold(rust_decimal::Decimal::ZERO, |net, allocation| {
                match allocation.allocation_action {
                    AllocationAction::Apply => net + allocation.allocated_quantity.to_decimal(),
                    AllocationAction::Reverse => net - allocation.allocated_quantity.to_decimal(),
                }
            });
        if net < rust_decimal::Decimal::ZERO {
            return Err(Error::from("履约事实的净验收数量不得为负"));
        }
        Quantity::try_from(net).map_err(|error| Error::from(error.to_string()))
    }

    /// 计算履约事实剩余可验收数量。
    ///
    /// # 参数
    /// * `successful_quantity` - 履约事实的净成功数量
    /// * `allocations` - 已加载的验收分配集合
    /// * `fulfillment_line_id` - 履约事实行主键
    ///
    /// # 返回
    /// 返回成功数量减净验收分配后的剩余数量。
    ///
    /// # 错误
    /// 既有净验收已超过成功数量，或结果超出统一数量精度时返回错误。
    pub fn eligible_quantity_for_fact(
        successful_quantity: Quantity,
        allocations: &[AcceptanceFulfillmentAllocation],
        fulfillment_line_id: &str,
    ) -> Result<Quantity> {
        let net = Self::net_quantity_for_fact(allocations, fulfillment_line_id)?;
        if net.to_decimal() > successful_quantity.to_decimal() {
            return Err(Error::from("履约事实的净验收数量超过其净成功履约数量"));
        }
        Quantity::try_from(successful_quantity.to_decimal() - net.to_decimal())
            .map_err(|error| Error::from(error.to_string()))
    }

    /// 校验追加应用分配不会超过履约事实的净成功数量。
    ///
    /// # 参数
    /// * `successful_quantity` - 履约事实的净成功数量
    /// * `existing` - 该履约事实的既有分配
    /// * `fulfillment_line_id` - 履约事实行主键
    /// * `applying_quantity` - 本次拟追加的应用数量
    ///
    /// # 返回
    /// 追加后仍不超上限时返回 `Ok(())`。
    ///
    /// # 错误
    /// 净数量计算失败或追加后超过成功数量时返回错误。
    pub fn ensure_apply_within_successful_quantity(
        successful_quantity: Quantity,
        existing: &[AcceptanceFulfillmentAllocation],
        fulfillment_line_id: &str,
        applying_quantity: Quantity,
    ) -> Result<()> {
        let net = Self::net_quantity_for_fact(existing, fulfillment_line_id)?;
        if net.to_decimal() + applying_quantity.to_decimal() > successful_quantity.to_decimal() {
            return Err(Error::from("履约事实的净验收数量超过其净成功履约数量"));
        }
        Ok(())
    }

    /// 校验一组分配属于可冲正的原始验收事实。
    ///
    /// 原始验收只能包含至少一条 `APPLY`；由冲正生成的验收包含 `REVERSE`，
    /// 不得再次作为冲正目标，否则会形成没有业务数量变化的反向记录。
    ///
    /// # 参数
    /// * `allocations` - 待冲正验收单的全部履约分配
    ///
    /// # 返回
    /// 全部为原始应用分配时返回 `Ok(())`。
    ///
    /// # 错误
    /// 分配为空、包含反向动作或带有反向引用时返回错误。
    pub fn ensure_reversible_source(allocations: &[AcceptanceFulfillmentAllocation]) -> Result<()> {
        if allocations.is_empty() {
            return Err(Error::from("原验收单没有可冲正的分配，无法冲正"));
        }
        if allocations.iter().any(|allocation| {
            allocation.allocation_action != AllocationAction::Apply
                || allocation.reverses_allocation_id.is_some()
        }) {
            return Err(Error::from("冲正记录不能再次冲正"));
        }
        Ok(())
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

    /// 原始应用分配可冲正；空分配和反向分配均必须拒绝。
    #[test]
    fn reversible_source_rejects_empty_and_reverse_allocations() {
        let apply = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-1"),
            apply_data(),
        )
        .unwrap();
        assert!(
            AcceptanceFulfillmentAllocation::ensure_reversible_source(std::slice::from_ref(&apply)).is_ok()
        );
        assert!(AcceptanceFulfillmentAllocation::ensure_reversible_source(&[]).is_err());

        let reverse = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-2"),
            AcceptanceFulfillmentAllocationData {
                allocation_action: AllocationAction::Reverse,
                reverses_allocation_id: Some(AcceptanceFulfillmentAllocationId::new("allocation-1")),
                ..apply_data()
            },
        )
        .unwrap();
        let error = AcceptanceFulfillmentAllocation::ensure_reversible_source(&[reverse]).unwrap_err();
        assert!(error.to_string().contains("不能再次冲正"));
    }

    /// 失败路径：数量越界、引用为空、动作与反向引用不一致。
    #[test]
    fn new_rejects_invalid_inputs() {
        let zero_quantity = AcceptanceFulfillmentAllocationData {
            allocated_quantity: Quantity::from_str("0").unwrap(),
            ..apply_data()
        };
        assert!(AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("a1"),
            zero_quantity
        )
        .is_err());

        let blank_line = AcceptanceFulfillmentAllocationData {
            fulfillment_line_id: "   ".to_string(),
            ..apply_data()
        };
        assert!(AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("a2"),
            blank_line
        )
        .is_err());

        let reverse_without_reference = AcceptanceFulfillmentAllocationData {
            allocation_action: AllocationAction::Reverse,
            reverses_allocation_id: None,
            ..apply_data()
        };
        assert!(AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("a3"),
            reverse_without_reference
        )
        .is_err());

        let apply_with_reference = AcceptanceFulfillmentAllocationData {
            reverses_allocation_id: Some(AcceptanceFulfillmentAllocationId::new("allocation-9")),
            ..apply_data()
        };
        assert!(AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("a4"),
            apply_with_reference
        )
        .is_err());
    }

    /// 净分配与剩余可验收数量按 APPLY - REVERSE 计算。
    #[test]
    fn net_and_eligible_quantities_are_conserved() {
        let apply = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-apply"),
            AcceptanceFulfillmentAllocationData {
                allocated_quantity: Quantity::from_str("4").unwrap(),
                ..apply_data()
            },
        )
        .unwrap();
        let reverse = AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new("allocation-reverse"),
            AcceptanceFulfillmentAllocationData {
                allocation_action: AllocationAction::Reverse,
                allocated_quantity: Quantity::from_str("1.5").unwrap(),
                reverses_allocation_id: Some(AcceptanceFulfillmentAllocationId::new("allocation-apply")),
                ..apply_data()
            },
        )
        .unwrap();
        let allocations = vec![apply, reverse];
        assert_eq!(
            AcceptanceFulfillmentAllocation::net_quantity_for_fact(&allocations, "dl-1").unwrap(),
            Quantity::from_str("2.5").unwrap()
        );
        assert_eq!(
            AcceptanceFulfillmentAllocation::eligible_quantity_for_fact(
                Quantity::from_str("5").unwrap(),
                &allocations,
                "dl-1",
            )
            .unwrap(),
            Quantity::from_str("2.5").unwrap()
        );
        assert!(
            AcceptanceFulfillmentAllocation::ensure_apply_within_successful_quantity(
                Quantity::from_str("5").unwrap(),
                &allocations,
                "dl-1",
                Quantity::from_str("2.5").unwrap(),
            )
            .is_ok()
        );
        assert!(
            AcceptanceFulfillmentAllocation::ensure_apply_within_successful_quantity(
                Quantity::from_str("5").unwrap(),
                &allocations,
                "dl-1",
                Quantity::from_str("2.6").unwrap(),
            )
            .is_err()
        );
        assert!(AcceptanceFulfillmentAllocation::eligible_quantity_for_fact(
            Quantity::from_str("2").unwrap(),
            &allocations,
            "dl-1",
        )
        .is_err());
        assert!(AcceptanceFulfillmentAllocation::net_quantity_for_fact(
            std::slice::from_ref(&allocations[1]),
            "dl-1",
        )
        .is_err());
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
            bson::deserialize_from_document(bson::serialize_to_document(&allocation).unwrap()).unwrap();
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
