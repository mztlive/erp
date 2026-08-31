//! `purchase_line_sales_allocation` 采购行→销售行分配（数据模型 §6.6）。
//!
//! 采购行到销售行的数量归属必须显式分配（§9.2）；入库预占必须沿本分配关系
//! 回到原销售明细，禁止按 SKU 猜测归属（§6.6）。

use std::collections::HashMap;

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use super::purchase_revision::PurchaseOrderRevisionLine;
use super::types::PurchaseLineType;
use crate::errors::{Error, Result};
use crate::ids::{
    PurchaseLineSalesAllocationId, PurchaseOrderRevisionLineId, SalesOrderLineId, SalesOrderRevisionLineId,
};
use crate::money::{Amount, Quantity};

/// 销售当前版本中可供采购分配重绑定的类型化行事实。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSalesAllocationLine {
    /// 销售稳定行身份。
    pub sales_order_line_id: SalesOrderLineId,
    /// 销售当前版本行身份。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
}

/// 采购当前版本销售分配计划。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentSalesAllocationPlan {
    /// 已重绑定到销售当前版本行的采购版本行。
    pub rebound_purchase_lines: Vec<PurchaseOrderRevisionLine>,
    /// 按采购商品/服务行构造的正式分配实体。
    pub allocations: Vec<PurchaseLineSalesAllocation>,
    /// 采购版本行主键到分配主键的履约引用查找表。
    pub allocation_ids_by_purchase_line: HashMap<String, PurchaseLineSalesAllocationId>,
}

/// 构造采购当前版本销售分配计划时的领域错误。
#[derive(Debug, thiserror::Error)]
pub enum CurrentSalesAllocationPlanError {
    /// 商品/服务采购版本行缺少可匹配的销售稳定行。
    #[error("采购版本行缺少销售稳定行")]
    MissingStableSalesLine,
    /// 销售当前版本中不存在采购行引用的稳定行。
    #[error("采购版本行在销售当前版本中没有对应稳定行")]
    MissingCurrentSalesLine,
    /// 商品/服务采购版本行缺少采购数量。
    #[error("采购版本行缺少数量")]
    MissingPurchaseQuantity,
    /// 商品/服务采购版本行缺少正式分配数量。
    #[error("采购版本行缺少分配数量")]
    MissingAllocatedQuantity,
    /// 正式分配数量没有完整覆盖采购数量。
    #[error("采购版本行分配数量必须等于采购数量")]
    QuantityMismatch,
    /// Service 提供的分配主键数量与商品/服务行数量不一致。
    #[error("采购分配标识数量与商品/服务行数量不一致")]
    AllocationIdCountMismatch,
    /// 单行分配实体自身的不变式校验失败。
    #[error(transparent)]
    InvalidAllocation(#[from] Error),
}

/// 采购行→销售行分配创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct PurchaseLineSalesAllocationData {
    /// 采购版本明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 被满足的销售版本明细。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
    /// 分配数量。
    pub allocated_quantity: Quantity,
    /// 分配采购成本（含税）。
    pub allocated_cost_gross: Amount,
    /// 分配采购成本（不含税）。
    pub allocated_cost_net: Amount,
}

/// 采购行→销售行分配实体（数据模型 §6.6）。
///
/// 实体层只做单行内非负与方向校验；「分配数量不超过采购数量」「不超过销售承诺
/// 数量」依赖聚合查询，由 P3 在形成分配的事务内校验（§6.6 必需约束）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct PurchaseLineSalesAllocation {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 采购版本明细。
    pub purchase_order_revision_line_id: PurchaseOrderRevisionLineId,
    /// 被满足的销售版本明细。
    pub sales_order_revision_line_id: SalesOrderRevisionLineId,
    /// 分配数量（非负）。
    pub allocated_quantity: Quantity,
    /// 分配采购成本（含税，非负）。
    pub allocated_cost_gross: Amount,
    /// 分配采购成本（不含税，非负）。
    pub allocated_cost_net: Amount,
}

