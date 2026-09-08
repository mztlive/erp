//! 采购创建依据的请求校验、金额计算和草稿行构造。
use super::{business_date_of, zero_amount};
use crate::dto::purchase_order::CreatePurchaseOrderFromBasisRequest;
use crate::entity::facts::SalesOrderBasisFact;
use crate::entity::purchase_order::{
    basis_id_for, stable_line_id, supply_cost, BasisGroup, BasisLine, BasisScope, FulfillmentResponsibility,
    PurchaseLineType, PurchaseOrderSubmissionLine, PurchaseOrderSubmissionLineData, RequestedLine,
};
use crate::{Error, Result};
use erp_core::common::time::BusinessDate;
use erp_core::ids::{PurchaseOrderSubmissionId, PurchaseOrderSubmissionLineId, SalesOrderId};
use erp_core::money::{line_amounts, Amount, Quantity, UnitPrice};
use id_generator::next_id;

/// 已通过事务内最新剩余量校验的采购行。
#[derive(Debug, Clone)]
pub struct SelectedLine {
    /// 当前依据行。
    basis: BasisLine,
    /// 本次采购数量。
    quantity: Quantity,
    /// 采购确认的预计交付日。
    expected_delivery_date: BusinessDate,
}

/// 一组已计算金额的本次采购行。
pub struct ComputedSelection {
    /// 已舍入行金额汇总。
    pub totals: (Amount, Amount, Amount),
    /// 逐行成本与金额。
    pub lines: Vec<ComputedLine>,
}

/// 单条已计算采购行。
pub struct ComputedLine {
    /// 事务内选择行。
    selected: SelectedLine,
    /// 含税成本。
    cost: UnitPrice,
    /// 含税金额。
    gross: Amount,
    /// 不含税金额。
    net: Amount,
    /// 税额。
    tax: Amount,
}

/// 计算选中行金额与表头汇总。
///
/// # 参数
/// * `selected_lines` - 事务内校验通过的本次采购行
///
/// # 返回
/// 返回逐行已舍入金额及汇总。
///
/// # 错误
/// 无。
///
/// # 关键业务约束
/// 表头只汇总逐行舍入后的金额。
pub fn compute_selected_lines(
    selected_lines: &[SelectedLine],
    responsibility: FulfillmentResponsibility,
) -> ComputedSelection {
    let mut gross_total = zero_amount();
    let mut net_total = zero_amount();
    let mut tax_total = zero_amount();
    let mut lines = Vec::with_capacity(selected_lines.len());
    for selected in selected_lines {
        let cost = supply_cost(&selected.basis.supply.revision, responsibility);
        let (gross, net, tax) = line_amounts(
            cost,
            selected.quantity,
            selected.basis.supply.revision.input_tax_rate,
        );
        gross_total = gross_total.checked_add(gross);
        net_total = net_total.checked_add(net);
        tax_total = tax_total.checked_add(tax);
        lines.push(ComputedLine {
            selected: selected.clone(),
            cost,
            gross,
            net,
            tax,
        });
    }
    ComputedSelection {
        totals: (gross_total, net_total, tax_total),
        lines,
    }
}

/// 构造单条采购草稿行。
///
/// # 参数
/// * `submission_id` - 所属采购提交
/// * `line_no` - 提交内行号
/// * `line` - 已计算本次采购行
///
/// # 返回
/// 返回带稳定销售行和销售当前版本行的采购提交行。
///
/// # 错误
/// 实体不变式失败时返回错误。
///
/// # 关键业务约束
/// `quantity` 与 `allocated_quantity` 均等于本次事务内校验数量。
pub fn build_submission_line(
    submission_id: &PurchaseOrderSubmissionId,
    line_no: u32,
    line: &ComputedLine,
) -> Result<PurchaseOrderSubmissionLine> {
    let basis = &line.selected.basis;
    PurchaseOrderSubmissionLine::new(
        PurchaseOrderSubmissionLineId::new(next_id()),
        PurchaseOrderSubmissionLineData {
            purchase_order_submission_id: submission_id.clone(),
            line_no,
            line_type: PurchaseLineType::ItemService,
            procurement_confirmation_line_id: Some(erp_core::ids::ProcurementConfirmationLineId::new(
                stable_line_id(basis).to_string(),
            )),
            sku_id: Some(basis.coverage.goods_line.sku_id.clone()),
            sku_revision_id: Some(basis.coverage.goods_line.sku_revision_id.clone()),
            product_name_snapshot: Some(basis.coverage.revision_line.item_name_snapshot.clone()),
            specification_snapshot: basis.coverage.revision_line.spec_snapshot.clone(),
            quantity: Some(line.selected.quantity),
            base_unit_code: Some(basis.coverage.goods_line.base_unit_code.clone()),
            unit_cost_gross: Some(line.cost),
            gross_amount: line.gross,
            net_amount: line.net,
            tax_amount: line.tax,
            input_tax_rate: Some(basis.supply.revision.input_tax_rate),
            expected_delivery_date: Some(line.selected.expected_delivery_date),
            sales_order_line_id: Some(basis.coverage.revision_line.sales_order_line_id.clone()),
            sales_order_revision_line_id: Some(erp_core::ids::SalesOrderRevisionLineId::new(
                basis.coverage.revision_line.base.id.clone(),
            )),
            sales_order_submission_line_id: None,
            allocated_quantity: Some(line.selected.quantity),
        },
    )
    .map_err(Into::into)
}

