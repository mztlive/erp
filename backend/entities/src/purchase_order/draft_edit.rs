//! 采购草稿行编辑校验领域规则。
//!
//! 草稿保存与提交只允许调整原销售来源行的可编辑字段：补丁路径由 DTO 把客户端
//! 可编辑字段（数量、含税单价、进项税率）合并到服务端当前草稿冻结字段，本模块
//! 负责来源行集合与冻结引用不变式、数量/分配一致性、最新覆盖上限及付款条件
//! 不可变等无 I/O 规则。最新覆盖行由 Service 作为事实传入
//! （`&[SalesProcurementCoverageLine]`），本模块不读取任何持久化数据。

use std::collections::{HashMap, HashSet};
use std::str::FromStr;

use rust_decimal::Decimal;

use crate::money::Quantity;

use super::coverage::SalesProcurementCoverageLine;
use super::purchase_submission::PurchaseOrderSubmissionLine;
use super::types::PurchaseLineType;

/// 客户端草稿行编辑请求（DTO 由 `SavePurchaseOrderLine` 生成）。
///
/// 字符串字段保持客户端原始形态；规范化、类型化与校验统一由
/// [`validate_draft_line_edits`] 在稳定顺序中完成，保证补丁与完整行两条路径
/// 的错误边界一致。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DraftLineEdit {
    /// 行类型；草稿编辑不允许修改。
    pub line_type: PurchaseLineType,
    /// 商品/服务采购数量。
    pub quantity: Option<String>,
    /// 销售分配数量。
    pub allocated_quantity: Option<String>,
    /// 商品/服务行对应的采购二次确认分行。
    pub procurement_confirmation_line_id: Option<String>,
    /// 商品行引用的 SKU。
    pub sku_id: Option<String>,
    /// 商品行引用的 SKU 版本。
    pub sku_revision_id: Option<String>,
    /// 商品行对应的销售稳定行。
    pub sales_order_line_id: Option<String>,
    /// 商品行对应的销售当前版本行。
    pub sales_order_revision_line_id: Option<String>,
    /// 商品行对应的历史销售提交行。
    pub sales_order_submission_line_id: Option<String>,
}

/// 采购草稿编辑校验失败原因。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum DraftLineEditViolation {
    /// 不能新增或删除销售来源行。
    #[error("采购草稿不能新增或删除销售来源行")]
    SourceLineCountChanged,
    /// 同一销售来源行出现多次。
    #[error("采购草稿包含重复销售来源行")]
    DuplicateSalesLine,
    /// 把行挂到其它销售来源行。
    #[error("采购草稿不能改写销售来源行")]
    RewrittenSalesLine,
    /// 改写销售或商品来源引用。
    #[error("采购草稿不能改写销售或商品来源引用")]
    RewrittenSourceReference,
    /// 当前草稿商品行缺少销售稳定行。
    #[error("采购草稿行缺少销售稳定行")]
    MissingSalesStableLine,
    /// 当前草稿商品行缺少原分配数量。
    #[error("采购草稿行缺少原分配数量")]
    MissingOriginalAllocatedQuantity,
    /// 请求行缺少销售来源行。
    #[error("销售来源行不能为空")]
    MissingSalesLineId,
    /// 商品行缺少采购数量。
    #[error("采购数量不能为空")]
    MissingQuantity,
    /// 商品行缺少销售分配数量。
    #[error("销售分配数量不能为空")]
    MissingAllocatedQuantity,
    /// 数量非法或非正。
    #[error("{0}")]
    InvalidQuantity(String),
    /// 销售分配数量与采购数量不一致。
    #[error("销售分配数量必须等于采购数量")]
    QuantityAllocationMismatch,
    /// 销售当前版本已移除采购来源行。
    #[error("销售当前版本已移除采购来源行，请刷新后重试")]
    SourceLineRemoved,
    /// 新数量超过最新可采购数量。
    #[error("可采购数量已更新，请刷新后重试")]
    ExceedsAvailableQuantity,
    /// 创建依据冻结的付款条件被改写。
    #[error("采购草稿创建后的付款条件不可修改")]
    PaymentTermChanged,
}

