//! 询价转销售的需求来源；不保存供应商、内部报价或正式选源事实。
use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::{CustomerAccountId, SalesOrderLineId, SkuId, SkuRevisionId};
use erp_core::money::Quantity;
use serde::{Deserialize, Serialize};

use super::{SalesDeliveryAllocation, SalesOrderSubmissionLine, SalesOrderWorkingCopyLine};
use crate::entity::sales_inquiry::{
    InquiryDeliveryAllocation, InquiryFee, InquiryQuantitySelection, InquiryRequirementLine,
    InquirySupplyPrice, SalesInquiry,
};
use crate::{Error, Result};

/// 一条询价地址的本次转单数量。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySalesAllocation {
    pub allocation_id: String,
    pub quantity: Quantity,
}

/// 显式选择的询价行与销售行号；成交价仍由销售草稿单独确认。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySalesLineSelection {
    pub line_no: u32,
    pub inquiry_line_id: String,
    pub price_id: String,
    pub quantity: Quantity,
    #[serde(default)]
    pub allocations: Vec<InquirySalesAllocation>,
}

/// 询价来源命令；客户端不得携带供应商及内部报价身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InquirySalesSourceRequest {
    pub inquiry_id: String,
    pub feedback_round: u32,
    pub selections: Vec<InquirySalesLineSelection>,
}

impl InquirySalesSourceRequest {
    /// 校验稳定身份、行号与数量分配，不执行数据库读取。
    /// # 参数
    /// 无。
    /// # 返回
    /// 所有来源选择均唯一且数量守恒时成功。
    /// # 错误
    /// 身份缺失、重复、数量非正或地址数量不守恒时拒绝。
    pub fn validate(&self) -> Result<()> {
        if self.inquiry_id.trim().is_empty()
            || self.feedback_round == 0
            || self.selections.is_empty()
            || self.selections.len() > 200
        {
            return Err(invalid("询价来源身份、轮次或商品行数无效"));
        }
        let mut lines = HashSet::new();
        let mut sources = HashSet::new();
        for line in &self.selections {
            if line.line_no == 0
                || line.inquiry_line_id.trim().is_empty()
                || line.price_id.trim().is_empty()
                || !lines.insert(line.line_no)
                || !sources.insert((&line.inquiry_line_id, &line.price_id))
                || line.quantity.to_decimal() <= rust_decimal::Decimal::ZERO
            {
                return Err(invalid("询价来源行重复、身份缺失或数量无效"));
            }
            validate_allocations(line)?;
        }
        Ok(())
    }
}

fn validate_allocations(line: &InquirySalesLineSelection) -> Result<()> {
    let mut ids = HashSet::new();
    let mut total = rust_decimal::Decimal::ZERO;
    if line.allocations.len() > 100 {
        return Err(invalid("每条商品最多分配 100 个收货地址"));
    }
    for allocation in &line.allocations {
        if allocation.allocation_id.trim().is_empty()
            || !ids.insert(&allocation.allocation_id)
            || allocation.quantity.to_decimal() <= rust_decimal::Decimal::ZERO
        {
            return Err(invalid("询价来源地址重复、身份缺失或数量无效"));
        }
        total = total
            .checked_add(allocation.quantity.to_decimal())
            .ok_or_else(|| invalid("询价来源数量超出范围"))?;
    }
    if !line.allocations.is_empty() && total != line.quantity.to_decimal() {
        return Err(invalid("询价来源地址数量合计必须等于商品数量"));
    }
    Ok(())
}

/// 来源地址及不可变收件资料；数量独立于收件身份。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquirySalesAddressBinding {
    pub allocation_id: String,
    pub delivery: SalesDeliveryAllocation,
}

