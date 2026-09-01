//! `acceptance_eligibility`：客户验收资格与销售履约进度领域投影（数据模型
//! §6.7 / §4.3.1）。
//!
//! 三类履约事实（发货、电子交付、服务履约）的净验收数量（APPLY − REVERSE）、
//! 剩余可验收数量与销售履约进度派生集中在 `AcceptanceFactEligibility` /
//! `AcceptanceLineEligibility` / `AcceptanceProgress` 三个投影 VO；Service 只负责
//! 跨聚合加载、按销售行组织输入与最终视图映射。数量汇总一律受检：精度或溢出
//! 错误向上传递，禁止静默回退为零（FUL-E07）。

use rust_decimal::Decimal;

use crate::errors::{Error, Result};
use crate::money::Quantity;
use crate::sales_order::FulfillmentProgress;

use super::acceptance_fulfillment_allocation::AcceptanceFulfillmentAllocation;

/// 单条履约事实的验收资格投影。
///
/// 由 Service 按事实行主键传入净成功履约数量与该事实的全部验收分配
/// （APPLY/REVERSE），本投影计算净验收数量与剩余可验收数量（守恒）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceFactEligibility {
    /// 履约事实行主键（发货行/电子交付/服务履约，跨域多态引用）。
    pub fulfillment_line_id: String,
    /// 净验收分配数量（APPLY − REVERSE）。
    pub net_accepted_quantity: Quantity,
    /// 剩余可验收数量（净成功履约数量 − 净验收数量）。
    pub eligible_quantity: Quantity,
}

impl AcceptanceFactEligibility {
    /// 计算单条履约事实的验收资格投影。
    ///
    /// 净验收与剩余可验收规则委托 [`AcceptanceFulfillmentAllocation`]，本类型
    /// 是三类履约事实共用的确定性投影入口。
    ///
    /// # 参数
    /// * `fulfillment_line_id` - 履约事实行主键
    /// * `net_successful_quantity` - 该事实的净成功履约数量
    /// * `allocations` - 该事实类型的全部验收分配（函数内按事实行主键过滤）
    ///
    /// # 返回
    /// 返回净验收与剩余可验收投影。
    ///
    /// # 错误
    /// 净验收为负、净验收超过净成功履约数量或数量超出统一精度时返回错误。
    pub fn from_fact(
        fulfillment_line_id: &str,
        net_successful_quantity: Quantity,
        allocations: &[AcceptanceFulfillmentAllocation],
    ) -> Result<Self> {
        let net_accepted_quantity =
            AcceptanceFulfillmentAllocation::net_quantity_for_fact(allocations, fulfillment_line_id)?;
        let eligible_quantity = AcceptanceFulfillmentAllocation::eligible_quantity_for_fact(
            net_successful_quantity,
            allocations,
            fulfillment_line_id,
        )?;
        Ok(Self {
            fulfillment_line_id: fulfillment_line_id.to_string(),
            net_accepted_quantity,
            eligible_quantity,
        })
    }
}

/// 销售行验收资格投影（一个销售稳定明细的全部履约事实汇总）。
///
/// 行级净验收与剩余可验收数量由逐事实投影受检汇总；任一事实的净验收超过
/// 成功履约数量或汇总溢出都会向上传递错误，禁止静默降为零（FUL-E07）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AcceptanceLineEligibility {
    /// 销售稳定明细。
    pub sales_order_line_id: String,
    /// 应履约数量。
    pub required_quantity: Quantity,
    /// 逐事实资格投影（保持输入顺序）。
    pub facts: Vec<AcceptanceFactEligibility>,
    /// 净已验收数量（逐事实净验收的受检汇总）。
    pub net_accepted_quantity: Quantity,
    /// 剩余可验收数量（逐事实剩余的受检汇总）。
    pub remaining_eligible_quantity: Quantity,
}

