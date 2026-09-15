use std::collections::{BTreeMap, HashMap};
use std::str::FromStr;

use erp_core::ids::WarehouseId;
use erp_core::money::Quantity;

use super::super::creation_basis::{
    BasisGroup, RequestedLine, basis_id_for, basis_scope_key, stable_line_id,
};
use super::super::types::FulfillmentResponsibility;
use super::assignment::{SourcingAssignment, SourcingAssignmentSet, SupplySourceType};
use super::stock::{RequestedStockLine, StockAllocationPlan, StockBasisGroup, stock_basis_id_for};
use crate::entity::facts::SalesOrderBasisFact as SalesOrder;

/// 已归入一张采购单的选源计划。
#[derive(Debug, Clone)]
pub struct SourcingDraftPlan {
    /// 命中的精确依据分组。
    pub group: BasisGroup,
    /// 仓库履约采购的目标收货仓。
    pub target_warehouse_id: Option<WarehouseId>,
    /// 本单规范化后的逐行数量。
    pub requested_lines: Vec<RequestedLine>,
}

/// 选源计划领域错误。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum SourcingPlanError {
    /// 依据、余额或剩余量在计划形成后发生变化，必须以最新事实重新验证。
    #[error("可分配供给数量已更新，请刷新后重试")]
    StaleFacts,
    /// 仓库履约契约违规。
    #[error("{0}")]
    WarehouseContract(String),
    /// 分配数量不符合单位粒度或单位精度缺失。
    #[error("{0}")]
    QuantityContract(String),
}

/// 把采购与库存选源行按精确依据分组并校验总量不变式的计划值对象。
#[derive(Debug, Clone)]
pub struct SourcingPlan {
    /// 按拆分维度稳定排序的采购草稿计划。
    purchase_plans: Vec<SourcingDraftPlan>,
    /// 按库存余额稳定排序的现有库存分配计划。
    stock_plans: Vec<StockAllocationPlan>,
}

impl SourcingPlan {
    /// 由销售单、当前依据事实与规范化选源行形成选源计划。
    ///
    /// # 参数
    /// * `order` - 已加载的销售稳定单
    /// * `purchase_groups` - 当前任务范围内的精确采购依据
    /// * `stock_groups` - 当前任务范围内的现有库存余额依据
    /// * `work_item_id` - 冻结本依据责任范围的开放任务
    /// * `assignments` - 已规范化选源行
    ///
    /// # 返回
    /// 返回分组完成且通过命令内总量校验的计划。
    ///
    /// # 错误
    /// 选源行依据失效、仓库履约契约违规或库存与采购合计超过剩余量时返回
    /// 领域错误。
    ///
    /// # 关键业务约束
    /// 同一拆分维度合并为一张采购单；同一销售行可拆库存与采购依据，但一次
    /// 命令的合计不得突破同一份最新剩余量；本方法只基于当前快照校验，事务
    /// 内必须由调用方以最新依据再次验证。
    pub fn plan(
        order: &SalesOrder,
        purchase_groups: &[BasisGroup],
        stock_groups: &[StockBasisGroup],
        work_item_id: &str,
        assignments: &SourcingAssignmentSet,
    ) -> std::result::Result<Self, SourcingPlanError> {
        let purchase_plans = plan_sourcing_drafts(order, purchase_groups, work_item_id, assignments)?;
        let stock_plans = plan_stock_allocations(order, stock_groups, work_item_id, assignments)?;
        validate_combined_line_totals(&purchase_plans, &stock_plans)?;
        Ok(Self { purchase_plans, stock_plans })
    }