/// 稳定销售行与询价需求行的绑定；历史来源不会随主数据及反馈改版覆盖。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InquirySalesLineBinding {
    pub sales_order_line_id: SalesOrderLineId,
    pub inquiry_line_id: String,
    pub price: InquirySupplyPrice,
    pub sku_id: SkuId,
    pub sku_revision_id: SkuRevisionId,
    pub fulfillment_due_at: Instant,
    pub quantity: Quantity,
    pub allocations: Vec<InquirySalesAddressBinding>,
}

/// 生命周期校验消费的最小销售行事实，不包含成交价或采购资料。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct InquirySalesLineFact {
    pub sales_order_line_id: SalesOrderLineId,
    pub sku_id: Option<SkuId>,
    pub sku_revision_id: Option<SkuRevisionId>,
    pub fulfillment_due_at: Option<Instant>,
    pub quantity: Option<Quantity>,
    pub delivery_allocations: Vec<SalesDeliveryAllocation>,
}

impl From<&SalesOrderWorkingCopyLine> for InquirySalesLineFact {
    fn from(line: &SalesOrderWorkingCopyLine) -> Self {
        Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            sku_id: line.sku_id.clone(),
            sku_revision_id: line.sku_revision_id.clone(),
            fulfillment_due_at: line.fulfillment_due_at,
            quantity: line.quantity,
            delivery_allocations: line.delivery_allocations.clone(),
        }
    }
}
impl From<&SalesOrderSubmissionLine> for InquirySalesLineFact {
    fn from(line: &SalesOrderSubmissionLine) -> Self {
        Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            sku_id: line.sku_id.clone(),
            sku_revision_id: line.sku_revision_id.clone(),
            fulfillment_due_at: line.fulfillment_due_at,
            quantity: line.quantity,
            delivery_allocations: line.delivery_allocations.clone(),
        }
    }
}

/// 一张来源销售单的数量占用和需求映射；ID 与销售单 ID 相同。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Entity)]
pub struct SalesOrderInquirySource {
    #[serde(flatten)]
    pub base: BaseModel,
    pub inquiry_id: String,
    pub inquiry_no: String,
    pub requirement_version: u32,
    pub feedback_round: u32,
    pub customer_id: CustomerAccountId,
    pub lines: Vec<InquirySalesLineBinding>,
    /// 销售草稿关联的已接受费用结果；归属与办理额度由询价费用台账单独核算。
    #[serde(default)]
    pub fees: Vec<InquiryFee>,
    #[serde(default)]
    pub consumed: bool,
    #[serde(default)]
    pub released: bool,
}