impl PurchaseLineSalesAllocation {
    /// 创建采购行→销售行分配。
    ///
    /// 校验分配数量、两端成本金额非负，且不含税成本不超过含税成本（方向校验）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::PurchaseLineSalesAllocationId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的分配实体。
    ///
    /// # 错误
    /// 分配数量/成本为负，或不含税成本大于含税成本时返回错误。
    ///
    /// # 说明
    /// 跨行上限校验（不超采购行数量、不超销售承诺数量）依赖聚合查询，留 P3（§6.6）。
    pub fn new(id: PurchaseLineSalesAllocationId, data: PurchaseLineSalesAllocationData) -> Result<Self> {
        if data.allocated_quantity.to_decimal() < rust_decimal::Decimal::ZERO {
            return Err(Error::from("分配数量不能为负"));
        }
        if data.allocated_cost_gross.to_decimal() < rust_decimal::Decimal::ZERO
            || data.allocated_cost_net.to_decimal() < rust_decimal::Decimal::ZERO
        {
            return Err(Error::from("分配成本不能为负"));
        }
        if data.allocated_cost_net > data.allocated_cost_gross {
            return Err(Error::from("不含税分配成本不能超过含税分配成本"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            purchase_order_revision_line_id: data.purchase_order_revision_line_id,
            sales_order_revision_line_id: data.sales_order_revision_line_id,
            allocated_quantity: data.allocated_quantity,
            allocated_cost_gross: data.allocated_cost_gross,
            allocated_cost_net: data.allocated_cost_net,
        })
    }
}

impl CurrentSalesAllocationPlan {
    /// 构造采购版本行到销售当前版本行的确定性正式分配计划。
    ///
    /// # 参数
    /// * `purchase_lines` - 即将成为当前版本的类型化采购版本行
    /// * `current_sales_lines` - 来源销售单当前版本的稳定行与版本行事实
    /// * `allocation_ids` - Service 预先生成且按商品/服务采购行顺序提供的分配主键
    ///
    /// # 返回
    /// 返回重绑定采购行、正式分配实体和采购行到分配主键的查找表。
    ///
    /// # 错误
    /// 稳定行、数量或分配主键缺失，当前销售版本无法匹配，数量不相等，或单行
    /// 分配实体不满足自身不变式时返回错误。
    ///
    /// # 关键约束
    /// 物流费用行保持原样且不消耗分配主键；稳定销售行优先取采购行已有引用，
    /// 缺失时才回退采购确认行引用；输入采购行不会被原地修改。
    pub fn build(
        purchase_lines: &[PurchaseOrderRevisionLine],
        current_sales_lines: &[CurrentSalesAllocationLine],
        allocation_ids: Vec<PurchaseLineSalesAllocationId>,
    ) -> std::result::Result<Self, CurrentSalesAllocationPlanError> {
        let current_sales_by_stable_line = current_sales_lines
            .iter()
            .map(|line| (line.sales_order_line_id.to_string(), line))
            .collect::<HashMap<_, _>>();
        let mut rebound_purchase_lines = purchase_lines.to_vec();
        let mut allocation_ids = allocation_ids.into_iter();
        let mut allocations = Vec::new();
        let mut allocation_ids_by_purchase_line = HashMap::new();
        for line in &mut rebound_purchase_lines {
            if line.line_type != PurchaseLineType::ItemService {
                continue;
            }
            let (allocation_id, allocation) =
                build_current_sales_allocation(line, &current_sales_by_stable_line, &mut allocation_ids)?;
            allocation_ids_by_purchase_line.insert(line.base.id.clone(), allocation_id);
            allocations.push(allocation);
        }
        if allocation_ids.next().is_some() {
            return Err(CurrentSalesAllocationPlanError::AllocationIdCountMismatch);
        }
        Ok(Self {
            rebound_purchase_lines,
            allocations,
            allocation_ids_by_purchase_line,
        })
    }
}

/// 重绑定一个商品/服务采购版本行并构造对应正式分配。
///
/// # 参数
/// * `purchase_line` - 待重绑定的商品/服务采购版本行
/// * `current_sales_by_stable_line` - 销售稳定行到当前版本行事实的查找表
/// * `allocation_ids` - 按采购商品/服务行顺序提供的分配主键迭代器
///
/// # 返回
/// 返回本采购行消费的分配主键和新建正式分配实体。
///
/// # 错误
/// 稳定行、当前销售版本行、数量或分配主键缺失，数量不相等，或分配实体校验
/// 失败时返回错误。
///
/// # 关键约束
/// 校验顺序保持稳定行、当前版本行、采购数量、分配数量、数量相等、主键；成功后
/// 才按销售当前版本事实重写采购行关联。
fn build_current_sales_allocation(
    purchase_line: &mut PurchaseOrderRevisionLine,
    current_sales_by_stable_line: &HashMap<String, &CurrentSalesAllocationLine>,
    allocation_ids: &mut impl Iterator<Item = PurchaseLineSalesAllocationId>,
) -> std::result::Result<
    (PurchaseLineSalesAllocationId, PurchaseLineSalesAllocation),
    CurrentSalesAllocationPlanError,
> {
    let stable_id = purchase_line
        .sales_order_line_id
        .as_ref()
        .map(ToString::to_string)
        .or_else(|| {
            purchase_line
                .procurement_confirmation_line_id
                .as_ref()
                .map(ToString::to_string)
        })
        .ok_or(CurrentSalesAllocationPlanError::MissingStableSalesLine)?;
    let current_sales_line = current_sales_by_stable_line
        .get(&stable_id)
        .ok_or(CurrentSalesAllocationPlanError::MissingCurrentSalesLine)?;
    let quantity = purchase_line
        .quantity
        .ok_or(CurrentSalesAllocationPlanError::MissingPurchaseQuantity)?;
    let allocated = purchase_line
        .allocated_quantity
        .ok_or(CurrentSalesAllocationPlanError::MissingAllocatedQuantity)?;
    if allocated != quantity {
        return Err(CurrentSalesAllocationPlanError::QuantityMismatch);
    }
    let allocation_id = allocation_ids
        .next()
        .ok_or(CurrentSalesAllocationPlanError::AllocationIdCountMismatch)?;
    purchase_line.sales_order_line_id = Some(current_sales_line.sales_order_line_id.clone());
    purchase_line.sales_order_revision_line_id =
        Some(current_sales_line.sales_order_revision_line_id.clone());
    let allocation = PurchaseLineSalesAllocation::new(
        allocation_id.clone(),
        PurchaseLineSalesAllocationData {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new(purchase_line.base.id.clone()),
            sales_order_revision_line_id: current_sales_line.sales_order_revision_line_id.clone(),
            allocated_quantity: allocated,
            allocated_cost_gross: purchase_line.gross_amount,
            allocated_cost_net: purchase_line.net_amount,
        },
    )?;
    Ok((allocation_id, allocation))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::{
        CurrentSalesAllocationLine, CurrentSalesAllocationPlan, CurrentSalesAllocationPlanError,
        PurchaseLineSalesAllocation, PurchaseLineSalesAllocationData,
    };
    use crate::common::time::BusinessDate;
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseLineSalesAllocationId, PurchaseOrderRevisionId,
        PurchaseOrderRevisionLineId, SalesOrderLineId, SalesOrderRevisionLineId, SkuId,
    };
    use crate::money::{line_amounts, Amount, Quantity, Rate, UnitPrice};
    use crate::purchase_order::{PurchaseLineType, PurchaseOrderRevisionLine, PurchaseOrderRevisionLineData};

