//! 商品 × 支付来源分摊矩阵的纯守恒评估。
//!
//! 本模块只读取商城订单明细、支付来源与分摊事实，不依赖数据库或外部 I/O。
//! 评估结果同时服务于支付事实接收校验和订单详情守恒视图，避免 Service 维护
//! 两份金额折叠规则。

use crate::ids::{MallOrderItemId, MallPaymentSourceId};
use crate::money::Amount;

use super::funding_allocation::MallItemFundingAllocation;
use super::order_item::MallOrderItem;
use super::payment_source::MallPaymentSource;

/// 单个守恒金额的期望值与实际值。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingAmountResult {
    /// 领域快照要求的金额。
    pub expected: Amount,
    /// 从分摊或明细集合折叠得到的金额。
    pub actual: Amount,
}

impl FundingAmountResult {
    /// 判断期望金额与实际金额是否精确一致。
    ///
    /// # 用途
    /// 为命令校验和详情视图提供统一的定点金额比较结果。
    ///
    /// # 参数
    /// * `self` - 待判断的金额结果
    ///
    /// # 返回
    /// 两个 `Amount` 精确相等时返回 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 关键约束
    /// 不做舍入或容差比较，保持既有分摊守恒的精确小数语义。
    pub fn is_valid(&self) -> bool {
        self.expected == self.actual
    }
}

/// 商品明细行的资金分摊守恒结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingItemRowResult {
    /// 商品明细 ID。
    pub item_id: MallOrderItemId,
    /// 来源商品明细身份，用于保持既有命令错误文案。
    pub external_item_id: String,
    /// 明细实付与该行分摊合计的比较结果。
    pub amount: FundingAmountResult,
}

/// 支付来源列的资金分摊守恒结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingSourceColumnResult {
    /// 支付来源 ID。
    pub source_id: MallPaymentSourceId,
    /// 单内来源序号，用于保持既有命令错误文案。
    pub source_no: u32,
    /// 来源金额与该列分摊合计的比较结果。
    pub amount: FundingAmountResult,
}

/// 订单金额快照中参与资金守恒的类型化金额。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingOrderAmounts {
    /// 订单原价。
    pub gross: Amount,
    /// 订单优惠。
    pub discount: Amount,
    /// 订单实付。
    pub paid: Amount,
}

/// 商品明细汇总与订单金额快照的逐项比较结果。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FundingOrderAmountResults {
    /// 明细原价合计与订单原价。
    pub gross: FundingAmountResult,
    /// 明细优惠合计与订单优惠。
    pub discount: FundingAmountResult,
    /// 明细实付合计与订单实付。
    pub paid: FundingAmountResult,
}

/// 资金守恒失败的领域分类。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FundingConservationViolation {
    /// 首个不守恒的商品明细行。
    ItemRow {
        /// 来源商品明细身份。
        external_item_id: String,
    },
    /// 首个不守恒的支付来源列。
    SourceColumn {
        /// 单内支付来源序号。
        source_no: u32,
    },
    /// 商品明细汇总与订单原价、优惠或实付快照不一致。
    OrderAmounts,
    /// 全部分摊合计与订单实付快照不一致。
    OrderPaid,
}

/// 商品 × 支付来源资金矩阵的完整守恒评估。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct FundingConservation {
    /// 按输入商品顺序生成的行结果。
    pub item_rows: Vec<FundingItemRowResult>,
    /// 按输入支付来源顺序生成的列结果。
    pub source_columns: Vec<FundingSourceColumnResult>,
    /// 全部分摊合计与订单实付的结果。
    pub order_paid: FundingAmountResult,
    /// 商品明细汇总与订单金额快照的结果。
    pub order_amounts: FundingOrderAmountResults,
}