impl SalesOrderInquirySource {
    /// 按已接受的精确需求与反馈建立来源；成交单价不参与来源复制。
    /// # 参数
    /// `id` 为销售单 ID；`inquiry` 为当前授权询价，`request` 为明确选择，`lines` 为服务端销售行。
    /// # 返回
    /// 返回不含内部报价的稳定行及地址来源快照。
    /// # 错误
    /// 选择无效、报价失效、客户或服务端行不匹配时拒绝。
    pub fn from_inquiry(
        id: String,
        inquiry: &SalesInquiry,
        request: &InquirySalesSourceRequest,
        lines: &[SalesOrderWorkingCopyLine],
        now: Instant,
    ) -> Result<Self> {
        request.validate()?;
        let feedback = inquiry.accepted_feedback(request.feedback_round, now)?;
        if lines.len() != request.selections.len() {
            return Err(invalid("询价转销售明细与来源选择不一致"));
        }
        let bindings = request
            .selections
            .iter()
            .map(|selection| {
                let (demand, price) = selection.source_parts(inquiry, request.feedback_round, now)?;
                let line = lines
                    .iter()
                    .find(|line| line.line_no == selection.line_no)
                    .ok_or_else(|| invalid("询价转销售缺少对应销售行"))?;
                binding_from_selection(selection, demand, price, line)
            })
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            base: BaseModel::new(id),
            inquiry_id: inquiry.base.id.clone(),
            inquiry_no: inquiry.inquiry_no.clone(),
            requirement_version: feedback.requirement_version,
            feedback_round: feedback.round,
            customer_id: CustomerAccountId::new(&inquiry.customer_id),
            lines: bindings,
            fees: feedback.data.fees.clone(),
            consumed: false,
            released: false,
        })
    }

    /// 取得当前有效来源数量，稳定需求、价格和地址身份随生命周期延续。
    /// # 参数
    /// 无。
    /// # 返回
    /// 返回非零商品及地址数量；已删除来源行保留快照但不参与占用。
    /// # 错误
    /// 无。
    pub fn selections(&self) -> Vec<InquiryQuantitySelection> {
        let mut result = Vec::new();
        for line in self.lines.iter().filter(|line| !line.quantity.to_decimal().is_zero()) {
            if line.allocations.is_empty() {
                result.push(InquiryQuantitySelection {
                    line_id: line.inquiry_line_id.clone(),
                    price_id: Some(line.price.price_id.clone()),
                    allocation_id: None,
                    quantity: line.quantity,
                });
            } else {
                result.extend(
                    line.allocations.iter().filter(|row| !row.delivery.quantity.to_decimal().is_zero()).map(
                        |row| InquiryQuantitySelection {
                            line_id: line.inquiry_line_id.clone(),
                            price_id: Some(line.price.price_id.clone()),
                            allocation_id: Some(row.allocation_id.clone()),
                            quantity: row.delivery.quantity,
                        },
                    ),
                );
            }
        }
        result
    }

    /// 校验正式变更仍在原消耗身份与数量内，减量不恢复来源余额。
    /// # 参数
    /// `customer` 与 `lines` 为即将生效的新销售版本事实。
    /// # 返回
    /// 保持原客户、商品、交期、地址且未增加来源数量时成功。
    /// # 错误
    /// 未消耗或已释放、改来源身份、商品或地址数量增加时拒绝。
    pub fn ensure_formal_change(
        &self,
        customer: &CustomerAccountId,
        lines: &[InquirySalesLineFact],
    ) -> Result<()> {
        if !self.consumed || self.released {
            return Err(invalid("正式销售单的询价消耗事实不完整"));
        }
        for changed in self.reconciled(customer, lines)? {
            let original = self
                .lines
                .iter()
                .find(|old| old.sales_order_line_id == changed.sales_order_line_id)
                .ok_or_else(|| invalid("正式询价来源行缺失"))?;
            if changed.quantity > original.quantity
                || changed.allocations.iter().any(|row| {
                    original
                        .allocations
                        .iter()
                        .find(|old| old.allocation_id == row.allocation_id)
                        .is_none_or(|old| row.delivery.quantity > old.delivery.quantity)
                })
            {
                return Err(invalid("正式销售增量或调整地址数量须先完成新的询价与来源确认"));
            }
        }
        Ok(())
    }

    /// 将草稿与原需求身份对齐，减少数量允许，客户、SKU、地址与交期变化拒绝。
    /// # 参数
    /// `customer` 为本次客户，`lines` 为即将保存或提交的完整销售行。
    /// # 返回
    /// 返回同一来源身份下的新数量绑定，删除的来源行保留零数量记录。
    /// # 错误
    /// 改变客户、来源身份或原地址，及来源地址数量无法唯一匹配时拒绝。
    pub fn reconciled(
        &self,
        customer: &CustomerAccountId,
        lines: &[InquirySalesLineFact],
    ) -> Result<Vec<InquirySalesLineBinding>> {
        if customer != &self.customer_id {
            return Err(invalid("询价来源销售单不能更换客户，请重新询价"));
        }
        if lines.iter().any(|line| {
            line.quantity.is_some_and(|quantity| quantity.to_decimal() > rust_decimal::Decimal::ZERO)
                && !self.lines.iter().any(|binding| binding.sales_order_line_id == line.sales_order_line_id)
        }) {
            return Err(invalid("询价来源销售单不得直接新增无来源商品，请先重新询价"));
        }
        self.lines
            .iter()
            .map(|binding| {
                let matching =
                    lines.iter().find(|line| line.sales_order_line_id == binding.sales_order_line_id);
                match matching {
                    Some(line) => binding.reconciled(line),
                    None => Ok(binding.without_quantity()),
                }
            })
            .collect()
    }
}

