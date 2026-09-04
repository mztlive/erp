//! 选源计划领域值对象与纯规则。
//!
//! 选源命令把客户确认的逐行供给分配一次落地为现有库存预占与采购缺口单：选源
//! 行必须先规范化（字符串类型化、同销售行同依据去重、稳定排序），再按精确采购
//! 依据与库存余额分组形成计划，并保证库存与采购合计不超过最新销售剩余量。
//! 本模块承载规范化、分组、仓库履约契约与跨方案总量不变式等无 I/O 规则；
//! DTO 负责调用 [`SourcingAssignment::parse`] 完成字符串类型化，Repository
//! 返回销售单、精确依据与库存余额事实，Service 负责事务内最新事实重验、
//! 预占写入与采购单创建编排。

use std::collections::{BTreeMap, HashMap, HashSet};
use std::str::FromStr;

use serde::{Deserialize, Serialize};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::WarehouseId;
use crate::inventory::StockBalance;
use crate::money::Quantity;
use crate::sales_order::{SalesOrder, SalesOrderRevision};

use super::command_receipt::digest_parts;
use super::coverage::SalesProcurementCoverageLine;
use super::creation_basis::{basis_id_for, basis_scope_key, stable_line_id, BasisGroup, RequestedLine};
use super::types::FulfillmentResponsibility;

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
        let target_warehouse_id = target_warehouse_id
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(str::to_string);
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
    /// [`super::creation_basis::normalize_requested_lines`] 同序，保证命令
    /// 指纹与建单行序稳定。
    pub fn normalize(assignments: &[SourcingAssignment]) -> Result<Self> {
        let mut seen = HashSet::new();
        let mut normalized = Vec::with_capacity(assignments.len());
        for assignment in assignments {
            if !seen.insert((
                assignment.sales_order_line_id.clone(),
                assignment.basis_id.clone(),
            )) {
                return Err(Error::from("同一销售行不能重复使用同一履约方案"));
            }
            normalized.push(assignment.clone());
        }
        normalized.sort_by(|left, right| {
            left.sales_order_line_id
                .cmp(&right.sales_order_line_id)
                .then_with(|| left.basis_id.cmp(&right.basis_id))
        });
        Ok(Self {
            assignments: normalized,
        })
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

/// 一条可由现有库存直接满足的销售行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBasisLine {
    /// 当前销售版本行及统一供给覆盖摘要。
    pub coverage: SalesProcurementCoverageLine,
    /// 本余额本次最多可分配数量。
    pub max_create_quantity: Quantity,
}

impl StockBasisLine {
    /// 判断本行是否覆盖指定稳定销售行。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 稳定销售行
    ///
    /// # 返回
    /// 覆盖行匹配时返回 `true`。
    ///
    /// # 错误
    /// 无。
    fn covers(&self, sales_order_line_id: &str) -> bool {
        self.coverage.revision_line.sales_order_line_id.as_ref() == sales_order_line_id
    }
}

/// 一个仓库库存余额形成的现有库存供给依据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StockBasisGroup {
    /// 销售当前版本。
    pub revision: SalesOrderRevision,
    /// 被分配的库存余额。
    pub balance: StockBalance,
    /// 仓库当前名称；基础资料缺失时回退仓库 ID。
    pub warehouse_name: String,
    /// 该余额可满足的销售行。
    pub lines: Vec<StockBasisLine>,
}

impl StockBasisGroup {
    /// 查找余额依据中的稳定销售行。
    ///
    /// # 参数
    /// * `sales_order_line_id` - 稳定销售行
    ///
    /// # 返回
    /// 命中时返回该行，否则返回 `None`。
    ///
    /// # 错误
    /// 无。
    pub fn line_for(&self, sales_order_line_id: &str) -> Option<&StockBasisLine> {
        self.lines.iter().find(|line| line.covers(sales_order_line_id))
    }
}

/// 形成绑定销售 guard、库存余额版本与逐行剩余量的现有库存依据 ID。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `group` - 现有库存余额依据
/// * `work_item_id` - 冻结本依据责任范围的开放任务
///
/// # 返回
/// 返回 `{sales_order_id}:{sha256}` 稳定依据 ID。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// guard 每次成功创建后推进，使作废释放的剩余量可形成新依据；余额版本、
/// 可用量与逐行剩余量任一变化都会改变依据身份。
pub fn stock_basis_id_for(order: &SalesOrder, group: &StockBasisGroup, work_item_id: &str) -> String {
    let mut parts = vec![
        order.base.id.clone(),
        work_item_id.to_string(),
        order.procurement_guard_version.to_string(),
        group.revision.base.id.clone(),
        group.balance.base.id.clone(),
        group.balance.base.version.to_string(),
        group.balance.available_quantity.to_string(),
    ];
    parts.extend(group.lines.iter().map(|line| {
        format!(
            "{}|{}|{}|{}",
            line.coverage.revision_line.sales_order_line_id,
            line.coverage.revision_line.base.id,
            line.coverage.summary.remaining_quantity,
            line.max_create_quantity,
        )
    }));
    format!("{}:{}", order.base.id, digest_parts(parts))
}

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