/// 校验采购草稿只调整原销售来源行数量且不会造成超采。
///
/// # 参数
/// * `requested` - 客户端编辑后的草稿行（补丁路径已由 DTO 与当前草稿合并）
/// * `existing` - 当前草稿不可变来源行
/// * `coverage_lines` - Service 传入的推进销售 guard 后的最新采购覆盖行
///
/// # 返回
/// 来源行集合、引用、数量与覆盖上限全部有效时返回 `Ok(())`。
///
/// # 错误
/// 来源行增删或改写、数量与分配不一致、销售来源行缺失、当前销售行已移除、
/// 新数量超过 `当前剩余 + 本采购单原占用` 或草稿行缺少稳定身份时返回对应
/// [`DraftLineEditViolation`]。
///
/// # 关键业务约束
/// 当前覆盖包含本采购单旧草稿，因此编辑上限需加回本单原占用后再比较；并发
/// 覆盖变化必须由调用方在事务内传入最新覆盖重新校验，返回可刷新冲突而非
/// 静默截断。
pub fn validate_draft_line_edits(
    requested: &[DraftLineEdit],
    existing: &[PurchaseOrderSubmissionLine],
    coverage_lines: &[SalesProcurementCoverageLine],
) -> Result<(), DraftLineEditViolation> {
    let existing = existing
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .map(|line| {
            let line_id = line
                .sales_order_line_id
                .as_ref()
                .map(ToString::to_string)
                .ok_or(DraftLineEditViolation::MissingSalesStableLine)?;
            Ok((line_id, line))
        })
        .collect::<Result<HashMap<_, _>, _>>()?;
    let coverage = coverage_lines
        .iter()
        .map(|line| (line.revision_line.sales_order_line_id.to_string(), line))
        .collect::<HashMap<_, _>>();
    let requested_items = requested
        .iter()
        .filter(|line| line.line_type == PurchaseLineType::ItemService)
        .collect::<Vec<_>>();
    if requested_items.len() != existing.len() {
        return Err(DraftLineEditViolation::SourceLineCountChanged);
    }

    let mut seen = HashSet::new();
    for requested_line in requested_items {
        let stable_id = normalized_line_id(requested_line.sales_order_line_id.as_deref())?;
        if !seen.insert(stable_id.clone()) {
            return Err(DraftLineEditViolation::DuplicateSalesLine);
        }
        let old_line = existing
            .get(&stable_id)
            .ok_or(DraftLineEditViolation::RewrittenSalesLine)?;
        old_line.ensure_source_references_unchanged(requested_line)?;

        let quantity = parse_required_quantity(
            requested_line.quantity.as_deref(),
            DraftLineEditViolation::MissingQuantity,
        )?;
        let allocated = parse_required_quantity(
            requested_line.allocated_quantity.as_deref(),
            DraftLineEditViolation::MissingAllocatedQuantity,
        )?;
        if quantity != allocated {
            return Err(DraftLineEditViolation::QuantityAllocationMismatch);
        }
        let old_allocated = old_line
            .allocated_quantity
            .ok_or(DraftLineEditViolation::MissingOriginalAllocatedQuantity)?;
        let current = coverage
            .get(&stable_id)
            .ok_or(DraftLineEditViolation::SourceLineRemoved)?;
        let allowed = current.summary.remaining_quantity.to_decimal() + old_allocated.to_decimal();
        if quantity.to_decimal() > allowed {
            return Err(DraftLineEditViolation::ExceedsAvailableQuantity);
        }
    }
    Ok(())
}

/// 规范化必填销售稳定行 ID。
///
/// # 参数
/// * `value` - 请求中的可选 ID
///
/// # 返回
/// 返回去除首尾空白的稳定行 ID。
///
/// # 错误
/// ID 缺失或为空时返回 [`DraftLineEditViolation::MissingSalesLineId`]。
fn normalized_line_id(value: Option<&str>) -> Result<String, DraftLineEditViolation> {
    normalized_optional_id(value).ok_or(DraftLineEditViolation::MissingSalesLineId)
}

/// 规范化可选引用 ID：空白视为缺失。
///
/// # 参数
/// * `value` - 可选原始 ID
///
/// # 返回
/// 空值或空白返回 `None`，否则返回去除首尾空白的字符串。
///
/// # 错误
/// 无。
pub(super) fn normalized_optional_id(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
}