impl AcceptanceLineEligibility {
    /// 汇总一个销售行的验收资格。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 销售稳定明细
    /// * `required_quantity` - 销售行应履约数量
    /// * `facts` - 该销售行的逐事实资格投影（保持输入顺序）
    ///
    /// # 返回
    /// 返回行级净验收与剩余可验收汇总。
    ///
    /// # 错误
    /// 汇总算术溢出或结果超出统一数量精度时返回错误。
    pub fn from_facts(
        sales_order_line_id: String,
        required_quantity: Quantity,
        facts: Vec<AcceptanceFactEligibility>,
    ) -> Result<Self> {
        let mut net_accepted = Decimal::ZERO;
        let mut remaining_eligible = Decimal::ZERO;
        for fact in &facts {
            net_accepted = net_accepted
                .checked_add(fact.net_accepted_quantity.to_decimal())
                .ok_or_else(|| Error::from("净验收数量汇总溢出"))?;
            remaining_eligible = remaining_eligible
                .checked_add(fact.eligible_quantity.to_decimal())
                .ok_or_else(|| Error::from("剩余可验收数量汇总溢出"))?;
        }
        let net_accepted_quantity = Quantity::try_from(net_accepted)
            .map_err(|error| Error::from(format!("净验收数量超出统一精度：{error}")))?;
        let remaining_eligible_quantity = Quantity::try_from(remaining_eligible)
            .map_err(|error| Error::from(format!("剩余可验收数量超出统一精度：{error}")))?;
        Ok(Self {
            sales_order_line_id,
            required_quantity,
            facts,
            net_accepted_quantity,
            remaining_eligible_quantity,
        })
    }

    /// 判断销售行是否已全部验收。
    ///
    /// # 返回
    /// 净已验收数量不小于应履约数量时返回 `true`（应履约数量为零的行视为
    /// 已满足，与历史进度规则一致）。
    pub fn is_fully_fulfilled(&self) -> bool {
        self.net_accepted_quantity.to_decimal() >= self.required_quantity.to_decimal()
    }

    /// 判断销售行是否存在任一验收。
    ///
    /// # 返回
    /// 净已验收数量不为零时返回 `true`。
    pub fn has_acceptance(&self) -> bool {
        self.net_accepted_quantity.to_decimal() != Decimal::ZERO
    }

    /// 判断销售行是否仍存在剩余可验收数量。
    ///
    /// 每条事实的剩余可验收数量由投影保证非负，因此行级汇总不为零当且仅当
    /// 存在某条事实剩余可验收数量不为零。
    ///
    /// # 返回
    /// 行级剩余可验收数量不为零时返回 `true`。
    pub fn has_remaining_eligible(&self) -> bool {
        self.remaining_eligible_quantity.to_decimal() != Decimal::ZERO
    }
}

/// 销售单验收进度派生投影（数据模型 §4.3.1：客户验收通过即履约完成）。
///
/// 全部销售行净验收达到应履约数量 → 已完成；任一销售行存在验收但未全部
/// 完成 → 部分履约；否则 → 未开始。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AcceptanceProgress {
    /// 派生出的销售履约进度。
    pub progress: FulfillmentProgress,
    /// 是否仍存在剩余可验收数量。
    pub has_remaining_eligible: bool,
}

impl AcceptanceProgress {
    /// 从销售行资格投影派生销售单履约进度。
    ///
    /// # 参数
    /// * `lines` - 全部销售行的资格投影
    ///
    /// # 返回
    /// 存在销售行时返回进度与剩余可验收标记；没有任何销售行时返回 `None`，
    /// 表示无法派生进度（调用方不得写回进度）。
    pub fn derive(lines: &[AcceptanceLineEligibility]) -> Option<Self> {
        if lines.is_empty() {
            return None;
        }
        let mut all_fulfilled = true;
        let mut any_accepted = false;
        let mut has_remaining_eligible = false;
        for line in lines {
            all_fulfilled &= line.is_fully_fulfilled();
            any_accepted |= line.has_acceptance();
            has_remaining_eligible |= line.has_remaining_eligible();
        }
        let progress = if all_fulfilled {
            FulfillmentProgress::Completed
        } else if any_accepted {
            FulfillmentProgress::PartiallyFulfilled
        } else {
            FulfillmentProgress::NotStarted
        };
        Some(Self {
            progress,
            has_remaining_eligible,
        })
    }
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::fulfillment::{
        AcceptanceFulfillmentAllocation, AcceptanceFulfillmentAllocationData, AllocationAction,
        FulfillmentFactType,
    };
    use crate::ids::AcceptanceFulfillmentAllocationId;