    /// 构造可被当前销售版本重新绑定的商品/服务采购版本行夹具。
    ///
    /// # 参数
    /// * `id` - 采购版本行稳定身份
    /// * `stable_sales_line_id` - 采购行已有的销售稳定行引用
    /// * `confirmation_line_id` - 可作为兼容回退的采购确认行引用
    /// * `allocated` - 待写入正式分配的数量文本
    ///
    /// # 返回
    /// 返回数量为二、金额合法且携带旧销售版本行引用的采购版本行。
    ///
    /// # 错误
    /// 夹具常量不合法时直接 panic，使测试明确失败。
    ///
    /// # 关键约束
    /// 既有销售稳定行和采购确认行可故意不同，用于验证匹配优先级。
    fn item_purchase_line(
        id: &str,
        stable_sales_line_id: &str,
        confirmation_line_id: &str,
        allocated: &str,
    ) -> PurchaseOrderRevisionLine {
        let quantity = Quantity::from_str("2").unwrap();
        let cost = UnitPrice::from_str("5").unwrap();
        let tax = Rate::from_str("0").unwrap();
        let (gross, net, tax_amount) = line_amounts(cost, quantity, tax);
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new(
                    confirmation_line_id,
                )),
                sku_id: Some(SkuId::new("sku-1")),
                sku_revision_id: None,
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(cost),
                gross_amount: gross,
                net_amount: net,
                tax_amount,
                input_tax_rate: Some(tax),
                expected_delivery_date: Some(BusinessDate::from_ymd(2026, 8, 24).unwrap()),
                sales_order_line_id: Some(SalesOrderLineId::new(stable_sales_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new("old-sorl")),
                allocated_quantity: Some(Quantity::from_str(allocated).unwrap()),
            },
        )
        .unwrap()
    }

    /// 构造不参与销售分配的物流费用采购版本行夹具。
    ///
    /// # 参数
    /// * `id` - 采购版本行稳定身份
    ///
    /// # 返回
    /// 返回金额守恒且不携带任何销售分配字段的物流费用行。
    ///
    /// # 错误
    /// 夹具常量不合法时直接 panic，使测试明确失败。
    ///
    /// # 关键约束
    /// 物流费用行数量、商品字段、销售引用和分配数量全部为空。
    fn logistics_purchase_line(id: &str) -> PurchaseOrderRevisionLine {
        PurchaseOrderRevisionLine::new(
            PurchaseOrderRevisionLineId::new(id),
            PurchaseOrderRevisionLineData {
                purchase_order_revision_id: PurchaseOrderRevisionId::new("por-1"),
                line_no: 2,
                line_type: PurchaseLineType::LogisticsFee,
                procurement_confirmation_line_id: None,
                sku_id: None,
                sku_revision_id: None,
                product_name_snapshot: None,
                specification_snapshot: None,
                quantity: None,
                base_unit_code: None,
                unit_cost_gross: None,
                gross_amount: Amount::from_str("10").unwrap(),
                net_amount: Amount::from_str("10").unwrap(),
                tax_amount: Amount::from_str("0").unwrap(),
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: None,
                sales_order_revision_line_id: None,
                allocated_quantity: None,
            },
        )
        .unwrap()
    }

    /// 构造销售当前版本稳定行与版本行的类型化事实。
    ///
    /// # 参数
    /// * `stable_id` - 销售稳定行身份
    /// * `revision_line_id` - 销售当前版本行身份
    ///
    /// # 返回
    /// 返回可供计划构造器匹配和重绑定的销售行事实。
    ///
    /// # 错误
    /// 本夹具不执行可失败操作。
    ///
    /// # 关键约束
    /// 仅包含分配规则需要的两个稳定身份，不引入销售仓储或其它快照字段。
    fn current_sales_line(stable_id: &str, revision_line_id: &str) -> CurrentSalesAllocationLine {
        CurrentSalesAllocationLine {
            sales_order_line_id: SalesOrderLineId::new(stable_id),
            sales_order_revision_line_id: SalesOrderRevisionLineId::new(revision_line_id),
        }
    }

    fn allocation_data() -> PurchaseLineSalesAllocationData {
        PurchaseLineSalesAllocationData {
            purchase_order_revision_line_id: PurchaseOrderRevisionLineId::new("porl-1"),
            sales_order_revision_line_id: SalesOrderRevisionLineId::new("sorl-1"),
            allocated_quantity: Quantity::from_str("3.000000").unwrap(),
            allocated_cost_gross: Amount::from_str("29.97").unwrap(),
            allocated_cost_net: Amount::from_str("26.07").unwrap(),
        }
    }

    #[test]
    fn allocation_happy_path() {
        let allocation = PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-1"),
            allocation_data(),
        )
        .unwrap();
        assert_eq!(
            allocation.allocated_quantity,
            Quantity::from_str("3.000000").unwrap()
        );
        assert_eq!(
            allocation.allocated_cost_gross,
            Amount::from_str("29.97").unwrap()
        );
    }

    #[test]
    fn allocation_rejects_negative_and_inverted_amounts() {
        let negative_quantity = PurchaseLineSalesAllocationData {
            allocated_quantity: Quantity::from_str("-1.000000").unwrap(),
            ..allocation_data()
        };
        assert!(PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-2"),
            negative_quantity,
        )
        .is_err());

        let inverted = PurchaseLineSalesAllocationData {
            allocated_cost_net: Amount::from_str("30.00").unwrap(),
            ..allocation_data()
        };
        assert!(
            PurchaseLineSalesAllocation::new(PurchaseLineSalesAllocationId::new("alloc-3"), inverted)
                .is_err()
        );

        let negative_cost = PurchaseLineSalesAllocationData {
            allocated_cost_gross: Amount::from_str("-1.00").unwrap(),
            ..allocation_data()
        };
        assert!(PurchaseLineSalesAllocation::new(
            PurchaseLineSalesAllocationId::new("alloc-4"),
            negative_cost
        )
        .is_err());
    }

    /// 验证计划按稳定行优先级重绑定并保持分配主键关联顺序。
    ///
    /// # 参数
    /// 无；测试使用两个采购商品行和两个销售当前版本行事实。
    ///
    /// # 返回
    /// 无；全部重绑定、实体和查找表断言通过即测试成功。
    ///
    /// # 错误
    /// 匹配优先级、输入不可变性、重绑定结果或主键关联漂移时测试失败。
    ///
    /// # 关键约束
    /// 已有销售稳定行优先于采购确认行回退，且输入采购行不被原地修改。
    #[test]
    fn current_sales_plan_rebinds_lines_and_builds_allocations() {
        let direct = item_purchase_line("porl-1", "sol-primary", "sol-fallback", "2");
        let mut fallback = item_purchase_line("porl-2", "sol-old", "sol-fallback", "2");
        fallback.sales_order_line_id = None;
        let purchase_lines = vec![direct, fallback];
        let current_sales_lines = vec![
            current_sales_line("sol-primary", "current-primary"),
            current_sales_line("sol-fallback", "current-fallback"),
        ];

        let plan = CurrentSalesAllocationPlan::build(
            &purchase_lines,
            &current_sales_lines,
            vec![
                PurchaseLineSalesAllocationId::new("alloc-1"),
                PurchaseLineSalesAllocationId::new("alloc-2"),
            ],
        )
        .unwrap();

        assert_eq!(
            purchase_lines[0]
                .sales_order_revision_line_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "old-sorl"
        );
        assert_eq!(
            plan.rebound_purchase_lines[0]
                .sales_order_line_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "sol-primary"
        );
        assert_eq!(
            plan.rebound_purchase_lines[0]
                .sales_order_revision_line_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "current-primary"
        );
        assert_eq!(
            plan.rebound_purchase_lines[1]
                .sales_order_line_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "sol-fallback"
        );
        assert_eq!(
            plan.rebound_purchase_lines[1]
                .sales_order_revision_line_id
                .as_ref()
                .unwrap()
                .as_ref(),
            "current-fallback"
        );
        assert_eq!(plan.allocations[0].base.id, "alloc-1");
        assert_eq!(plan.allocations[1].base.id, "alloc-2");
        assert_eq!(plan.allocation_ids_by_purchase_line["porl-1"].as_ref(), "alloc-1");
        assert_eq!(plan.allocation_ids_by_purchase_line["porl-2"].as_ref(), "alloc-2");
    }

    /// 验证计划按既有优先级拒绝缺失匹配事实和数量不一致。
    ///
    /// # 参数
    /// 无；测试逐项破坏采购行的稳定引用、当前版本匹配和数量字段。
    ///
    /// # 返回
    /// 无；每个输入均返回对应领域错误即测试成功。
    ///
    /// # 错误
    /// 错误优先级或错误变体与既有业务规则不一致时测试失败。
    ///
    /// # 关键约束
    /// 校验顺序固定为稳定行、当前版本行、采购数量、分配数量和数量相等。
    #[test]
    fn current_sales_plan_rejects_missing_facts_and_quantity_mismatch() {
        let current_sales_lines = vec![current_sales_line("sol-1", "current-sorl")];
        let allocation_id = || vec![PurchaseLineSalesAllocationId::new("alloc-1")];

        let mut missing_stable = item_purchase_line("porl-1", "sol-1", "confirm-1", "2");
        missing_stable.sales_order_line_id = None;
        missing_stable.procurement_confirmation_line_id = None;
        assert!(matches!(
            CurrentSalesAllocationPlan::build(&[missing_stable], &current_sales_lines, allocation_id(),),
            Err(CurrentSalesAllocationPlanError::MissingStableSalesLine)
        ));

        let missing_current = item_purchase_line("porl-1", "sol-1", "confirm-1", "2");
        assert!(matches!(
            CurrentSalesAllocationPlan::build(&[missing_current], &[], allocation_id()),
            Err(CurrentSalesAllocationPlanError::MissingCurrentSalesLine)
        ));

        let mut missing_quantity = item_purchase_line("porl-1", "sol-1", "confirm-1", "2");
        missing_quantity.quantity = None;
        assert!(matches!(
            CurrentSalesAllocationPlan::build(&[missing_quantity], &current_sales_lines, allocation_id(),),
            Err(CurrentSalesAllocationPlanError::MissingPurchaseQuantity)
        ));

        let mut missing_allocated = item_purchase_line("porl-1", "sol-1", "confirm-1", "2");
        missing_allocated.allocated_quantity = None;
        assert!(matches!(
            CurrentSalesAllocationPlan::build(&[missing_allocated], &current_sales_lines, allocation_id(),),
            Err(CurrentSalesAllocationPlanError::MissingAllocatedQuantity)
        ));

        let mismatch = item_purchase_line("porl-1", "sol-1", "confirm-1", "1");
        let error = CurrentSalesAllocationPlan::build(&[mismatch], &current_sales_lines, allocation_id())
            .unwrap_err();
        assert!(matches!(error, CurrentSalesAllocationPlanError::QuantityMismatch));
        assert_eq!(error.to_string(), "采购版本行分配数量必须等于采购数量");
    }

    /// 验证物流费用边界和分配主键数量守恒。
    ///
    /// # 参数
    /// 无；测试零商品行、缺少主键和多余主键三种边界。
    ///
    /// # 返回
    /// 无；物流行保持原样且主键数量偏差均被拒绝即测试成功。
    ///
    /// # 错误
    /// 物流行消耗主键、被改写，或主键基数不匹配未失败时测试失败。
    ///
    /// # 关键约束
    /// 每个商品/服务行恰好消费一个 Service 生成的主键，物流费用行消费零个。
    #[test]
    fn current_sales_plan_skips_logistics_and_requires_exact_id_count() {
        let logistics = logistics_purchase_line("porl-logistics");
        let plan =
            CurrentSalesAllocationPlan::build(std::slice::from_ref(&logistics), &[], Vec::new()).unwrap();
        assert_eq!(plan.rebound_purchase_lines, vec![logistics.clone()]);
        assert!(plan.allocations.is_empty());
        assert!(plan.allocation_ids_by_purchase_line.is_empty());

        let item = item_purchase_line("porl-1", "sol-1", "confirm-1", "2");
        let current_sales_lines = vec![current_sales_line("sol-1", "current-sorl")];
        assert!(matches!(
            CurrentSalesAllocationPlan::build(&[item], &current_sales_lines, Vec::new()),
            Err(CurrentSalesAllocationPlanError::AllocationIdCountMismatch)
        ));
        assert!(matches!(
            CurrentSalesAllocationPlan::build(
                &[logistics],
                &[],
                vec![PurchaseLineSalesAllocationId::new("extra-allocation")],
            ),
            Err(CurrentSalesAllocationPlanError::AllocationIdCountMismatch)
        ));
    }
}