/// 解析必填正数量。
///
/// # 参数
/// * `value` - 数量文本
/// * `missing` - 缺失时的失败原因
///
/// # 返回
/// 返回领域数量。
///
/// # 错误
/// 缺失、格式非法或非正时返回对应 [`DraftLineEditViolation`]。
fn parse_required_quantity(
    value: Option<&str>,
    missing: DraftLineEditViolation,
) -> Result<Quantity, DraftLineEditViolation> {
    let value = value.ok_or(missing)?;
    let quantity = Quantity::from_str(value.trim())
        .map_err(|error| DraftLineEditViolation::InvalidQuantity(format!("数量非法: {error}")))?;
    if quantity.to_decimal() <= Decimal::ZERO {
        return Err(DraftLineEditViolation::InvalidQuantity(
            "数量必须大于0".to_string(),
        ));
    }
    Ok(quantity)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use crate::catalog::ProductKind;
    use crate::common::time::Instant;
    use crate::ids::{
        ProcurementConfirmationLineId, PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId,
        SalesOrderLineId, SalesOrderRevisionId, SalesOrderRevisionLineId, SalesOrderSubmissionLineId, SkuId,
        SkuRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::purchase_order::coverage::SalesProcurementCoverageLine;
    use crate::purchase_order::purchase_submission::{
        PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData,
    };
    use crate::purchase_order::PurchaseLineType;
    use crate::sales_order::revision::{SalesOrderGoodsServiceLineRevision, SalesOrderRevisionLine};
    use crate::sales_order::{LineType, ProcurementCoverageSummary};

    use super::{validate_draft_line_edits, DraftLineEdit, DraftLineEditViolation};

    /// 构造草稿商品行。
    fn submission_line(
        id: &str,
        stable_line_id: &str,
        allocated: &str,
        sku_id: &str,
        revision_line_id: &str,
        submission_line_id: Option<&str>,
    ) -> PurchaseOrderSubmissionLine {
        let quantity = Quantity::from_str(allocated).unwrap();
        let (gross, net, tax) = crate::money::line_amounts(
            UnitPrice::from_str("5").unwrap(),
            quantity,
            Rate::from_str("0").unwrap(),
        );
        let line = PurchaseOrderSubmissionLine::new(
            PurchaseOrderSubmissionLineId::new(id),
            PurchaseOrderSubmissionLineData {
                purchase_order_submission_id: PurchaseOrderSubmissionId::new("sub-1"),
                line_no: 1,
                line_type: PurchaseLineType::ItemService,
                procurement_confirmation_line_id: Some(ProcurementConfirmationLineId::new("pcl-1")),
                sku_id: Some(SkuId::new(sku_id)),
                sku_revision_id: Some(SkuRevisionId::new(format!("skur-{sku_id}"))),
                product_name_snapshot: Some("商品".to_string()),
                specification_snapshot: Some("规格".to_string()),
                quantity: Some(quantity),
                base_unit_code: Some("件".to_string()),
                unit_cost_gross: Some(UnitPrice::from_str("5").unwrap()),
                gross_amount: gross,
                net_amount: net,
                tax_amount: tax,
                input_tax_rate: Some(Rate::from_str("0").unwrap()),
                expected_delivery_date: None,
                sales_order_line_id: Some(SalesOrderLineId::new(stable_line_id)),
                sales_order_revision_line_id: Some(SalesOrderRevisionLineId::new(revision_line_id)),
                sales_order_submission_line_id: submission_line_id.map(SalesOrderSubmissionLineId::new),
                allocated_quantity: Some(quantity),
            },
        )
        .unwrap();
        line
    }

    /// 构造当前草稿商品行（引用齐全）。
    fn draft_line(
        stable_line_id: &str,
        allocated: &str,
        sku_id: &str,
        revision_line_id: &str,
        submission_line_id: Option<&str>,
    ) -> PurchaseOrderSubmissionLine {
        submission_line(
            format!("subl-{stable_line_id}").as_str(),
            stable_line_id,
            allocated,
            sku_id,
            revision_line_id,
            submission_line_id,
        )
    }

    /// 构造一条最新覆盖行，剩余量为 `remaining`。
    fn coverage_line(stable_line_id: &str, remaining: &str) -> SalesProcurementCoverageLine {
        let total = Quantity::from_str("10").unwrap();
        let remaining = Quantity::from_str(remaining).unwrap();
        let covered = Quantity::try_from(total.to_decimal() - remaining.to_decimal()).expect("剩余量合法");
        SalesProcurementCoverageLine {
            revision_line: SalesOrderRevisionLine {
                base: entity_core::BaseModel::new(format!("sorl-{stable_line_id}")),
                sales_order_revision_id: SalesOrderRevisionId::new("rev-1"),
                sales_order_line_id: SalesOrderLineId::new(stable_line_id),
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
            goods_line: SalesOrderGoodsServiceLineRevision {
                base: entity_core::BaseModel::new(format!("goods-{stable_line_id}")),
                revision_line_id: SalesOrderRevisionLineId::new(format!("sorl-{stable_line_id}")),
                sku_id: SkuId::new("sku-1"),
                sku_revision_id: SkuRevisionId::new("skur-1"),
                welfare_scenario: None,
                service_region: None,
                fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
                quantity: total,
                base_unit_code: "件".to_string(),
                unit_price_gross: UnitPrice::from_str("5").unwrap(),
            },
            product_kind: ProductKind::Physical,
            summary: ProcurementCoverageSummary::new(total, covered).unwrap(),
        }
    }

    /// 构造与现有草稿行一致的编辑请求。
    fn edit(
        stable_line_id: &str,
        quantity: Option<&str>,
        allocated: Option<&str>,
        sku_id: &str,
        revision_line_id: &str,
        submission_line_id: Option<&str>,
    ) -> DraftLineEdit {
        DraftLineEdit {
            line_type: PurchaseLineType::ItemService,
            quantity: quantity.map(str::to_string),
            allocated_quantity: allocated.map(str::to_string),
            procurement_confirmation_line_id: Some("pcl-1".to_string()),
            sku_id: Some(sku_id.to_string()),
            sku_revision_id: Some(format!("skur-{sku_id}")),
            sales_order_line_id: Some(stable_line_id.to_string()),
            sales_order_revision_line_id: Some(revision_line_id.to_string()),
            sales_order_submission_line_id: submission_line_id.map(str::to_string),
        }
    }

    /// 构造默认通过校验的编辑请求：剩余 3 + 原占用 2，数量 5 恰在边界。
    fn valid_edit() -> DraftLineEdit {
        edit("sol-1", Some("5"), Some("5"), "sku-1", "sorl-1", Some("sosl-1"))
    }

    /// 零、部分、完整覆盖边界：等于 `剩余 + 原占用` 允许，超过拒绝。
    #[test]
    fn quantity_within_latest_available_is_accepted_at_boundary() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        for quantity in ["0.01", "3", "5"] {
            let requested = vec![edit(
                "sol-1",
                Some(quantity),
                Some(quantity),
                "sku-1",
                "sorl-1",
                Some("sosl-1"),
            )];
            assert!(validate_draft_line_edits(&requested, &existing, &coverage).is_ok());
        }
    }

    /// 超过 `剩余 + 原占用` 必须返回可刷新冲突。
    #[test]
    fn exceeding_available_quantity_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![edit(
            "sol-1",
            Some("5.01"),
            Some("5.01"),
            "sku-1",
            "sorl-1",
            Some("sosl-1"),
        )];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::ExceedsAvailableQuantity)
        );
    }

    /// 同一销售来源行出现多次必须拒绝。
    #[test]
    fn duplicate_sales_source_lines_are_rejected() {
        let existing = vec![
            draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1")),
            draft_line("sol-2", "1", "sku-2", "sorl-2", None),
        ];
        let coverage = vec![coverage_line("sol-1", "3"), coverage_line("sol-2", "0")];
        let requested = vec![valid_edit(), valid_edit()];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::DuplicateSalesLine)
        );
    }

    /// 新增或删除销售来源行必须拒绝。
    #[test]
    fn source_line_count_changes_are_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![
            valid_edit(),
            edit("sol-2", Some("1"), Some("1"), "sku-2", "sorl-2", None),
        ];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::SourceLineCountChanged)
        );
    }

    /// 把行挂到不存在的销售来源行必须拒绝。
    #[test]
    fn rewriting_sales_source_line_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![edit(
            "sol-9",
            Some("5"),
            Some("5"),
            "sku-1",
            "sorl-1",
            Some("sosl-1"),
        )];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::RewrittenSalesLine)
        );
    }

    /// 任一冻结来源引用被改写都必须拒绝。
    #[test]
    fn rewriting_frozen_source_references_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let cases = [
            edit("sol-1", Some("5"), Some("5"), "sku-9", "sorl-1", Some("sosl-1")),
            edit("sol-1", Some("5"), Some("5"), "sku-1", "sorl-9", Some("sosl-1")),
            edit("sol-1", Some("5"), Some("5"), "sku-1", "sorl-1", None),
        ];
        for requested in cases {
            assert_eq!(
                validate_draft_line_edits(&[requested], &existing, &coverage),
                Err(DraftLineEditViolation::RewrittenSourceReference)
            );
        }
    }

    /// 采购数量与销售分配数量必须一致。
    #[test]
    fn quantity_allocation_mismatch_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![edit(
            "sol-1",
            Some("4"),
            Some("5"),
            "sku-1",
            "sorl-1",
            Some("sosl-1"),
        )];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::QuantityAllocationMismatch)
        );
    }

    /// 商品行缺少采购数量或分配数量必须拒绝。
    #[test]
    fn missing_quantities_are_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![edit("sol-1", None, Some("5"), "sku-1", "sorl-1", Some("sosl-1"))];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::MissingQuantity)
        );
        let requested = vec![edit("sol-1", Some("5"), None, "sku-1", "sorl-1", Some("sosl-1"))];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::MissingAllocatedQuantity)
        );
    }

    /// 空白或缺失销售来源行必须拒绝。
    #[test]
    fn blank_sales_line_id_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let mut requested = valid_edit();
        requested.sales_order_line_id = Some("  ".to_string());
        assert_eq!(
            validate_draft_line_edits(&[requested], &existing, &coverage),
            Err(DraftLineEditViolation::MissingSalesLineId)
        );
    }

    /// 数量非正或格式非法必须拒绝且保留文案。
    #[test]
    fn invalid_quantities_are_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let requested = vec![edit(
            "sol-1",
            Some("0"),
            Some("0"),
            "sku-1",
            "sorl-1",
            Some("sosl-1"),
        )];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &coverage),
            Err(DraftLineEditViolation::InvalidQuantity(
                "数量必须大于0".to_string()
            ))
        );
        let requested = vec![edit(
            "sol-1",
            Some("abc"),
            Some("abc"),
            "sku-1",
            "sorl-1",
            Some("sosl-1"),
        )];
        let error = validate_draft_line_edits(&requested, &existing, &coverage).unwrap_err();
        assert!(matches!(error, DraftLineEditViolation::InvalidQuantity(_)));
        assert!(error.to_string().contains("数量非法"));
    }

    /// 销售当前版本已移除采购来源行必须返回可刷新冲突。
    #[test]
    fn removed_sales_source_line_is_rejected() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let requested = vec![valid_edit()];
        assert_eq!(
            validate_draft_line_edits(&requested, &existing, &[]),
            Err(DraftLineEditViolation::SourceLineRemoved)
        );
    }

    /// 物流费用行不参与销售来源与覆盖校验。
    #[test]
    fn logistics_lines_are_ignored() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let mut logistics = valid_edit();
        logistics.line_type = PurchaseLineType::LogisticsFee;
        logistics.quantity = Some("not-a-number".to_string());
        logistics.sales_order_line_id = Some("sol-99".to_string());
        let requested = vec![valid_edit(), logistics];
        assert!(validate_draft_line_edits(&requested, &existing, &coverage).is_ok());
    }

    /// 首尾空白在比较前规范化，与既有校验语义一致。
    #[test]
    fn whitespace_padded_ids_and_quantities_are_normalized() {
        let existing = vec![draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"))];
        let coverage = vec![coverage_line("sol-1", "3")];
        let mut requested = valid_edit();
        requested.sales_order_line_id = Some(" sol-1 ".to_string());
        requested.quantity = Some(" 5 ".to_string());
        requested.allocated_quantity = Some("5".to_string());
        assert!(validate_draft_line_edits(&[requested], &existing, &coverage).is_ok());
    }

    /// 草稿行缺少销售稳定行或原分配数量按一致性错误失败。
    #[test]
    fn inconsistent_existing_lines_are_rejected() {
        let coverage = vec![coverage_line("sol-1", "3")];
        let mut no_stable = draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"));
        no_stable.sales_order_line_id = None;
        let requested = vec![valid_edit()];
        assert_eq!(
            validate_draft_line_edits(&requested, &[no_stable], &coverage),
            Err(DraftLineEditViolation::MissingSalesStableLine)
        );
        let mut no_allocated = draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1"));
        no_allocated.allocated_quantity = None;
        assert_eq!(
            validate_draft_line_edits(&requested, &[no_allocated], &coverage),
            Err(DraftLineEditViolation::MissingOriginalAllocatedQuantity)
        );
    }

    /// 多行场景稳定校验：既有行与请求行一一对应时全部通过。
    #[test]
    fn multi_line_edits_validate_stably() {
        let existing = vec![
            draft_line("sol-1", "2", "sku-1", "sorl-1", Some("sosl-1")),
            draft_line("sol-2", "1", "sku-2", "sorl-2", None),
        ];
        let coverage = vec![coverage_line("sol-1", "3"), coverage_line("sol-2", "0")];
        let requested = vec![
            edit("sol-1", Some("5"), Some("5"), "sku-1", "sorl-1", Some("sosl-1")),
            edit("sol-2", Some("1"), Some("1"), "sku-2", "sorl-2", None),
        ];
        assert!(validate_draft_line_edits(&requested, &existing, &coverage).is_ok());
    }
}