impl FundingConservation {
    /// 评估商品 × 支付来源分摊矩阵及订单金额汇总。
    ///
    /// # 用途
    /// 统一计算命令接收校验与订单详情展示所需的资金守恒事实。
    ///
    /// # 参数
    /// * `expected_order` - 订单原价、优惠与实付快照
    /// * `items` - 按业务顺序排列的商城订单明细
    /// * `sources` - 按业务顺序排列的支付来源
    /// * `allocations` - 商品 × 支付来源分摊事实，包含零金额单元格
    ///
    /// # 返回
    /// 返回行、列、订单实付和明细汇总的类型化评估结果。
    ///
    /// # 错误
    /// 不返回错误；规则违反通过 [`Self::ensure_valid`] 转换为领域分类。
    ///
    /// # 关键约束
    /// 按精确 ID 关联且不去重分摊；保留商品与来源输入顺序，并使用精确金额比较。
    pub fn evaluate(
        expected_order: FundingOrderAmounts,
        items: &[MallOrderItem],
        sources: &[MallPaymentSource],
        allocations: &[MallItemFundingAllocation],
    ) -> Self {
        let item_rows = item_row_results(items, allocations);
        let source_columns = source_column_results(sources, allocations);
        let actual_order = aggregate_item_amounts(items);
        Self {
            item_rows,
            source_columns,
            order_paid: FundingAmountResult {
                expected: expected_order.paid,
                actual: allocated_total(allocations, |_| true),
            },
            order_amounts: FundingOrderAmountResults {
                gross: amount_result(expected_order.gross, actual_order.gross),
                discount: amount_result(expected_order.discount, actual_order.discount),
                paid: amount_result(expected_order.paid, actual_order.paid),
            },
        }
    }

    /// 按既有命令失败优先级确认全部守恒结果有效。
    ///
    /// # 用途
    /// 将完整评估压缩为支付事实接收所需的首个领域违规。
    ///
    /// # 参数
    /// * `self` - 已完成的资金守恒评估
    ///
    /// # 返回
    /// 全部结果有效时返回 `Ok(())`。
    ///
    /// # 错误
    /// 依次返回首个商品行、首个来源列、订单金额汇总或订单实付违规。
    ///
    /// # 关键约束
    /// 商品与来源失败顺序跟随输入顺序；订单原价/优惠/实付汇总先于独立分摊总额失败。
    pub fn ensure_valid(&self) -> std::result::Result<(), FundingConservationViolation> {
        if let Some(row) = self.item_rows.iter().find(|row| !row.amount.is_valid()) {
            return Err(FundingConservationViolation::ItemRow {
                external_item_id: row.external_item_id.clone(),
            });
        }
        if let Some(column) = self
            .source_columns
            .iter()
            .find(|column| !column.amount.is_valid())
        {
            return Err(FundingConservationViolation::SourceColumn {
                source_no: column.source_no,
            });
        }
        if !self.order_amounts.gross.is_valid()
            || !self.order_amounts.discount.is_valid()
            || !self.order_amounts.paid.is_valid()
        {
            return Err(FundingConservationViolation::OrderAmounts);
        }
        if !self.order_paid.is_valid() {
            return Err(FundingConservationViolation::OrderPaid);
        }
        Ok(())
    }
}

/// 构造单个金额比较结果。
///
/// # 用途
/// 保持订单金额各分量的结果构造一致。
///
/// # 参数
/// * `expected` - 订单快照金额
/// * `actual` - 明细聚合金额
///
/// # 返回
/// 返回类型化金额结果。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 不做舍入或规范化。
fn amount_result(expected: Amount, actual: Amount) -> FundingAmountResult {
    FundingAmountResult { expected, actual }
}

/// 按谓词折叠分摊金额。
///
/// # 用途
/// 复用商品行、来源列与订单总分摊的精确求和逻辑。
///
/// # 参数
/// * `allocations` - 全部分摊事实
/// * `matches` - 决定当前分摊是否计入的精确关联谓词
///
/// # 返回
/// 返回命中分摊金额的精确合计；无命中时返回零。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 不过滤零金额、不去重记录，并保持 `Amount::checked_add` 的既有精确加法语义。
fn allocated_total(
    allocations: &[MallItemFundingAllocation],
    matches: impl Fn(&MallItemFundingAllocation) -> bool,
) -> Amount {
    allocations
        .iter()
        .filter(|allocation| matches(allocation))
        .fold(zero_amount(), |total, allocation| {
            total.checked_add(allocation.allocated_payment_amount)
        })
}