impl InquirySalesLineSelection {
    /// 读取已经接受的需求及选中价格段，并校验本次范围。
    /// # 参数
    /// `inquiry` 为当前询价；`round` 为明确轮次；`now` 为服务端时间。
    /// # 返回
    /// 返回原不可变需求行与公开供给价格段。
    /// # 错误
    /// 未接受、失效、档位或地址不适用时拒绝，不查找替代报价。
    pub fn source_parts<'a>(
        &self,
        inquiry: &'a SalesInquiry,
        round: u32,
        now: Instant,
    ) -> Result<(&'a InquiryRequirementLine, &'a InquirySupplyPrice)> {
        let feedback = inquiry.accepted_feedback(round, now)?;
        let requirement = inquiry
            .requirements
            .iter()
            .find(|snapshot| snapshot.version == feedback.requirement_version)
            .ok_or_else(|| invalid("已接受反馈缺少原需求快照"))?;
        let demand = requirement
            .data
            .lines
            .iter()
            .find(|line| line.line_id == self.inquiry_line_id)
            .ok_or_else(|| invalid("来源需求行不存在"))?;
        let result = feedback
            .data
            .lines
            .iter()
            .find(|line| line.line_id == self.inquiry_line_id)
            .ok_or_else(|| invalid("已接受反馈缺少商品行"))?;
        let price = result
            .prices
            .iter()
            .find(|price| price.price_id == self.price_id)
            .ok_or_else(|| invalid("选中的供给价格不存在或该商品不供应"))?;
        self.ensure_price_scope(price, demand, result.available_quantity, now)?;
        Ok((demand, price))
    }

    fn ensure_price_scope(
        &self,
        price: &InquirySupplyPrice,
        demand: &InquiryRequirementLine,
        available: Quantity,
        now: Instant,
    ) -> Result<()> {
        if price.valid_until <= now || self.quantity > available {
            return Err(invalid("询价价格已过期或本次数量超过可供量，请重新询价"));
        }
        if demand.allocations.is_empty() != self.allocations.is_empty() {
            return Err(invalid("询价需求配送范围不匹配"));
        }
        for allocation in &self.allocations {
            if !demand.allocations.iter().any(|available| {
                available.allocation_id == allocation.allocation_id
                    && available.quantity >= allocation.quantity
            }) {
                return Err(invalid("询价需求不覆盖本次地址数量"));
            }
        }
        Ok(())
    }
}

fn binding_from_selection(
    selection: &InquirySalesLineSelection,
    demand: &InquiryRequirementLine,
    price: &InquirySupplyPrice,
    line: &SalesOrderWorkingCopyLine,
) -> Result<InquirySalesLineBinding> {
    let allocations = selection
        .allocations
        .iter()
        .map(|selected| {
            let original = demand
                .allocations
                .iter()
                .find(|row| row.allocation_id == selected.allocation_id)
                .ok_or_else(|| invalid("询价需求地址不存在"))?;
            Ok(InquirySalesAddressBinding {
                allocation_id: selected.allocation_id.clone(),
                delivery: copied_delivery(original, selected.quantity),
            })
        })
        .collect::<Result<Vec<_>>>()?;
    let binding = InquirySalesLineBinding {
        sales_order_line_id: line.sales_order_line_id.clone(),
        inquiry_line_id: demand.line_id.clone(),
        price: price.clone(),
        sku_id: SkuId::new(&demand.sku_id),
        sku_revision_id: SkuRevisionId::new(&demand.sku_revision_id),
        fulfillment_due_at: demand.delivery_due_at,
        quantity: selection.quantity,
        allocations,
    };
    let actual = binding.reconciled(&InquirySalesLineFact::from(line))?;
    if actual != binding {
        return Err(invalid("询价转销售数量或配送分配与明确选择不一致"));
    }
    Ok(binding)
}