    /// 以 guard 推进后重新加载的最新库存余额依据验证计划。
    ///
    /// # 参数
    /// * `latest_groups` - 事务内 guard 推进后重新计算的最新库存余额依据
    ///
    /// # 返回
    /// 全部预占数量未超过最新行剩余量与余额可用量时返回 `Ok(())`。
    ///
    /// # 错误
    /// 余额失效或逐行、逐余额累计超量时返回 [`SourcingPlanError::StaleFacts`]。
    ///
    /// # 关键业务约束
    /// 必须在 guard 推进且余额重载之后调用；单次快照校验不得替代事务内
    /// 重验，实际预占仍依赖余额 CAS。
    pub fn validate_against_latest_stock(
        &self,
        latest_groups: &[StockBasisGroup],
    ) -> std::result::Result<(), SourcingPlanError> {
        let mut line_totals = HashMap::<String, rust_decimal::Decimal>::new();
        let mut line_caps = HashMap::<String, rust_decimal::Decimal>::new();
        let mut balance_totals = HashMap::<String, rust_decimal::Decimal>::new();
        let mut balance_caps = HashMap::<String, rust_decimal::Decimal>::new();
        for plan in &self.stock_plans {
            let latest = latest_stock_group(latest_groups, &plan.group.balance.base.id)?;
            for requested in &plan.requested_lines {
                let line =
                    latest.line_for(&requested.sales_order_line_id).ok_or(SourcingPlanError::StaleFacts)?;
                validate_quantity(&line.coverage, requested.quantity)?;
                add_requested_total(
                    &mut line_totals,
                    &mut line_caps,
                    &requested.sales_order_line_id,
                    requested.quantity,
                    line.coverage.summary.remaining_quantity,
                );
                *balance_totals
                    .entry(latest.balance.base.id.clone())
                    .or_insert(rust_decimal::Decimal::ZERO) += requested.quantity.to_decimal();
                balance_caps
                    .insert(latest.balance.base.id.clone(), latest.balance.available_quantity.to_decimal());
            }
        }
        if exceeds_any_cap(&line_totals, &line_caps) || exceeds_any_cap(&balance_totals, &balance_caps) {
            return Err(SourcingPlanError::StaleFacts);
        }
        Ok(())
    }

    /// 以 guard 推进后重新加载的最新精确依据验证计划。
    ///
    /// # 参数
    /// * `latest_groups` - 事务内 guard 推进后重新计算的最新采购依据
    ///
    /// # 返回
    /// 全部拆分数量未超过最新销售剩余量且同一供给未被跨方案超量占用时返回
    /// `Ok(())`。
    ///
    /// # 错误
    /// 依据失效、销售行累计超量或同一供给跨履约责任累计超量时返回
    /// [`SourcingPlanError::StaleFacts`]。
    ///
    /// # 关键业务约束
    /// 一条销售行可以拆到多个方案，但一次命令的总量不得突破同一份最新剩余
    /// 量；同一供应商供给跨销售行或履约责任时仍共享该供给的可用量。
    pub fn validate_against_latest_sourcing(
        &self,
        latest_groups: &[BasisGroup],
    ) -> std::result::Result<(), SourcingPlanError> {
        let mut line_totals = HashMap::<String, rust_decimal::Decimal>::new();
        let mut line_caps = HashMap::<String, rust_decimal::Decimal>::new();
        let mut supply_totals = HashMap::<String, rust_decimal::Decimal>::new();
        let mut supply_caps = HashMap::<String, rust_decimal::Decimal>::new();
        for plan in &self.purchase_plans {
            let latest = latest_groups
                .iter()
                .find(|group| group.scope == plan.group.scope)
                .ok_or(SourcingPlanError::StaleFacts)?;
            for requested in &plan.requested_lines {
                let basis = latest
                    .lines
                    .iter()
                    .find(|line| stable_line_id(line) == requested.sales_order_line_id)
                    .ok_or(SourcingPlanError::StaleFacts)?;
                validate_quantity(&basis.coverage, requested.quantity)?;
                *line_totals
                    .entry(requested.sales_order_line_id.clone())
                    .or_insert(rust_decimal::Decimal::ZERO) += requested.quantity.to_decimal();
                line_caps.insert(
                    requested.sales_order_line_id.clone(),
                    basis.coverage.summary.remaining_quantity.to_decimal(),
                );
                let supply_key = basis.supply.offering.base.id.clone();
                *supply_totals.entry(supply_key.clone()).or_insert(rust_decimal::Decimal::ZERO) +=
                    requested.quantity.to_decimal();
                supply_caps.insert(
                    supply_key,
                    basis
                        .supply
                        .availability
                        .available_quantity
                        .map(Quantity::to_decimal)
                        .unwrap_or(rust_decimal::Decimal::MAX),
                );
            }
        }
        if exceeds_any_cap(&line_totals, &line_caps) || exceeds_any_cap(&supply_totals, &supply_caps) {
            return Err(SourcingPlanError::StaleFacts);
        }
        Ok(())
    }