/// 已归入一个库存余额的现有库存分配计划。
#[derive(Debug, Clone)]
pub struct StockAllocationPlan {
    /// 命中的现有库存依据。
    pub group: StockBasisGroup,
    /// 本余额逐销售行分配数量。
    pub requested_lines: Vec<RequestedStockLine>,
}

/// 已规范化的现有库存分配行。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RequestedStockLine {
    /// 稳定销售行。
    pub sales_order_line_id: String,
    /// 本次预占数量。
    pub quantity: Quantity,
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
        Ok(Self {
            purchase_plans,
            stock_plans,
        })
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
                let line = latest
                    .line_for(&requested.sales_order_line_id)
                    .ok_or(SourcingPlanError::StaleFacts)?;
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
                balance_caps.insert(
                    latest.balance.base.id.clone(),
                    latest.balance.available_quantity.to_decimal(),
                );
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
                *line_totals
                    .entry(requested.sales_order_line_id.clone())
                    .or_insert(rust_decimal::Decimal::ZERO) += requested.quantity.to_decimal();
                line_caps.insert(
                    requested.sales_order_line_id.clone(),
                    basis.coverage.summary.remaining_quantity.to_decimal(),
                );
                let supply_key = basis.supply.offering.base.id.clone();
                *supply_totals
                    .entry(supply_key.clone())
                    .or_insert(rust_decimal::Decimal::ZERO) += requested.quantity.to_decimal();
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
            target_warehouse_id
                .as_ref()
                .map(ToString::to_string)
                .unwrap_or_default()
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
        _ if assignment.target_warehouse_id.is_some() => Err(SourcingPlanError::WarehouseContract(
            "非仓库履约不能指定目标收货仓".to_string(),
        )),
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
            let line = plan
                .group
                .line_for(&requested.sales_order_line_id)
                .ok_or(SourcingPlanError::StaleFacts)?;
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
    *totals
        .entry(line_id.to_string())
        .or_insert(rust_decimal::Decimal::ZERO) += quantity.to_decimal();
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
    groups
        .iter()
        .find(|group| group.balance.base.id == balance_id)
        .ok_or(SourcingPlanError::StaleFacts)
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
                && group
                    .lines
                    .iter()
                    .any(|line| stable_line_id(line) == assignment.sales_order_line_id)
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
    totals
        .iter()
        .any(|(key, total)| caps.get(key).is_none_or(|cap| total > cap))
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
fn zero_quantity() -> Quantity {
    Quantity::from_str("0").expect("零数量合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::common::time::Instant;
    use crate::ids::{
        SalesOrderId, SalesOrderRevisionId, SalesOrderRevisionLineId, SkuId, StockBalanceId,
        SupplierAccountId, SupplierOfferingAvailabilityId, SupplierOfferingId, SupplierOfferingRevisionId,
        WarehouseId,
    };
    use crate::inventory::{StockBalance, StockBalanceData};
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::sales_order::revision::{
        SalesOrderGoodsServiceLineRevision, SalesOrderGoodsServiceLineRevisionData, SalesOrderRevision,
        SalesOrderRevisionData, SalesOrderRevisionLine, SalesOrderRevisionLineData,
    };
    use crate::sales_order::snapshot::HeaderSnapshotData;
    use crate::sales_order::{CommercialStatus, LineType, ProcurementCoverageSummary, RevisionSource};
    use crate::supplier_offering::{
        AvailabilityStatus, OfferingSourceType, PrefillSourceRefs, SupplierOffering,
        SupplierOfferingAvailability, SupplierOfferingAvailabilityData, SupplierOfferingData,
        SupplierOfferingRevision, SupplierOfferingRevisionData,
    };

    use super::{
        stock_basis_id_for, SourcingAssignment, SourcingAssignmentSet, SourcingPlan, SourcingPlanError,
        StockBasisGroup, StockBasisLine, SupplySourceType,
    };
    use crate::purchase_order::coverage::SalesProcurementCoverageLine;
    use crate::purchase_order::creation_basis::{
        basis_id_for, BasisGroup, BasisLine, BasisScope, LineSupply,
    };
    use crate::purchase_order::{FulfillmentResponsibility, PurchaseType};

    /// 构造销售当前版本头。
    fn revision(id: &str) -> SalesOrderRevision {
        SalesOrderRevision::new(
            SalesOrderRevisionId::new(format!("rev-{id}")),
            SalesOrderRevisionData {
                sales_order_id: SalesOrderId::new("so-1"),
                revision_no: 1,
                revision_source: RevisionSource::ErpApproval,
                previous_revision_id: None,
                content_hash: format!("hash-{id}"),
                customer_revision_id: None,
                contract_revision_id: None,
                snapshot: HeaderSnapshotData {
                    customer_name: "客户".to_string(),
                    contract_no: None,
                    settlement_party_name: None,
                    payment_term_code: "NET-30".to_string(),
                    payment_term_name: "净30天".to_string(),
                    invoice_type: "增值税专用发票".to_string(),
                    tax_point: "13".to_string(),
                },
                project_name: None,
                business_remark: None,
                voucher_category_sku_id: None,
                voucher_expiry_at: None,
                gross_amount: Amount::from_str("100").unwrap(),
                net_amount: Amount::from_str("100").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                effective_at: Instant::from_unix_secs(1_800_000_000),
                recorded_at: Instant::from_unix_secs(1_800_000_000),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本公共行。
    fn revision_line(id: &str, stable_line_id: &str) -> SalesOrderRevisionLine {
        SalesOrderRevisionLine::new(
            SalesOrderRevisionLineId::new(id),
            SalesOrderRevisionLineData {
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: crate::ids::SalesOrderLineId::new(stable_line_id),
                line_no: 1,
                line_type: LineType::GoodsService,
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                sales_tax_rate: Rate::from_str("0").unwrap(),
                item_name_snapshot: "商品".to_string(),
                spec_snapshot: Some("规格".to_string()),
                unit_snapshot: Some("件".to_string()),
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本商品/服务子类型行。
    fn goods_line(revision_line_id: &str) -> SalesOrderGoodsServiceLineRevision {
        SalesOrderGoodsServiceLineRevision::new(
            crate::ids::SalesOrderGoodsServiceLineRevisionId::new(format!("goods-{revision_line_id}")),
            SalesOrderGoodsServiceLineRevisionData {
                revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: crate::ids::SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: Quantity::from_str("10").unwrap(),
                base_unit_code: "件".to_string(),
                unit_price_gross: UnitPrice::from_str("5").unwrap(),
            },
        )
        .unwrap()
    }

    /// 构造一条销售覆盖目标行；剩余量等于目标减覆盖。
    fn coverage_line(stable_line_id: &str, total: &str, covered: &str) -> SalesProcurementCoverageLine {
        SalesProcurementCoverageLine {
            revision_line: revision_line(&format!("sorl-{stable_line_id}"), stable_line_id),
            goods_line: goods_line(&format!("sorl-{stable_line_id}")),
            product_kind: crate::catalog::ProductKind::Physical,
            summary: ProcurementCoverageSummary::new(
                Quantity::from_str(total).unwrap(),
                Quantity::from_str(covered).unwrap(),
            )
            .unwrap(),
        }
    }

    /// 计算剩余量文本对应的数量。
    fn remaining(total: &str, covered: &str) -> Quantity {
        Quantity::try_from(
            Quantity::from_str(total).unwrap().to_decimal()
                - Quantity::from_str(covered).unwrap().to_decimal(),
        )
        .unwrap()
    }

    /// 构造供给稳定身份。
    fn offering(id: &str, supplier_id: &str) -> SupplierOffering {
        SupplierOffering::new(
            SupplierOfferingId::new(id),
            SupplierOfferingData {
                sku_id: SkuId::new("sku-1"),
                supplier_id: SupplierAccountId::new(supplier_id),
                supplier_product_code: None,
                supplier_sku_code: format!("SKU-{id}"),
                source_type: OfferingSourceType::Manual,
                source_connection_id: None,
            },
            "test",
        )
        .unwrap()
    }

    /// 构造供给商业条款修订。
    fn offering_revision(offering_id: &str) -> SupplierOfferingRevision {
        SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new(format!("offrev-{offering_id}")),
            SupplierOfferingRevisionData::from_gross_prices(
                SupplierOfferingId::new(offering_id),
                1,
                UnitPrice::from_str("6").unwrap(),
                UnitPrice::from_str("5").unwrap(),
                Rate::from_str("0.13").unwrap(),
                None,
                None,
                None,
                Quantity::from_str("1").unwrap(),
                vec!["全国".to_string()],
                Vec::new(),
                crate::common::time::BusinessDate::from_str("2026-01-01").unwrap(),
                None,
                PrefillSourceRefs {
                    input_tax_rate: None,
                    supply_region: None,
                    valid_from_date: None,
                    valid_from_timezone: None,
                    valid_from_calendar_version: None,
                },
            ),
        )
        .unwrap()
    }

    /// 构造供给实时可供投影。
    fn availability(offering_id: &str, quantity: &str) -> SupplierOfferingAvailability {
        SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new(format!("avail-{offering_id}")),
            SupplierOfferingAvailabilityData {
                supplier_offering_id: SupplierOfferingId::new(offering_id),
                availability_status: AvailabilityStatus::Available,
                available_quantity: Some(Quantity::from_str(quantity).unwrap()),
                source_updated_at: Instant::from_unix_secs(1_800_000_000),
                received_at: Instant::from_unix_secs(1_800_000_000),
                source_revision_token: None,
                updated_by: "test".to_string(),
            },
        )
        .unwrap()
    }

    /// 构造一条合格供给。
    fn line_supply(offering_id: &str, supplier_id: &str, available: &str) -> LineSupply {
        LineSupply {
            offering: offering(offering_id, supplier_id),
            revision: offering_revision(offering_id),
            availability: availability(offering_id, available),
        }
    }

    /// 构造销售稳定单。
    fn sales_order(id: &str) -> crate::sales_order::SalesOrder {
        let mut order = crate::sales_order::SalesOrder::new(
            SalesOrderId::new(id),
            crate::sales_order::SalesOrderData {
                order_no: format!("SO-{id}"),
                business_type: crate::sales_order::BusinessType::GoodsService,
                origin_system: crate::sales_order::OriginSystem::Erp,
                source_identity_id: None,
                customer_id: crate::ids::CustomerAccountId::new("customer-1"),
                contract_id: None,
                settlement_party_id: crate::ids::PartyId::new("party-1"),
                source_status_code: None,
            },
            "seller-1",
        )
        .unwrap();
        order.commercial_status = CommercialStatus::Effective;
        order.procurement_guard_version = 3;
        order
    }

    /// 构造完整精确依据分组；行元组为 `(稳定销售行, 目标数量, 覆盖数量)`。
    fn basis_group(
        supplier_id: &str,
        payment_term_code: &str,
        fulfillment: FulfillmentResponsibility,
        supply_available: &str,
        lines: &[(&str, &str, &str)],
    ) -> BasisGroup {
        let lines = lines
            .iter()
            .map(|(line_id, total, covered)| BasisLine {
                coverage: coverage_line(line_id, total, covered),
                supply: line_supply("offering-1", supplier_id, supply_available),
                max_create_quantity: remaining(total, covered),
            })
            .collect();
        BasisGroup {
            revision: revision("1"),
            scope: BasisScope {
                supplier_id: SupplierAccountId::new(supplier_id),
                purchase_type: PurchaseType::Physical,
                payment_term_code: payment_term_code.to_string(),
                fulfillment_responsibility: fulfillment,
            },
            business_category: None,
            lines,
        }
    }

    /// 构造现有库存余额依据；行元组为 `(稳定销售行, 目标数量, 覆盖数量)`。
    fn stock_basis_group(
        balance_id: &str,
        warehouse_id: &str,
        available: &str,
        lines: &[(&str, &str, &str)],
    ) -> StockBasisGroup {
        let available = Quantity::from_str(available).unwrap();
        let lines = lines
            .iter()
            .map(|(line_id, total, covered)| StockBasisLine {
                coverage: coverage_line(line_id, total, covered),
                max_create_quantity: remaining(total, covered),
            })
            .collect();
        StockBasisGroup {
            revision: revision("1"),
            balance: StockBalance::new(
                StockBalanceId::new(balance_id),
                StockBalanceData {
                    warehouse_id: WarehouseId::new(warehouse_id),
                    sku_id: SkuId::new("sku-1"),
                    on_hand_quantity: available,
                    reserved_quantity: Quantity::from_str("0").unwrap(),
                    available_quantity: available,
                    last_movement_id: None,
                },
            )
            .unwrap(),
            warehouse_name: warehouse_id.to_string(),
            lines,
        }
    }

    /// 构造一条已类型化的选源行。
    fn assignment(
        line_id: &str,
        basis_id: &str,
        source_type: SupplySourceType,
        target_warehouse_id: Option<&str>,
        quantity: &str,
    ) -> SourcingAssignment {
        SourcingAssignment {
            sales_order_line_id: line_id.to_string(),
            basis_id: basis_id.to_string(),
            source_type,
            target_warehouse_id: target_warehouse_id.map(str::to_string),
            quantity: Quantity::from_str(quantity).unwrap(),
            expected_delivery_date: crate::common::time::BusinessDate::from_str("2026-09-01").unwrap(),
        }
    }

    /// 解析成功时去除首尾空白并类型化数量与日期。
    #[test]
    fn parse_trims_and_types_valid_assignment() {
        let parsed = SourcingAssignment::parse(
            " sol-1 ",
            " basis-1 ",
            SupplySourceType::Purchase,
            Some(" wh-1 "),
            " 10 ",
            " 2026-09-01 ",
        )
        .expect("合法选源行必须解析成功");

        assert_eq!(parsed.sales_order_line_id, "sol-1");
        assert_eq!(parsed.basis_id, "basis-1");
        assert_eq!(parsed.target_warehouse_id.as_deref(), Some("wh-1"));
        assert_eq!(parsed.quantity, Quantity::from_str("10").unwrap());
    }

    /// 空白销售行必须拒绝。
    #[test]
    fn parse_rejects_blank_sales_line() {
        let error = SourcingAssignment::parse(
            " ",
            "basis-1",
            SupplySourceType::Purchase,
            None,
            "10",
            "2026-09-01",
        )
        .expect_err("空白销售行必须失败");
        assert_eq!(error.to_string(), "销售行不能为空");
    }

    /// 空白依据必须拒绝。
    #[test]
    fn parse_rejects_blank_basis() {
        let error =
            SourcingAssignment::parse("sol-1", " ", SupplySourceType::Purchase, None, "10", "2026-09-01")
                .expect_err("空白依据必须失败");
        assert_eq!(error.to_string(), "履约方案不能为空");
    }

    /// 非法数量文本必须拒绝。
    #[test]
    fn parse_rejects_invalid_quantity() {
        let error = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::Purchase,
            None,
            "abc",
            "2026-09-01",
        )
        .expect_err("非法数量必须失败");
        assert!(error.to_string().starts_with("本次分配数量非法"));
    }

    /// 非法预计交付日必须拒绝。
    #[test]
    fn parse_rejects_invalid_delivery_date() {
        let error = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::Purchase,
            None,
            "10",
            "not-a-date",
        )
        .expect_err("非法日期必须失败");
        assert!(error.to_string().starts_with("预计交付日非法"));
    }

    /// 零数量与负数量必须拒绝。
    #[test]
    fn parse_rejects_zero_and_negative_quantity() {
        for quantity in ["0", "-1"] {
            let error = SourcingAssignment::parse(
                "sol-1",
                "basis-1",
                SupplySourceType::Purchase,
                None,
                quantity,
                "2026-09-01",
            )
            .expect_err("非正数量必须失败");
            assert_eq!(error.to_string(), "本次分配数量必须大于 0");
        }
    }

    /// 现有库存不能另行指定目标仓。
    #[test]
    fn parse_rejects_target_warehouse_for_existing_stock() {
        let error = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::ExistingStock,
            Some("wh-1"),
            "10",
            "2026-09-01",
        )
        .expect_err("现有库存指定目标仓必须失败");
        assert_eq!(
            error.to_string(),
            "现有库存由所选库存余额确定仓库，不能另行指定目标仓"
        );
    }

    /// 现有库存不携带目标仓时解析成功。
    #[test]
    fn parse_accepts_existing_stock_without_target_warehouse() {
        let parsed = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::ExistingStock,
            None,
            "10",
            "2026-09-01",
        )
        .expect("现有库存不指定目标仓必须成功");
        assert_eq!(parsed.source_type, SupplySourceType::ExistingStock);
        assert_eq!(parsed.target_warehouse_id, None);
    }

    /// 空白目标仓归一为未指定。
    #[test]
    fn parse_treats_blank_target_warehouse_as_none() {
        let parsed = SourcingAssignment::parse(
            "sol-1",
            "basis-1",
            SupplySourceType::Purchase,
            Some("  "),
            "10",
            "2026-09-01",
        )
        .expect("空白目标仓必须归一为空");
        assert_eq!(parsed.target_warehouse_id, None);
    }

    /// 同一销售行可拆到不同依据，但同一依据不能重复。
    #[test]
    fn normalize_allows_split_and_rejects_duplicate() {
        let set = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
            assignment("sol-1", "basis-b", SupplySourceType::Purchase, None, "1"),
        ])
        .expect("不同依据允许拆分");
        assert_eq!(set.assignments().len(), 2);

        let error = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
            assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
        ])
        .expect_err("重复分配必须失败");
        assert_eq!(error.to_string(), "同一销售行不能重复使用同一履约方案");
    }

    /// 规范化结果按销售行与依据稳定排序。
    #[test]
    fn normalize_sorts_stably_by_line_then_basis() {
        let set = SourcingAssignmentSet::normalize(&[
            assignment("sol-2", "basis-a", SupplySourceType::Purchase, None, "1"),
            assignment("sol-1", "basis-b", SupplySourceType::Purchase, None, "1"),
            assignment("sol-1", "basis-a", SupplySourceType::Purchase, None, "1"),
        ])
        .expect("合法集合必须成功");

        let ids = set
            .assignments()
            .iter()
            .map(|line| (line.sales_order_line_id.as_str(), line.basis_id.as_str()))
            .collect::<Vec<_>>();
        assert_eq!(
            ids,
            vec![("sol-1", "basis-a"), ("sol-1", "basis-b"), ("sol-2", "basis-a")]
        );
    }

    /// 空集合必须归一为空计划。
    #[test]
    fn normalize_accepts_empty_set() {
        let set = SourcingAssignmentSet::normalize(&[]).expect("空集合必须成功");
        assert!(set.assignments().is_empty());
    }

    /// 同一拆分维度与目标仓的多销售行合并为一张采购单。
    #[test]
    fn purchase_assignments_merge_into_one_plan_per_scope_and_warehouse() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::Warehouse,
            "8",
            &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "3"),
            assignment("sol-2", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "2"),
        ])
        .expect("合法集合必须成功");

        let plan = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect("计划必须成功");
        let drafts = plan.purchase_plans();
        assert_eq!(drafts.len(), 1);
        assert_eq!(
            drafts[0].target_warehouse_id.as_ref().map(ToString::to_string),
            Some("wh-1".to_string())
        );
        assert_eq!(drafts[0].requested_lines.len(), 2);
        assert!(plan.stock_plans().is_empty());
    }

    /// 同一依据的不同销售行指定不同目标仓时必须拆分为不同采购单。
    #[test]
    fn purchase_assignments_split_by_target_warehouse() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::Warehouse,
            "8",
            &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &basis_id, SupplySourceType::Purchase, Some("wh-1"), "3"),
            assignment("sol-2", &basis_id, SupplySourceType::Purchase, Some("wh-2"), "2"),
        ])
        .expect("合法集合必须成功");

        let plan = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect("计划必须成功");
        let warehouses = plan
            .purchase_plans()
            .iter()
            .map(|plan| plan.target_warehouse_id.as_ref().map(ToString::to_string))
            .collect::<Vec<_>>();
        assert_eq!(
            warehouses,
            vec![Some("wh-1".to_string()), Some("wh-2".to_string())]
        );
    }

    /// 不同付款条件必须拆分为不同采购单。
    #[test]
    fn purchase_assignments_split_by_scope() {
        let order = sales_order("so-1");
        let first = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let second = basis_group(
            "supplier-1",
            "NET-60",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-2", "10", "2")],
        );
        let first_basis = basis_id_for(&order, &first, "wi-1", None);
        let second_basis = basis_id_for(&order, &second, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &first_basis, SupplySourceType::Purchase, None, "3"),
            assignment("sol-2", &second_basis, SupplySourceType::Purchase, None, "2"),
        ])
        .expect("合法集合必须成功");

        let plan =
            SourcingPlan::plan(&order, &[first, second], &[], "wi-1", &assignments).expect("计划必须成功");
        assert_eq!(plan.purchase_plans().len(), 2);
    }

    /// 选源行依据失效必须失败关闭。
    #[test]
    fn purchase_assignment_with_unknown_basis_fails_closed() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            "unknown-basis",
            SupplySourceType::Purchase,
            None,
            "3",
        )])
        .expect("合法集合必须成功");

        let error =
            SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect_err("失效依据必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 仓库履约必须先选择目标收货仓。
    #[test]
    fn warehouse_fulfillment_requires_target_warehouse() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::Warehouse,
            "8",
            &[("sol-1", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::Purchase,
            None,
            "3",
        )])
        .expect("合法集合必须成功");

        let error =
            SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments).expect_err("缺少目标仓必须失败");
        assert_eq!(
            error,
            SourcingPlanError::WarehouseContract("仓库履约必须先选择目标收货仓".to_string())
        );
    }

    /// 非仓库履约不能指定目标收货仓。
    #[test]
    fn non_warehouse_fulfillment_rejects_target_warehouse() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::Purchase,
            Some("wh-1"),
            "3",
        )])
        .expect("合法集合必须成功");

        let error = SourcingPlan::plan(&order, &[group], &[], "wi-1", &assignments)
            .expect_err("非仓库履约指定目标仓必须失败");
        assert_eq!(
            error,
            SourcingPlanError::WarehouseContract("非仓库履约不能指定目标收货仓".to_string())
        );
    }

    /// 现有库存按余额归组，同余额的多销售行合并为一次预占。
    #[test]
    fn stock_assignments_group_by_balance() {
        let order = sales_order("so-1");
        let group = stock_basis_group(
            "bal-1",
            "wh-1",
            "8",
            &[("sol-1", "10", "2"), ("sol-2", "10", "2")],
        );
        let basis_id = stock_basis_id_for(&order, &group, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &basis_id, SupplySourceType::ExistingStock, None, "3"),
            assignment("sol-2", &basis_id, SupplySourceType::ExistingStock, None, "2"),
        ])
        .expect("合法集合必须成功");

        let plan = SourcingPlan::plan(&order, &[], &[group], "wi-1", &assignments).expect("计划必须成功");
        let stock_plans = plan.stock_plans();
        assert_eq!(stock_plans.len(), 1);
        assert_eq!(stock_plans[0].group.balance.base.id, "bal-1");
        assert_eq!(stock_plans[0].requested_lines.len(), 2);
        assert!(plan.purchase_plans().is_empty());
    }

    /// 不同余额必须拆分，未知余额依据失败关闭。
    #[test]
    fn stock_assignments_split_by_balance_and_reject_unknown() {
        let order = sales_order("so-1");
        let first = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
        let second = stock_basis_group("bal-2", "wh-2", "8", &[("sol-2", "10", "2")]);
        let first_basis = stock_basis_id_for(&order, &first, "wi-1");
        let second_basis = stock_basis_id_for(&order, &second, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &first_basis, SupplySourceType::ExistingStock, None, "3"),
            assignment("sol-2", &second_basis, SupplySourceType::ExistingStock, None, "2"),
        ])
        .expect("合法集合必须成功");

        let plan =
            SourcingPlan::plan(&order, &[], &[first, second], "wi-1", &assignments).expect("计划必须成功");
        assert_eq!(plan.stock_plans().len(), 2);

        let unknown = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            "unknown-balance",
            SupplySourceType::ExistingStock,
            None,
            "3",
        )])
        .expect("合法集合必须成功");
        let error = SourcingPlan::plan(
            &order,
            &[],
            &[stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")])],
            "wi-1",
            &unknown,
        )
        .expect_err("未知余额依据必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 库存与采购合计不得超过同一份最新剩余量。
    #[test]
    fn combined_purchase_and_stock_totals_capped_by_latest_remaining() {
        let order = sales_order("so-1");
        let purchase = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let stock = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
        let purchase_basis = basis_id_for(&order, &purchase, "wi-1", None);
        let stock_basis = stock_basis_id_for(&order, &stock, "wi-1");

        let within = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &purchase_basis, SupplySourceType::Purchase, None, "5"),
            assignment("sol-1", &stock_basis, SupplySourceType::ExistingStock, None, "3"),
        ])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(
            &order,
            std::slice::from_ref(&purchase),
            std::slice::from_ref(&stock),
            "wi-1",
            &within,
        )
        .expect("合计等于剩余量必须成功");
        assert_eq!(plan.purchase_plans().len(), 1);
        assert_eq!(plan.stock_plans().len(), 1);

        let excess = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &purchase_basis, SupplySourceType::Purchase, None, "6"),
            assignment("sol-1", &stock_basis, SupplySourceType::ExistingStock, None, "3"),
        ])
        .expect("合法集合必须成功");
        let error =
            SourcingPlan::plan(&order, &[purchase], &[stock], "wi-1", &excess).expect_err("超量必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 无选源行时计划为空。
    #[test]
    fn plan_without_assignments_is_empty() {
        let order = sales_order("so-1");
        let assignments = SourcingAssignmentSet::normalize(&[]).expect("空集合必须成功");
        let plan = SourcingPlan::plan(&order, &[], &[], "wi-1", &assignments).expect("空计划必须成功");
        assert!(plan.purchase_plans().is_empty());
        assert!(plan.stock_plans().is_empty());
    }

    /// 最新余额重验接受恰好等于行剩余量与余额可用量的分配。
    #[test]
    fn validate_against_latest_stock_accepts_exact_cap() {
        let order = sales_order("so-1");
        let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
        let basis_id = stock_basis_id_for(&order, &group, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::ExistingStock,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
            .expect("计划必须成功");

        plan.validate_against_latest_stock(&[group])
            .expect("恰好等于上限必须成功");
    }

    /// 最新行剩余量下降时重验失败关闭。
    #[test]
    fn validate_against_latest_stock_rejects_line_excess() {
        let order = sales_order("so-1");
        let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
        let basis_id = stock_basis_id_for(&order, &group, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::ExistingStock,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
            .expect("计划必须成功");

        let shrunken = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "3")]);
        let error = plan
            .validate_against_latest_stock(&[shrunken])
            .expect_err("剩余量下降必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 余额可用量下降时重验失败关闭。
    #[test]
    fn validate_against_latest_stock_rejects_balance_excess() {
        let order = sales_order("so-1");
        let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
        let basis_id = stock_basis_id_for(&order, &group, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::ExistingStock,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, &[], std::slice::from_ref(&group), "wi-1", &assignments)
            .expect("计划必须成功");

        let shrunken = stock_basis_group("bal-1", "wh-1", "5", &[("sol-1", "10", "2")]);
        let error = plan
            .validate_against_latest_stock(&[shrunken])
            .expect_err("余额可用量下降必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 最新余额缺失时重验失败关闭。
    #[test]
    fn validate_against_latest_stock_missing_group_fails_closed() {
        let order = sales_order("so-1");
        let group = stock_basis_group("bal-1", "wh-1", "10", &[("sol-1", "10", "2")]);
        let basis_id = stock_basis_id_for(&order, &group, "wi-1");
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::ExistingStock,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, &[], &[group], "wi-1", &assignments).expect("计划必须成功");

        let error = plan
            .validate_against_latest_stock(&[])
            .expect_err("余额失效必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 最新依据重验接受恰好等于剩余量与可供量的拆分。
    #[test]
    fn validate_against_latest_sourcing_accepts_exact_cap() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::Purchase,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, std::slice::from_ref(&group), &[], "wi-1", &assignments)
            .expect("计划必须成功");

        plan.validate_against_latest_sourcing(&[group])
            .expect("恰好等于上限必须成功");
    }

    /// 最新行剩余量下降时重验失败关闭。
    #[test]
    fn validate_against_latest_sourcing_rejects_line_excess() {
        let order = sales_order("so-1");
        let group = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "2")],
        );
        let basis_id = basis_id_for(&order, &group, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[assignment(
            "sol-1",
            &basis_id,
            SupplySourceType::Purchase,
            None,
            "8",
        )])
        .expect("合法集合必须成功");
        let plan = SourcingPlan::plan(&order, std::slice::from_ref(&group), &[], "wi-1", &assignments)
            .expect("计划必须成功");

        let shrunken = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "3")],
        );
        let error = plan
            .validate_against_latest_sourcing(&[shrunken])
            .expect_err("剩余量下降必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 同一供给跨方案共享可用量，累计超量失败关闭。
    #[test]
    fn validate_against_latest_sourcing_shares_supply_cap_across_plans() {
        let order = sales_order("so-1");
        let first = basis_group(
            "supplier-1",
            "NET-30",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-1", "10", "0")],
        );
        let second = basis_group(
            "supplier-1",
            "NET-60",
            FulfillmentResponsibility::SupplierDirect,
            "8",
            &[("sol-2", "10", "0")],
        );
        let first_basis = basis_id_for(&order, &first, "wi-1", None);
        let second_basis = basis_id_for(&order, &second, "wi-1", None);
        let assignments = SourcingAssignmentSet::normalize(&[
            assignment("sol-1", &first_basis, SupplySourceType::Purchase, None, "5"),
            assignment("sol-2", &second_basis, SupplySourceType::Purchase, None, "5"),
        ])
        .expect("合法集合必须成功");
        let plan =
            SourcingPlan::plan(&order, &[first, second], &[], "wi-1", &assignments).expect("计划必须成功");

        let latest = [
            basis_group(
                "supplier-1",
                "NET-30",
                FulfillmentResponsibility::SupplierDirect,
                "8",
                &[("sol-1", "10", "0")],
            ),
            basis_group(
                "supplier-1",
                "NET-60",
                FulfillmentResponsibility::SupplierDirect,
                "8",
                &[("sol-2", "10", "0")],
            ),
        ];
        let error = plan
            .validate_against_latest_sourcing(&latest)
            .expect_err("同一供给跨方案累计超量必须失败");
        assert_eq!(error, SourcingPlanError::StaleFacts);
    }

    /// 相同输入重复构造库存依据 ID 完全一致。
    #[test]
    fn stock_basis_id_is_deterministic() {
        let order = sales_order("so-1");
        let group = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
        assert_eq!(
            stock_basis_id_for(&order, &group, "wi-1"),
            stock_basis_id_for(&order, &group, "wi-1")
        );
    }

    /// 逐行剩余量变化必须改变库存依据 ID。
    #[test]
    fn stock_basis_id_changes_with_quantity() {
        let order = sales_order("so-1");
        let first = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
        let second = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "3")]);
        assert_ne!(
            stock_basis_id_for(&order, &first, "wi-1"),
            stock_basis_id_for(&order, &second, "wi-1")
        );
    }

    /// 余额依据可按稳定销售行查找。
    #[test]
    fn stock_group_line_for_finds_and_misses() {
        let group = stock_basis_group("bal-1", "wh-1", "8", &[("sol-1", "10", "2")]);
        assert!(group.line_for("sol-1").is_some());
        assert!(group.line_for("sol-2").is_none());
    }

    /// 计划错误必须保持稳定文案。
    #[test]
    fn sourcing_plan_error_messages_are_stable() {
        assert_eq!(
            SourcingPlanError::StaleFacts.to_string(),
            "可分配供给数量已更新，请刷新后重试"
        );
        assert_eq!(
            SourcingPlanError::WarehouseContract("仓库履约必须先选择目标收货仓".to_string()).to_string(),
            "仓库履约必须先选择目标收货仓"
        );
    }
}