/// 计算商品明细行结果。
///
/// # 用途
/// 按商品输入顺序比较明细实付与精确 ID 对应的分摊行合计。
///
/// # 参数
/// * `items` - 商城订单明细序列
/// * `allocations` - 全部分摊事实
///
/// # 返回
/// 返回与商品顺序一致的行守恒结果。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 每个商品都生成结果，即使没有分摊或只有零金额分摊。
fn item_row_results(
    items: &[MallOrderItem],
    allocations: &[MallItemFundingAllocation],
) -> Vec<FundingItemRowResult> {
    items
        .iter()
        .map(|item| FundingItemRowResult {
            item_id: MallOrderItemId::new(item.base.id.clone()),
            external_item_id: item.external_item_id.clone(),
            amount: FundingAmountResult {
                expected: item.paid_amount,
                actual: allocated_total(allocations, |allocation| {
                    allocation.mall_order_item_id.as_ref() == item.base.id
                }),
            },
        })
        .collect()
}

/// 计算支付来源列结果。
///
/// # 用途
/// 按来源输入顺序比较来源金额与精确 ID 对应的分摊列合计。
///
/// # 参数
/// * `sources` - 支付来源序列
/// * `allocations` - 全部分摊事实
///
/// # 返回
/// 返回与来源顺序一致的列守恒结果。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 每个来源都生成结果，即使没有分摊或只有零金额分摊。
fn source_column_results(
    sources: &[MallPaymentSource],
    allocations: &[MallItemFundingAllocation],
) -> Vec<FundingSourceColumnResult> {
    sources
        .iter()
        .map(|source| FundingSourceColumnResult {
            source_id: MallPaymentSourceId::new(source.base.id.clone()),
            source_no: source.source_no,
            amount: FundingAmountResult {
                expected: source.amount,
                actual: allocated_total(allocations, |allocation| {
                    allocation.mall_payment_source_id.as_ref() == source.base.id
                }),
            },
        })
        .collect()
}

/// 汇总商品明细中参与订单守恒的金额。
///
/// # 用途
/// 计算既有命令校验使用的原价、优惠与实付明细合计。
///
/// # 参数
/// * `items` - 商城订单明细序列
///
/// # 返回
/// 返回明细原价、优惠与实付合计。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 仅汇总既有规则覆盖的三个金额分量，不提前改变运费错误的外部失败语义。
fn aggregate_item_amounts(items: &[MallOrderItem]) -> FundingOrderAmounts {
    items.iter().fold(
        FundingOrderAmounts {
            gross: zero_amount(),
            discount: zero_amount(),
            paid: zero_amount(),
        },
        |total, item| FundingOrderAmounts {
            gross: total.gross.checked_add(item.line_gross_amount),
            discount: total.discount.checked_add(item.allocated_discount_amount),
            paid: total.paid.checked_add(item.paid_amount),
        },
    )
}