    /// 返回按拆分维度稳定排序的采购草稿计划。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回待创建采购单计划切片。
    ///
    /// # 错误
    /// 无。
    pub fn purchase_plans(&self) -> &[SourcingDraftPlan] {
        &self.purchase_plans
    }

    /// 返回按库存余额稳定排序的现有库存分配计划。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回待预占库存余额计划切片。
    ///
    /// # 错误
    /// 无。
    pub fn stock_plans(&self) -> &[StockAllocationPlan] {
        &self.stock_plans
    }
}

/// 把采购来源行归入精确依据分组，形成待创建采购单计划。
///
/// # 参数
/// * `groups` - 当前任务范围内的精确依据
/// * `assignments` - 已规范化选源行
///
/// # 返回
/// 返回按拆分维度稳定排序的草稿计划。
///
/// # 错误
/// 销售行不属于当前任务或依据失效时返回 [`SourcingPlanError::StaleFacts`]；
/// 仓库履约契约违规时返回 [`SourcingPlanError::WarehouseContract`]。
///
/// # 关键业务约束
/// 同一拆分维度的选源行合并为一张采购单；不同目标仓必须拆分。
fn plan_sourcing_drafts(
    order: &SalesOrder,
    groups: &[BasisGroup],
    work_item_id: &str,
    assignments: &SourcingAssignmentSet,
) -> std::result::Result<Vec<SourcingDraftPlan>, SourcingPlanError> {
    let mut plans: BTreeMap<String, SourcingDraftPlan> = BTreeMap::new();
    for assignment in assignments
        .assignments()
        .iter()
        .filter(|assignment| assignment.source_type == SupplySourceType::Purchase)
    {
        let group = find_assignment_group(order, groups, work_item_id, assignment)?;
        let target_warehouse_id = target_warehouse_for_assignment(group, assignment)?;
        let key = format!(
            "{}|{}",
            basis_scope_key(&group.scope),
            target_warehouse_id.as_ref().map(ToString::to_string).unwrap_or_default()
        );
        let requested = RequestedLine {
            sales_order_line_id: assignment.sales_order_line_id.clone(),
            quantity: assignment.quantity,
            expected_delivery_date: assignment.expected_delivery_date,
        };
        if let Some(plan) = plans.get_mut(&key) {
            plan.requested_lines.push(requested);
        } else {
            plans.insert(
                key,
                SourcingDraftPlan {
                    group: group.clone(),
                    target_warehouse_id,
                    requested_lines: vec![requested],
                },
            );
        }
    }
    Ok(plans.into_values().collect())
}

/// 校验采购选源行的目标仓库契约。
///
/// # 参数
/// * `group` - 选源行命中的采购依据
/// * `assignment` - 已规范化选源行
///
/// # 返回
/// 仓库履约返回目标仓库，其他履约返回空。
///
/// # 错误
/// 仓库履约缺少目标仓，或非仓库履约携带目标仓时返回
/// [`SourcingPlanError::WarehouseContract`]。
///
/// # 关键业务约束
/// 目标仓只在仓库履约下参与依据身份与建单，其他履约不得携带。
fn target_warehouse_for_assignment(
    group: &BasisGroup,
    assignment: &SourcingAssignment,
) -> std::result::Result<Option<WarehouseId>, SourcingPlanError> {
    match group.scope.fulfillment_responsibility {
        FulfillmentResponsibility::Warehouse => assignment
            .target_warehouse_id
            .as_ref()
            .map(|value| Some(WarehouseId::new(value.clone())))
            .ok_or_else(|| SourcingPlanError::WarehouseContract("仓库履约必须先选择目标收货仓".to_string())),
        _ if assignment.target_warehouse_id.is_some() => {
            Err(SourcingPlanError::WarehouseContract("非仓库履约不能指定目标收货仓".to_string()))
        },
        _ => Ok(None),
    }
}