/// 查找客户端选择的精确依据。
///
/// # 参数
/// * `order` - 销售稳定单
/// * `groups` - 当前可用依据集合
/// * `basis_id` - 客户端依据 ID
/// * `work_item_id` - 当前开放供给分配任务
///
/// # 返回
/// 返回与当前 guard、当前版本及精确范围完全匹配的依据。
///
/// # 错误
/// 依据不存在或已失效时返回统一的剩余数量变化冲突。
///
/// # 关键业务约束
/// 不接受旧 guard 或旧销售版本生成的依据 ID。
pub fn find_requested_group<'a>(
    order: &SalesOrderBasisFact,
    groups: &'a [BasisGroup],
    basis_id: &str,
    work_item_id: &str,
) -> Result<&'a BasisGroup> {
    groups
        .iter()
        .find(|group| basis_id_for(order, group, work_item_id, None) == basis_id)
        .ok_or_else(procurement_quantity_changed)
}

/// 校验请求表头与依据精确范围一致。
///
/// # 参数
/// * `req` - 创建请求
/// * `scope` - 依据精确范围
///
/// # 返回
/// 一致时返回 `Ok(())`。
///
/// # 错误
/// 采购类型或付款条件不一致时返回校验错误。
///
/// # 关键业务约束
/// 客户端不能把一个依据改造成另一拆分范围。
pub fn ensure_request_scope(req: &CreatePurchaseOrderFromBasisRequest, scope: &BasisScope) -> Result<()> {
    if req.purchase_type != scope.purchase_type {
        return Err(Error::ValidationError("采购类型与创建依据不一致".to_string()));
    }
    if req.payment_term_code.trim() != scope.payment_term_code {
        return Err(Error::ValidationError("付款条件与创建依据不一致".to_string()));
    }
    Ok(())
}

/// 按事务内最新依据校验逐行本次数量。
///
/// # 参数
/// * `requested` - 已规范化请求行
/// * `group` - guard 后重算的最新精确依据
///
/// # 返回
/// 返回按请求稳定行排序的已选择采购行。
///
/// # 错误
/// 请求行不属于依据，或数量超过最新剩余量/供应商可供上限时返回冲突。
///
/// # 关键业务约束
/// 同时校验 `quantity <= remaining` 与 `quantity <= min(remaining, available)`。
pub fn validate_requested_quantities(
    requested: &[RequestedLine],
    group: &BasisGroup,
) -> Result<Vec<SelectedLine>> {
    let mut selected = Vec::with_capacity(requested.len());
    for requested_line in requested {
        let basis = group
            .lines
            .iter()
            .find(|line| stable_line_id(line) == requested_line.sales_order_line_id)
            .ok_or_else(procurement_quantity_changed)?;
        crate::entity::purchase_order::ensure_sourcing_quantity(
            requested_line.quantity,
            basis.coverage.quantity_scale,
            &basis.coverage.goods_line.base_unit_code,
        )?;
        if requested_line.quantity > basis.coverage.summary.remaining_quantity
            || requested_line.quantity > basis.max_create_quantity
        {
            return Err(procurement_quantity_changed());
        }
        let sales_due = business_date_of(basis.coverage.goods_line.fulfillment_due_at)?;
        ensure_expected_delivery_within_sales_due(requested_line.expected_delivery_date, sales_due)?;
        selected.push(SelectedLine {
            basis: basis.clone(),
            quantity: requested_line.quantity,
            expected_delivery_date: requested_line.expected_delivery_date,
        });
    }
    Ok(selected)
}

/// 校验采购预计交付日不突破销售对客户的承诺期限。
///
/// # 参数
/// * `expected_delivery_date` - 采购确认的预计交付日
/// * `sales_due` - 销售对客户承诺的最晚交付日
///
/// # 返回
/// 预计交付日不晚于销售承诺期限时返回 `Ok(())`。
///
/// # 错误
/// 预计交付日晚于销售承诺期限时返回校验错误。
pub fn ensure_expected_delivery_within_sales_due(
    expected_delivery_date: BusinessDate,
    sales_due: BusinessDate,
) -> Result<()> {
    if expected_delivery_date > sales_due {
        return Err(Error::ValidationError(format!(
            "预计交付日不能晚于销售承诺期限 {sales_due}"
        )));
    }
    Ok(())
}

/// 从依据 ID 提取销售单稳定身份。
///
/// # 参数
/// * `basis_id` - `{sales_order_id}:{sha256}` 形式的依据 ID
///
/// # 返回
/// 返回销售单 ID。
///
/// # 错误
/// 依据 ID 形态非法时返回 `NotFound`。
///
/// # 关键业务约束
/// 不兼容旧 `{sales_order_id}:{supplier_id}` 依据 ID。
pub fn parse_basis_sales_order_id(basis_id: &str) -> Result<SalesOrderId> {
    let (sales_order_id, digest) = basis_id
        .trim()
        .split_once(':')
        .ok_or_else(|| Error::NotFound("采购创建依据不存在".to_string()))?;
    if sales_order_id.is_empty()
        || digest.len() != 64
        || !digest.bytes().all(|value| value.is_ascii_hexdigit())
    {
        return Err(Error::NotFound("采购创建依据不存在".to_string()));
    }
    Ok(SalesOrderId::new(sales_order_id.to_string()))
}

/// 返回统一的采购剩余或供给变化冲突。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 HTTP 409 对应的稳定业务错误。
///
/// # 错误
/// 无。
pub fn procurement_quantity_changed() -> Error {
    Error::ConflictError("可分配供给数量已更新，请刷新后重试".to_string())
}