    /// 构造一条 APPLY 分配（默认数量 4）。
    fn apply_allocation(line_id: &str, quantity: &str) -> AcceptanceFulfillmentAllocation {
        AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new(format!("allocation-{line_id}-{quantity}")),
            AcceptanceFulfillmentAllocationData {
                customer_acceptance_line_id: crate::ids::CustomerAcceptanceLineId::new("acceptance-line-1"),
                fulfillment_fact_type: FulfillmentFactType::Delivery,
                fulfillment_line_id: line_id.to_string(),
                allocation_action: AllocationAction::Apply,
                allocated_quantity: Quantity::from_str(quantity).unwrap(),
                reverses_allocation_id: None,
            },
        )
        .unwrap()
    }

    /// 构造冲销 `applied` 的 REVERSE 分配。
    fn reverse_allocation(
        line_id: &str,
        quantity: &str,
        applied: &AcceptanceFulfillmentAllocation,
    ) -> AcceptanceFulfillmentAllocation {
        AcceptanceFulfillmentAllocation::new(
            AcceptanceFulfillmentAllocationId::new(format!("allocation-{line_id}-reverse")),
            AcceptanceFulfillmentAllocationData {
                customer_acceptance_line_id: crate::ids::CustomerAcceptanceLineId::new("acceptance-line-1"),
                fulfillment_fact_type: FulfillmentFactType::Delivery,
                fulfillment_line_id: line_id.to_string(),
                allocation_action: AllocationAction::Reverse,
                allocated_quantity: Quantity::from_str(quantity).unwrap(),
                reverses_allocation_id: Some(applied.base.id.clone().into()),
            },
        )
        .unwrap()
    }

    /// 单条事实投影按 APPLY − REVERSE 计算净验收与剩余可验收数量。
    #[test]
    fn fact_projection_conserves_apply_reverse_net() {
        let applied = apply_allocation("dl-1", "4");
        let reversed = reverse_allocation("dl-1", "1.5", &applied);
        let allocations = vec![applied, reversed];
        let projection =
            AcceptanceFactEligibility::from_fact("dl-1", Quantity::from_str("5").unwrap(), &allocations)
                .unwrap();
        assert_eq!(projection.fulfillment_line_id, "dl-1");
        assert_eq!(
            projection.net_accepted_quantity,
            Quantity::from_str("2.5").unwrap()
        );
        assert_eq!(projection.eligible_quantity, Quantity::from_str("2.5").unwrap());
    }

    /// 净验收为负、净验收超过成功履约数量时投影明确失败，不得回退为零。
    #[test]
    fn fact_projection_fails_on_negative_net_or_over_acceptance() {
        let applied = apply_allocation("dl-1", "4");
        let reversed = reverse_allocation("dl-1", "4.5", &applied);
        assert!(AcceptanceFactEligibility::from_fact(
            "dl-1",
            Quantity::from_str("5").unwrap(),
            &[applied.clone(), reversed],
        )
        .is_err());

        let over = apply_allocation("dl-1", "6");
        assert!(AcceptanceFactEligibility::from_fact(
            "dl-1",
            Quantity::from_str("5").unwrap(),
            &[applied, over],
        )
        .is_err());
    }

    /// 行级投影按输入顺序汇总多条事实并保持事实顺序。
    #[test]
    fn line_projection_sums_facts_in_input_order() {
        let fact_a = AcceptanceFactEligibility {
            fulfillment_line_id: "dl-1".to_string(),
            net_accepted_quantity: Quantity::from_str("2.5").unwrap(),
            eligible_quantity: Quantity::from_str("2.5").unwrap(),
        };
        let fact_b = AcceptanceFactEligibility {
            fulfillment_line_id: "ed-1".to_string(),
            net_accepted_quantity: Quantity::from_str("1").unwrap(),
            eligible_quantity: Quantity::from_str("3").unwrap(),
        };
        let line = AcceptanceLineEligibility::from_facts(
            "sol-1".to_string(),
            Quantity::from_str("10").unwrap(),
            vec![fact_a.clone(), fact_b.clone()],
        )
        .unwrap();
        assert_eq!(line.net_accepted_quantity, Quantity::from_str("3.5").unwrap());
        assert_eq!(
            line.remaining_eligible_quantity,
            Quantity::from_str("5.5").unwrap()
        );
        assert_eq!(line.facts, vec![fact_a, fact_b]);
    }

    /// 空事实集合并入零应履约数量时行级汇总为零。
    #[test]
    fn line_projection_empty_facts_zeroes() {
        let line = AcceptanceLineEligibility::from_facts(
            "sol-1".to_string(),
            Quantity::from_str("0").unwrap(),
            Vec::new(),
        )
        .unwrap();
        assert_eq!(line.net_accepted_quantity, Quantity::from_str("0").unwrap());
        assert_eq!(line.remaining_eligible_quantity, Quantity::from_str("0").unwrap());
        assert!(line.is_fully_fulfilled());
        assert!(!line.has_acceptance());
        assert!(!line.has_remaining_eligible());
    }

    /// 汇总算术溢出时行级投影明确失败，不得回退为零。
    #[test]
    fn line_projection_overflow_fails() {
        let max = Quantity::from_str(&Decimal::MAX.to_string()).unwrap();
        let fact_a = AcceptanceFactEligibility {
            fulfillment_line_id: "dl-1".to_string(),
            net_accepted_quantity: max,
            eligible_quantity: Quantity::from_str("0").unwrap(),
        };
        let fact_b = AcceptanceFactEligibility {
            fulfillment_line_id: "dl-2".to_string(),
            net_accepted_quantity: max,
            eligible_quantity: Quantity::from_str("0").unwrap(),
        };
        let error = AcceptanceLineEligibility::from_facts(
            "sol-1".to_string(),
            Quantity::from_str("0").unwrap(),
            vec![fact_a, fact_b],
        )
        .unwrap_err();
        assert!(error.to_string().contains("溢出"));
    }

    /// 行级谓词覆盖未验收、部分验收与全部验收。
    #[test]
    fn line_predicates_cover_not_started_partial_full() {
        let no_acceptance = line_with_net(
            Quantity::from_str("0").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(!no_acceptance.has_acceptance());
        assert!(!no_acceptance.is_fully_fulfilled());

        let partial = line_with_net(
            Quantity::from_str("5").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(partial.has_acceptance());
        assert!(!partial.is_fully_fulfilled());

        let full = line_with_net(
            Quantity::from_str("10").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(full.has_acceptance());
        assert!(full.is_fully_fulfilled());

        let over = line_with_net(
            Quantity::from_str("12").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(over.is_fully_fulfilled());
    }

    /// 行级剩余可验收标记只在仍有剩余时成立。
    #[test]
    fn line_remaining_eligible_flag() {
        let remaining = line_with_net(
            Quantity::from_str("2").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(remaining.has_remaining_eligible());

        let exhausted = line_with_net(
            Quantity::from_str("10").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        assert!(!exhausted.has_remaining_eligible());
    }

    /// 进度派生覆盖未开始、部分履约、已完成及剩余可验收标记。
    #[test]
    fn progress_derive_covers_full_matrix() {
        let not_started = line_with_net(
            Quantity::from_str("0").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        let partial = line_with_net(
            Quantity::from_str("5").unwrap(),
            Quantity::from_str("10").unwrap(),
        );
        let full = line_with_net(
            Quantity::from_str("10").unwrap(),
            Quantity::from_str("10").unwrap(),
        );

        let derived = AcceptanceProgress::derive(std::slice::from_ref(&not_started)).unwrap();
        assert_eq!(derived.progress, FulfillmentProgress::NotStarted);
        assert!(derived.has_remaining_eligible);

        let derived = AcceptanceProgress::derive(std::slice::from_ref(&partial)).unwrap();
        assert_eq!(derived.progress, FulfillmentProgress::PartiallyFulfilled);
        assert!(derived.has_remaining_eligible);

        let derived = AcceptanceProgress::derive(std::slice::from_ref(&full)).unwrap();
        assert_eq!(derived.progress, FulfillmentProgress::Completed);
        assert!(!derived.has_remaining_eligible);

        // 一条未完成 + 一条完成 → 部分履约。
        let derived = AcceptanceProgress::derive(&[full, partial]).unwrap();
        assert_eq!(derived.progress, FulfillmentProgress::PartiallyFulfilled);

        // 全部行验收通过 → 已完成；剩余可验收标记跨行汇总。
        let derived = AcceptanceProgress::derive(&[
            line_with_net(
                Quantity::from_str("10").unwrap(),
                Quantity::from_str("10").unwrap(),
            ),
            line_with_net(Quantity::from_str("2").unwrap(), Quantity::from_str("2").unwrap()),
        ])
        .unwrap();
        assert_eq!(derived.progress, FulfillmentProgress::Completed);
        assert!(!derived.has_remaining_eligible);
    }

    /// 没有任何销售行时进度无法派生（调用方不得写回）。
    #[test]
    fn progress_derive_returns_none_for_empty_lines() {
        assert!(AcceptanceProgress::derive(&[]).is_none());
    }

    /// 构造指定净验收与应履约数量的行级投影。
    fn line_with_net(net_accepted: Quantity, required: Quantity) -> AcceptanceLineEligibility {
        AcceptanceLineEligibility::from_facts(
            "sol-1".to_string(),
            required,
            vec![AcceptanceFactEligibility {
                fulfillment_line_id: "dl-1".to_string(),
                net_accepted_quantity: net_accepted,
                eligible_quantity: Quantity::try_from(required.to_decimal() - net_accepted.to_decimal())
                    .unwrap(),
            }],
        )
        .unwrap()
    }
}