/// 把现有库存选源行按库存余额归组。
///
/// # 参数
/// * `order` - 已加载的销售稳定单
/// * `groups` - 当前任务范围内的库存余额依据
/// * `work_item_id` - 冻结本依据责任范围的开放任务
/// * `assignments` - 已规范化选源行
///
/// # 返回
/// 返回按余额主键稳定排序的现有库存分配计划。
///
/// # 错误
/// 选源行依据失效时返回 [`SourcingPlanError::StaleFacts`]。
///
/// # 关键业务约束
/// 同一余额的选源行合并为一次预占；同一依据不得被重复分配。
fn plan_stock_allocations(
    order: &SalesOrder,
    groups: &[StockBasisGroup],
    work_item_id: &str,
    assignments: &SourcingAssignmentSet,
) -> std::result::Result<Vec<StockAllocationPlan>, SourcingPlanError> {
    let mut plans = BTreeMap::<String, StockAllocationPlan>::new();
    for assignment in assignments
        .assignments()
        .iter()
        .filter(|assignment| assignment.source_type == SupplySourceType::ExistingStock)
    {
        let group = groups
            .iter()
            .find(|group| {
                stock_basis_id_for(order, group, work_item_id) == assignment.basis_id
                    && group.line_for(&assignment.sales_order_line_id).is_some()
            })
            .ok_or(SourcingPlanError::StaleFacts)?;
        let requested = RequestedStockLine {
            sales_order_line_id: assignment.sales_order_line_id.clone(),
            quantity: assignment.quantity,
        };
        plans
            .entry(group.balance.base.id.clone())
            .and_modify(|plan| plan.requested_lines.push(requested.clone()))
            .or_insert_with(|| StockAllocationPlan {
                group: group.clone(),
                requested_lines: vec![requested],
            });
    }
    Ok(plans.into_values().collect())
}

/// 校验同一命令内库存和采购拆分合计不超过当前销售缺口。
///
/// # 参数
/// * `purchase_plans` - 采购草稿计划
/// * `stock_plans` - 现有库存分配计划
///
/// # 返回
/// 全部销售行合计未超过当前剩余量时返回 `Ok(())`。
///
/// # 错误
/// 计划行依据失效或任一销售行合计超量时返回
/// [`SourcingPlanError::StaleFacts`]。
///
/// # 关键业务约束
/// 库存与采购按同一份剩余量共享上限；本校验基于计划形成时的快照，事务内
/// 必须再次以最新依据验证。
fn validate_combined_line_totals(
    purchase_plans: &[SourcingDraftPlan],
    stock_plans: &[StockAllocationPlan],
) -> std::result::Result<(), SourcingPlanError> {
    let mut totals = HashMap::<String, rust_decimal::Decimal>::new();
    let mut caps = HashMap::<String, rust_decimal::Decimal>::new();
    for plan in purchase_plans {
        for requested in &plan.requested_lines {
            let line = plan
                .group
                .lines
                .iter()
                .find(|line| stable_line_id(line) == requested.sales_order_line_id)
                .ok_or(SourcingPlanError::StaleFacts)?;
            validate_quantity(&line.coverage, requested.quantity)?;
            add_requested_total(
                &mut totals,
                &mut caps,
                &requested.sales_order_line_id,
                requested.quantity,
                line.coverage.summary.remaining_quantity,
            );
        }
    }
    for plan in stock_plans {
        for requested in &plan.requested_lines {
            let line =
                plan.group.line_for(&requested.sales_order_line_id).ok_or(SourcingPlanError::StaleFacts)?;
            validate_quantity(&line.coverage, requested.quantity)?;
            add_requested_total(
                &mut totals,
                &mut caps,
                &requested.sales_order_line_id,
                requested.quantity,
                line.coverage.summary.remaining_quantity,
            );
        }
    }
    if exceeds_any_cap(&totals, &caps) {
        return Err(SourcingPlanError::StaleFacts);
    }
    Ok(())
}