/// 复制历史收件身份，数量使用本次明确选择，不使用当前客户地址主数据。
/// # 参数
/// `original` 为询价冻结地址，`quantity` 为转单数量。
/// # 返回
/// 返回销售配送快照。
/// # 错误
/// 无。
pub fn copied_delivery(original: &InquiryDeliveryAllocation, quantity: Quantity) -> SalesDeliveryAllocation {
    SalesDeliveryAllocation {
        recipient: original.recipient.clone(),
        phone: original.phone.clone(),
        region_code: original.region_code.clone(),
        region_name: original.region_name.clone(),
        address: original.address.clone(),
        quantity,
    }
}

impl InquirySalesLineBinding {
    /// 对齐来源行的同一 SKU、交期和地址，不从最新主数据重建历史来源。
    fn reconciled(&self, line: &InquirySalesLineFact) -> Result<Self> {
        let quantity = line.quantity.ok_or_else(|| invalid("询价来源行缺少商品数量"))?;
        if quantity.to_decimal().is_zero() {
            return Ok(self.without_quantity());
        }
        if line.sku_id.as_ref() != Some(&self.sku_id)
            || line.sku_revision_id.as_ref() != Some(&self.sku_revision_id)
            || line.fulfillment_due_at != Some(self.fulfillment_due_at)
        {
            return Err(invalid("询价来源商品或交期已变化，请先重新询价"));
        }
        let mut result = self.clone();
        result.quantity = quantity;
        result.allocations = self.reconcile_addresses(&line.delivery_allocations)?;
        Ok(result)
    }

    fn without_quantity(&self) -> Self {
        let mut result = self.clone();
        result.quantity = zero_quantity();
        for allocation in &mut result.allocations {
            allocation.delivery.quantity = zero_quantity();
        }
        result
    }

    fn reconcile_addresses(
        &self,
        requested: &[SalesDeliveryAllocation],
    ) -> Result<Vec<InquirySalesAddressBinding>> {
        let mut result = self.allocations.clone();
        for row in &mut result {
            row.delivery.quantity = zero_quantity();
        }
        for allocation in requested {
            let matching = result
                .iter_mut()
                .filter(|binding| same_address(&binding.delivery, allocation))
                .collect::<Vec<_>>();
            if matching.len() != 1 {
                return Err(invalid("询价来源收货地址已变化或身份不明确，请先重新询价"));
            }
            let binding = matching.into_iter().next().ok_or_else(|| invalid("询价来源地址不存在"))?;
            if !binding.delivery.quantity.to_decimal().is_zero() {
                return Err(invalid("同一询价来源地址不得重复填写"));
            }
            binding.delivery.quantity = allocation.quantity;
        }
        if self.allocations.is_empty() && !requested.is_empty() {
            return Err(invalid("询价来源交付范围已变化，请先重新询价"));
        }
        if !self.allocations.is_empty() && requested.is_empty() {
            return Err(invalid("询价来源商品必须保留收货地址"));
        }
        Ok(result)
    }
}

fn same_address(left: &SalesDeliveryAllocation, right: &SalesDeliveryAllocation) -> bool {
    left.recipient == right.recipient
        && left.phone == right.phone
        && left.region_code == right.region_code
        && left.address == right.address
}

fn zero_quantity() -> Quantity {
    Quantity::try_from(rust_decimal::Decimal::ZERO).expect("零数量满足六位精度约束")
}

fn invalid(message: &str) -> Error {
    Error::BusinessLogicError(message.into())
}

#[cfg(test)]
mod tests {
    use std::slice;

    use erp_core::region::RegionCode;

    use super::*;