/// 返回资金守恒折叠使用的零金额。
///
/// # 用途
/// 提供不经字符串解析的确定性求和起点。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回精确 `0.00` 金额。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 零值必须满足 `Amount` 两位小数约束。
fn zero_amount() -> Amount {
    Amount::try_from(rust_decimal::Decimal::new(0, 2)).expect("零金额必须合法")
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::ids::{MallItemFundingAllocationId, MallOrderId, MallOrderItemId, MallPaymentSourceId};
    use crate::mall_order::funding_allocation::{MallItemFundingAllocation, MallItemFundingAllocationData};
    use crate::mall_order::order_item::{MallOrderItem, MallOrderItemData};
    use crate::mall_order::payment_source::{MallPaymentSource, MallPaymentSourceData, PaymentSourceType};
    use crate::mall_order::types::AttributionStatus;
    use crate::money::{Amount, Quantity, Rate, UnitPrice};

    use super::{FundingConservation, FundingConservationViolation, FundingOrderAmounts};

    /// 解析测试金额并在非法固定样例时立即失败。
    ///
    /// 参数为小数字符串，返回 `Amount`；不执行任何外部 I/O。
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    /// 构造守恒测试使用的最小商城订单明细。
    ///
    /// 参数指定稳定 ID 与实付金额，返回通过实体校验的单数量明细。
    fn item(id: &str, paid: &str) -> MallOrderItem {
        MallOrderItem::new(
            MallOrderItemId::new(id),
            MallOrderItemData {
                mall_order_id: MallOrderId::new("order-1"),
                external_item_id: format!("line-{id}"),
                sku_id: None,
                product_publication_revision_id: None,
                supplier_offering_revision_id: None,
                name_snapshot: format!("item-{id}"),
                spec_snapshot: None,
                quantity: Quantity::from_str("1.000000").unwrap(),
                unit_price_gross: UnitPrice::from_str(paid).unwrap(),
                line_gross_amount: amount(paid),
                allocated_discount_amount: amount("0.00"),
                allocated_freight_amount: amount("0.00"),
                paid_amount: amount(paid),
                sales_tax_rate: Rate::from_str("0.000000").unwrap(),
                unit_cost_snapshot: None,
                cost_snapshot_total: None,
                cost_tax_inclusion: None,
                cost_input_tax_rate: None,
            },
        )
        .unwrap()
    }

    /// 构造守恒测试使用的最小支付来源。
    ///
    /// 参数指定稳定 ID、来源序号和金额，返回已归集的微信支付来源。
    fn source(id: &str, source_no: u32, value: &str) -> MallPaymentSource {
        MallPaymentSource::new(
            MallPaymentSourceId::new(id),
            MallPaymentSourceData {
                mall_order_id: MallOrderId::new("order-1"),
                source_no,
                source_type: PaymentSourceType::Wechat,
                amount: amount(value),
                source_card_instance_ref: None,
                mall_card_instance_id: None,
                wechat_payment_ref: Some(format!("wx-{id}")),
                attribution_status: AttributionStatus::Attributed,
            },
        )
        .unwrap()
    }

    /// 构造守恒测试使用的商品与支付来源分摊事实。
    ///
    /// 参数指定三方稳定身份和金额，返回通过实体校验的分摊记录。
    fn allocation(id: &str, item_id: &str, source_id: &str, value: &str) -> MallItemFundingAllocation {
        MallItemFundingAllocation::new(
            MallItemFundingAllocationId::new(id),
            MallItemFundingAllocationData {
                mall_order_item_id: MallOrderItemId::new(item_id),
                mall_payment_source_id: MallPaymentSourceId::new(source_id),
                allocated_payment_amount: amount(value),
            },
        )
        .unwrap()
    }

    /// 构造行列与订单金额全部守恒的二乘二测试图。
    ///
    /// 返回商品、来源和分摊三个有序集合，供正常与违规测试复用。
    fn normal_graph() -> (
        Vec<MallOrderItem>,
        Vec<MallPaymentSource>,
        Vec<MallItemFundingAllocation>,
    ) {
        (
            vec![item("item-1", "60.00"), item("item-2", "40.00")],
            vec![source("source-1", 1, "70.00"), source("source-2", 2, "30.00")],
            vec![
                allocation("a-1", "item-1", "source-1", "50.00"),
                allocation("a-2", "item-1", "source-2", "10.00"),
                allocation("a-3", "item-2", "source-1", "20.00"),
                allocation("a-4", "item-2", "source-2", "20.00"),
            ],
        )
    }

    /// 构造测试使用的订单金额快照。
    ///
    /// 参数依次指定原价、优惠和实付，返回对应的类型化金额集合。
    fn order_amounts(gross: &str, discount: &str, paid: &str) -> FundingOrderAmounts {
        FundingOrderAmounts {
            gross: amount(gross),
            discount: amount(discount),
            paid: amount(paid),
        }
    }

    /// 正常路径：行、列、订单实付和订单金额汇总全部守恒，并保持输入顺序。
    ///
    /// 测试使用固定二乘二矩阵，任一结果分类或顺序漂移时失败。
    #[test]
    fn evaluates_valid_matrix_and_typed_totals() {
        let (items, sources, allocations) = normal_graph();
        let result = FundingConservation::evaluate(
            order_amounts("100.00", "0.00", "100.00"),
            &items,
            &sources,
            &allocations,
        );

        assert!(result.ensure_valid().is_ok());
        assert_eq!(result.item_rows[0].item_id, MallOrderItemId::new("item-1"));
        assert_eq!(result.item_rows[1].item_id, MallOrderItemId::new("item-2"));
        assert_eq!(
            result.source_columns[0].source_id,
            MallPaymentSourceId::new("source-1")
        );
        assert!(result.order_paid.is_valid());
        assert!(result.order_amounts.gross.is_valid());
        assert!(result.order_amounts.discount.is_valid());
        assert!(result.order_amounts.paid.is_valid());
    }

    /// 失败路径：保持商品行、来源列、订单汇总的既有首错优先级和身份信息。
    ///
    /// 测试依次制造三类违规，不访问数据库，错误分类或身份变化时失败。
    #[test]
    fn reports_item_source_and_order_amount_failures_in_precedence_order() {
        let (items, sources, mut allocations) = normal_graph();
        allocations[1] = allocation("a-2", "item-1", "source-2", "9.00");
        let item_failure = FundingConservation::evaluate(
            order_amounts("100.00", "0.00", "100.00"),
            &items,
            &sources,
            &allocations,
        );
        assert_eq!(
            item_failure.ensure_valid(),
            Err(FundingConservationViolation::ItemRow {
                external_item_id: "line-item-1".to_string(),
            })
        );

        let (_, _, allocations) = normal_graph();
        let mismatched_sources = vec![source("source-1", 1, "69.00"), source("source-2", 2, "31.00")];
        let source_failure = FundingConservation::evaluate(
            order_amounts("100.00", "0.00", "100.00"),
            &items,
            &mismatched_sources,
            &allocations,
        );
        assert_eq!(
            source_failure.ensure_valid(),
            Err(FundingConservationViolation::SourceColumn { source_no: 1 })
        );

        let aggregate_failure = FundingConservation::evaluate(
            order_amounts("99.00", "0.00", "100.00"),
            &items,
            &sources,
            &allocations,
        );
        assert_eq!(
            aggregate_failure.ensure_valid(),
            Err(FundingConservationViolation::OrderAmounts)
        );
    }

    /// 边界路径：显式零金额矩阵单元格参与折叠，但不破坏行列守恒或结果完整性。
    ///
    /// 测试固定零单元格仍保留全部行列，并断言关键实际金额不变。
    #[test]
    fn accepts_zero_allocations_without_dropping_rows_or_columns() {
        let items = vec![item("item-1", "60.00"), item("item-2", "40.00")];
        let sources = vec![source("source-1", 1, "60.00"), source("source-2", 2, "40.00")];
        let allocations = vec![
            allocation("a-1", "item-1", "source-1", "60.00"),
            allocation("a-2", "item-1", "source-2", "0.00"),
            allocation("a-3", "item-2", "source-1", "0.00"),
            allocation("a-4", "item-2", "source-2", "40.00"),
        ];

        let result = FundingConservation::evaluate(
            order_amounts("100.00", "0.00", "100.00"),
            &items,
            &sources,
            &allocations,
        );

        assert!(result.ensure_valid().is_ok());
        assert_eq!(result.item_rows.len(), 2);
        assert_eq!(result.source_columns.len(), 2);
        assert_eq!(result.item_rows[0].amount.actual, amount("60.00"));
        assert_eq!(result.source_columns[1].amount.actual, amount("40.00"));
    }

    /// 边界路径：空折叠和零小数位分配都保持两位小数的展示起点。
    ///
    /// 测试直接断言实体结果的字符串表示，防止详情响应重新退化为零位小数。
    #[test]
    fn preserves_two_decimal_scale_for_empty_and_scale_zero_folds() {
        let empty = FundingConservation::evaluate(order_amounts("0.00", "0.00", "0.00"), &[], &[], &[]);
        assert_eq!(empty.order_paid.actual.to_string(), "0.00");
        assert_eq!(empty.order_amounts.gross.actual.to_string(), "0.00");
        assert_eq!(empty.order_amounts.discount.actual.to_string(), "0.00");
        assert_eq!(empty.order_amounts.paid.actual.to_string(), "0.00");

        let result = FundingConservation::evaluate(
            order_amounts("100.00", "0.00", "100.00"),
            &[item("item-1", "60.00"), item("item-2", "40.00")],
            &[source("source-1", 1, "60.00"), source("source-2", 2, "40.00")],
            &[allocation("a-1", "item-1", "source-1", "60")],
        );
        assert_eq!(result.item_rows[1].amount.actual.to_string(), "0.00");
        assert_eq!(result.source_columns[1].amount.actual.to_string(), "0.00");
    }
}