/// 累加一条请求数量并登记该稳定销售行的统一上限。
///
/// # 参数
/// * `totals` - 稳定销售行到累计数量的映射
/// * `caps` - 稳定销售行到剩余量上限的映射
/// * `line_id` - 稳定销售行
/// * `quantity` - 本次请求数量
/// * `cap` - 该行最新剩余量上限
///
/// # 返回
/// 无。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 同一稳定销售行跨采购与库存方案累计，上限取最后登记的最新剩余量。
fn add_requested_total(
    totals: &mut HashMap<String, rust_decimal::Decimal>,
    caps: &mut HashMap<String, rust_decimal::Decimal>,
    line_id: &str,
    quantity: Quantity,
    cap: Quantity,
) {
    *totals.entry(line_id.to_string()).or_insert(rust_decimal::Decimal::ZERO) += quantity.to_decimal();
    caps.insert(line_id.to_string(), cap.to_decimal());
}

/// 查找最新库存余额依据。
///
/// # 参数
/// * `groups` - 最新库存余额依据
/// * `balance_id` - 计划命中的余额主键
///
/// # 返回
/// 命中时返回该余额依据。
///
/// # 错误
/// 余额已失效时返回 [`SourcingPlanError::StaleFacts`]。
///
/// # 关键业务约束
/// 余额依据在 guard 推进后可能被作废释放，必须以最新集合查找。
fn latest_stock_group<'a>(
    groups: &'a [StockBasisGroup],
    balance_id: &str,
) -> std::result::Result<&'a StockBasisGroup, SourcingPlanError> {
    groups.iter().find(|group| group.balance.base.id == balance_id).ok_or(SourcingPlanError::StaleFacts)
}

/// 查找一条选源行命中的精确依据。
///
/// # 参数
/// * `groups` - 当前任务范围内的精确依据
/// * `assignment` - 已规范化选源行
///
/// # 返回
/// 返回同时包含该销售行且 ID 与客户端选择一致的依据分组。
///
/// # 错误
/// 销售行不存在或依据已失效时返回 [`SourcingPlanError::StaleFacts`]。
///
/// # 关键业务约束
/// 不以供应商或 SKU 猜测路线，只接受当前开放任务生成的精确依据。
fn find_assignment_group<'a>(
    order: &SalesOrder,
    groups: &'a [BasisGroup],
    work_item_id: &str,
    assignment: &SourcingAssignment,
) -> std::result::Result<&'a BasisGroup, SourcingPlanError> {
    groups
        .iter()
        .find(|group| {
            basis_id_for(order, group, work_item_id, None) == assignment.basis_id
                && group.lines.iter().any(|line| stable_line_id(line) == assignment.sales_order_line_id)
        })
        .ok_or(SourcingPlanError::StaleFacts)
}

/// 判断任一累计数量是否缺少上限或超过上限。
///
/// # 参数
/// * `totals` - 累计数量映射
/// * `caps` - 上限映射
///
/// # 返回
/// 任一累计数量缺少上限或超过上限时返回 `true`。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 缺少上限视为失败关闭，不允许无上限放行。
fn exceeds_any_cap(
    totals: &HashMap<String, rust_decimal::Decimal>,
    caps: &HashMap<String, rust_decimal::Decimal>,
) -> bool {
    totals.iter().any(|(key, total)| caps.get(key).is_none_or(|cap| total > cap))
}

/// 返回合法分配数量零值。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回六位精度数量零值。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 只用于边界比较，不代表缺失业务数量。
pub(super) fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

/// 用当前依据单位精度校验采购与库存分配；缺失或不符统一返回数量合同错误。
fn validate_quantity(
    coverage: &crate::entity::purchase_order::SalesProcurementCoverageLine,
    quantity: Quantity,
) -> std::result::Result<(), SourcingPlanError> {
    crate::entity::purchase_order::ensure_sourcing_quantity(
        quantity,
        coverage.quantity_scale,
        &coverage.goods_line.base_unit_code,
    )
    .map_err(|error| SourcingPlanError::QuantityContract(error.to_string()))
}