    #[test]
    fn selection_requires_unique_stable_rows_and_conserved_addresses() {
        let line = InquirySalesLineSelection {
            line_no: 1,
            inquiry_line_id: "line".into(),
            price_id: "price".into(),
            quantity: "2".parse().unwrap(),
            allocations: vec![InquirySalesAllocation {
                allocation_id: "address".into(),
                quantity: "2".parse().unwrap(),
            }],
        };
        let mut request = InquirySalesSourceRequest {
            inquiry_id: "inquiry".into(),
            feedback_round: 1,
            selections: vec![line.clone()],
        };
        request.validate().unwrap();
        request.selections.push(line);
        assert!(request.validate().is_err());
        request.selections.pop();
        request.selections[0].allocations[0].quantity = "1".parse().unwrap();
        assert!(request.validate().is_err());
        request.selections[0].allocations.clear();
        request.selections[0].quantity = zero_quantity();
        assert!(request.validate().is_err());
    }
    fn price() -> InquirySupplyPrice {
        InquirySupplyPrice {
            price_id: "price".into(),
            unit_price_gross: Some("8.00".parse().unwrap()),
            tax_rate: Some("0.13".parse().unwrap()),
            valid_until: Instant::from_unix_secs(3_000),
        }
    }
    fn source() -> SalesOrderInquirySource {
        let delivery = SalesDeliveryAllocation {
            recipient: "张三".into(),
            phone: "0013800000000".into(),
            region_code: RegionCode::parse_leaf("310115").unwrap(),
            region_name: "上海市浦东新区".into(),
            address: "测试路 1 号".into(),
            quantity: "2".parse().unwrap(),
        };
        SalesOrderInquirySource {
            base: BaseModel::new("sales".into()),
            inquiry_id: "inquiry".into(),
            inquiry_no: "IQ-1".into(),
            requirement_version: 1,
            feedback_round: 2,
            customer_id: CustomerAccountId::new("customer"),
            consumed: false,
            released: false,
            fees: Vec::new(),
            lines: vec![InquirySalesLineBinding {
                sales_order_line_id: SalesOrderLineId::new("stable"),
                inquiry_line_id: "demand".into(),
                price: price(),
                sku_id: SkuId::new("sku"),
                sku_revision_id: SkuRevisionId::new("revision"),
                fulfillment_due_at: Instant::from_unix_secs(2_000),
                quantity: "2".parse().unwrap(),
                allocations: vec![InquirySalesAddressBinding { allocation_id: "address".into(), delivery }],
            }],
        }
    }
    fn fact(source: &SalesOrderInquirySource) -> InquirySalesLineFact {
        let line = &source.lines[0];
        InquirySalesLineFact {
            sales_order_line_id: line.sales_order_line_id.clone(),
            sku_id: Some(line.sku_id.clone()),
            sku_revision_id: Some(line.sku_revision_id.clone()),
            fulfillment_due_at: Some(line.fulfillment_due_at),
            quantity: Some(line.quantity),
            delivery_allocations: line.allocations.iter().map(|row| row.delivery.clone()).collect(),
        }
    }
    #[test]
    fn draft_reduction_and_removed_rows_keep_stable_origin_without_overwriting_snapshot() {
        let source = source();
        let mut line = fact(&source);
        line.quantity = Some("1".parse().unwrap());
        line.delivery_allocations[0].quantity = "1".parse().unwrap();
        let changed = source.reconciled(&source.customer_id, &[line]).unwrap();
        assert_eq!(changed[0].quantity, "1".parse().unwrap());
        assert_eq!(changed[0].allocations[0].allocation_id, "address");
        assert_eq!(source.lines[0].quantity, "2".parse().unwrap());
        let removed = source.reconciled(&source.customer_id, &[]).unwrap();
        assert!(removed[0].quantity.to_decimal().is_zero());
        assert_eq!(removed[0].sku_id, source.lines[0].sku_id);
        assert_eq!(removed[0].allocations[0].delivery.phone, "0013800000000");
    }
    #[test]
    fn draft_source_rejects_customer_sku_revision_due_date_and_address_changes() {
        let source = source();
        assert!(source.reconciled(&CustomerAccountId::new("other"), &[fact(&source)]).is_err());
        let original = fact(&source);
        for changed in 0..6 {
            let mut line = original.clone();
            match changed {
                0 => line.sku_id = Some(SkuId::new("other")),
                1 => line.sku_revision_id = Some(SkuRevisionId::new("other")),
                2 => line.fulfillment_due_at = Some(Instant::from_unix_secs(2_001)),
                3 => line.delivery_allocations[0].address = "其他地址".into(),
                4 => line.sales_order_line_id = SalesOrderLineId::new("new-line"),
                _ => line.delivery_allocations.clear(),
            }
            assert!(source.reconciled(&source.customer_id, &[line]).is_err());
        }
    }
    #[test]
    fn price_scope_checks_expiry_available_quantity_and_demand_addresses() {
        let demand: InquiryRequirementLine = serde_json::from_value(serde_json::json!({
            "line_id": "demand", "sku_id": "sku", "sku_revision_id": "revision",
            "sku_code": "sku", "product_name": "商品", "specification": "盒", "unit_code": "盒",
            "quantity_scale": 0, "quantity": "10", "fulfillment_kind": "physical", "delivery_due_at": 2000,
            "allocations": [{"allocation_id": "address", "recipient": "收货人", "phone": "13800000000",
                "region_code": "310115", "region_name": "上海市浦东新区", "address": "测试路", "quantity": "10"}]
        }))
        .unwrap();
        let available = "10".parse().unwrap();
        let mut selection = InquirySalesLineSelection {
            line_no: 1,
            inquiry_line_id: "demand".into(),
            price_id: "price".into(),
            quantity: "2".parse().unwrap(),
            allocations: vec![InquirySalesAllocation {
                allocation_id: "address".into(),
                quantity: "2".parse().unwrap(),
            }],
        };
        selection.ensure_price_scope(&price(), &demand, available, Instant::from_unix_secs(2_999)).unwrap();
        assert!(
            selection
                .ensure_price_scope(&price(), &demand, available, Instant::from_unix_secs(3_000))
                .is_err()
        );
        selection.quantity = "1".parse().unwrap();
        selection.ensure_price_scope(&price(), &demand, available, Instant::from_unix_secs(1)).unwrap();
        selection.quantity = "11".parse().unwrap();
        assert!(
            selection.ensure_price_scope(&price(), &demand, available, Instant::from_unix_secs(1)).is_err()
        );
        selection.quantity = "2".parse().unwrap();
        selection.allocations[0].allocation_id = "foreign".into();
        assert!(
            selection.ensure_price_scope(&price(), &demand, available, Instant::from_unix_secs(1)).is_err()
        );
    }

    #[test]
    fn formal_reduction_does_not_release_consumed_quantity_and_cannot_regrow_past_original() {
        let mut source = source();
        let original = fact(&source);
        assert!(source.ensure_formal_change(&source.customer_id, slice::from_ref(&original)).is_err());
        source.consumed = true;
        let mut reduced = original.clone();
        reduced.quantity = Some("1".parse().unwrap());
        reduced.delivery_allocations[0].quantity = "1".parse().unwrap();
        source.ensure_formal_change(&source.customer_id, &[reduced]).unwrap();
        assert_eq!(source.lines[0].quantity, "2".parse().unwrap());
        assert_eq!(source.selections()[0].quantity, "2".parse().unwrap());
        let mut increased = original;
        increased.quantity = Some("3".parse().unwrap());
        increased.delivery_allocations[0].quantity = "3".parse().unwrap();
        assert!(source.ensure_formal_change(&source.customer_id, &[increased]).is_err());
        source.released = true;
        assert!(source.ensure_formal_change(&source.customer_id, &[]).is_err());
    }
}
