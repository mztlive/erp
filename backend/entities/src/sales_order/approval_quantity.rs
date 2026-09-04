//! 销售提交审批总数量。
//!
//! 实物及服务按各行基础单位数量精确合计；卡券固定取唯一卡券行的卡张数，换算为
//! 数量（单位为张）。Service 只负责把结果写入审批快照，不得再按 `quantity`
//! 过滤或兜底。

use rust_decimal::Decimal;

use crate::errors::{Error, Result};
use crate::money::Quantity;

use super::submission::{SalesOrderSubmission, SalesOrderSubmissionLine};
use super::types::{integer_quantity, validate_line_list, BusinessType, LineSummary};

impl SalesOrderSubmission {
    /// 按业务性质计算审批快照总数量。
    ///
    /// 实物及服务汇总各行基础单位 `quantity`；卡券取唯一卡券行 `card_count`，
    /// 精确转换成 `Quantity`（单位为张）。
    ///
    /// # 参数
    /// * `lines` - 该提交的全部明细行
    ///
    /// # 返回
    /// 返回审批快照应冻结的总数量。
    ///
    /// # 错误
    /// 空行、行类型与业务性质不一致、适用行缺量、卡张数缺失或数量溢出时返回领域错误。
    ///
    /// # 关键业务约束
    /// 不得跳过无 `quantity` 的卡券行；卡券数量不得从 `quantity` 字段读取。
    pub fn approval_total_quantity(&self, lines: &[SalesOrderSubmissionLine]) -> Result<Quantity> {
        let summaries = lines
            .iter()
            .map(|line| LineSummary {
                line_no: line.line_no,
                line_id: line.sales_order_line_id.clone(),
                line_type: line.line_type,
            })
            .collect::<Vec<_>>();
        validate_line_list(self.business_type, &summaries)?;
        match self.business_type {
            BusinessType::GoodsService => sum_goods_quantities(lines),
            BusinessType::Voucher => voucher_line_quantity(required_single_line(lines)?),
        }
    }
}

/// 在行清单已通过跨行断言后取出唯一一行。
///
/// # 参数
/// * `lines` - 已通过 [`validate_line_list`] 的明细行
///
/// # 返回
/// 恰好一行时返回该行。
///
/// # 错误
/// 行数不是 1 时返回领域错误。
///
/// # 关键业务约束
/// 卡券审批数量只承认唯一卡券行，禁止按多行合计。
fn required_single_line(lines: &[SalesOrderSubmissionLine]) -> Result<&SalesOrderSubmissionLine> {
    match lines {
        [line] => Ok(line),
        _ => Err(Error::from("卡券销售单每个版本必须恰好包含一条卡券明细")),
    }
}

/// 合计实物及服务行的基础单位数量。
///
/// # 参数
/// * `lines` - 已确认为实物及服务的明细行
///
/// # 返回
/// 返回精确合计数量。
///
/// # 错误
/// 任一行缺少数量、合计溢出或超出数量精度时返回领域错误。
///
/// # 关键业务约束
/// 缺量必须失败，禁止 `filter_map` 跳过。
fn sum_goods_quantities(lines: &[SalesOrderSubmissionLine]) -> Result<Quantity> {
    let mut total = Decimal::ZERO;
    for line in lines {
        let quantity = goods_line_quantity(line)?;
        total = total
            .checked_add(quantity.to_decimal())
            .ok_or_else(|| Error::from("审批总数量溢出"))?;
    }
    quantity_from_total(total)
}

/// 读取实物及服务行已冻结的基础单位数量。
///
/// # 参数
/// * `line` - 实物及服务提交行
///
/// # 返回
/// 返回该行基础单位数量。
///
/// # 错误
/// `quantity` 缺失时返回领域错误。
///
/// # 关键业务约束
/// 不得用零值或空值代替缺失数量。
fn goods_line_quantity(line: &SalesOrderSubmissionLine) -> Result<Quantity> {
    line.quantity
        .ok_or_else(|| Error::from(format!("第 {} 行缺少数量，无法冻结审批快照", line.line_no)))
}

/// 将唯一卡券行的卡张数换算为审批数量。
///
/// # 参数
/// * `line` - 卡券提交行
///
/// # 返回
/// 返回以张为单位的 `Quantity`。
///
/// # 错误
/// 卡张数缺失、为零或超出数量精度时返回领域错误。
///
/// # 关键业务约束
/// 只使用 `card_count`，忽略可能为空的 `quantity`。
fn voucher_line_quantity(line: &SalesOrderSubmissionLine) -> Result<Quantity> {
    let card_count = line
        .card_count
        .ok_or_else(|| Error::from(format!("第 {} 行缺少卡张数，无法冻结审批快照", line.line_no)))?;
    if card_count == 0 {
        return Err(Error::from(format!("第 {} 行卡张数必须为正整数", line.line_no)));
    }
    integer_quantity(card_count)
}

/// 把已合计的定点值收成数量值对象。
///
/// # 参数
/// * `total` - 已完成 checked 合计的定点值
///
/// # 返回
/// 返回合规 `Quantity`。
///
/// # 错误
/// 有效小数位超过数量精度时返回领域错误。
///
/// # 关键业务约束
/// 禁止静默舍入。
fn quantity_from_total(total: Decimal) -> Result<Quantity> {
    Quantity::try_from(total).map_err(|error| Error::from(format!("审批总数量超出精度：{error}")))
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use rust_decimal::Decimal;

    use super::*;
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        ContractRevisionId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId,
        SalesOrderSubmissionId, SalesOrderSubmissionLineId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use crate::money::{Amount, Rate, UnitPrice};
    use crate::sales_order::snapshot::HeaderSnapshotData;
    use crate::sales_order::submission::SalesOrderSubmissionData;
    use crate::sales_order::types::{CardForm, GoodsLineFields, LineType, VoucherLineDraft};

    fn amt(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    fn qty(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    fn price(value: &str) -> UnitPrice {
        UnitPrice::from_str(value).unwrap()
    }

    fn goods_fields(quantity: &str) -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: None,
            service_region: None,
            fulfillment_due_at: Instant::from_unix_secs(1_800_000_000),
            quantity: qty(quantity),
            base_unit_code: "件".to_string(),
            unit_price_gross: price("10.0000"),
        }
    }

    fn goods_line_data(
        line_no: u32,
        quantity: &str,
    ) -> crate::sales_order::submission::SalesOrderSubmissionLineData {
        crate::sales_order::submission::SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.000000"),
            item_name_snapshot: "商品".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("件".to_string()),
            goods: Some(goods_fields(quantity)),
            voucher: None,
        }
    }

    fn voucher_line_data(
        line_no: u32,
        card_count: u32,
    ) -> crate::sales_order::submission::SalesOrderSubmissionLineData {
        let count = Decimal::from(card_count);
        crate::sales_order::submission::SalesOrderSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::Voucher,
            sales_tax_rate: rate("0.000000"),
            item_name_snapshot: "卡券".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("张".to_string()),
            goods: None,
            voucher: Some(VoucherLineDraft {
                face_value: amt("100.00"),
                card_count,
                unit_price_gross: price("90.0000"),
                face_value_total: Amount::try_from(Decimal::from_str("100.00").unwrap() * count).unwrap(),
                transaction_amount: Amount::try_from(Decimal::from_str("90.00").unwrap() * count).unwrap(),
                gift_amount: Amount::try_from(Decimal::from_str("10.00").unwrap() * count).unwrap(),
                gift_rate: None,
                card_form: CardForm::Electronic,
            }),
        }
    }

    fn goods_header(
        lines: Vec<crate::sales_order::submission::SalesOrderSubmissionLineData>,
    ) -> SalesOrderSubmissionData {
        let mut gross = Decimal::ZERO;
        for line in &lines {
            let quantity = line.goods.as_ref().unwrap().quantity.to_decimal();
            gross += Decimal::from_str("10.00").unwrap() * quantity;
        }
        let gross_amount = Amount::try_from(gross).unwrap();
        SalesOrderSubmissionData {
            sales_order_id: SalesOrderId::new("o-1"),
            submission_no: 1,
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 1,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: HeaderSnapshotData {
                customer_name: "客户".to_string(),
                contract_no: None,
                settlement_party_name: Some("结算".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结".to_string(),
                invoice_type: "普通发票".to_string(),
                tax_point: "开票".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            receivable_due_date: None,
            gross_amount,
            net_amount: gross_amount,
            tax_amount: amt("0.00"),
            submitted_at: Instant::from_unix_secs(1_790_000_000),
            submitted_by: "sales-1".to_string(),
            lines,
        }
    }

    fn voucher_header(
        line: crate::sales_order::submission::SalesOrderSubmissionLineData,
    ) -> SalesOrderSubmissionData {
        let card_count = Decimal::from(line.voucher.as_ref().unwrap().card_count);
        let gross = Amount::try_from(Decimal::from_str("90.00").unwrap() * card_count).unwrap();
        SalesOrderSubmissionData {
            business_type: BusinessType::Voucher,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            receivable_due_date: Some(BusinessDate::from_ymd(2026, 10, 31).unwrap()),
            gross_amount: gross,
            net_amount: gross,
            tax_amount: amt("0.00"),
            lines: vec![line],
            ..goods_header(vec![goods_line_data(1, "1")])
        }
    }

    fn goods_submission(
        line_data: Vec<crate::sales_order::submission::SalesOrderSubmissionLineData>,
    ) -> (SalesOrderSubmission, Vec<SalesOrderSubmissionLine>) {
        let submission = SalesOrderSubmission::new(
            SalesOrderSubmissionId::new("s-1"),
            goods_header(line_data.clone()),
        )
        .unwrap();
        let lines = line_data
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                SalesOrderSubmissionLine::new(
                    SalesOrderSubmissionLineId::new(format!("sl-{index}")),
                    SalesOrderSubmissionId::new("s-1"),
                    data,
                )
                .unwrap()
            })
            .collect();
        (submission, lines)
    }

    fn voucher_submission(
        line_data: crate::sales_order::submission::SalesOrderSubmissionLineData,
    ) -> (SalesOrderSubmission, SalesOrderSubmissionLine) {
        let submission = SalesOrderSubmission::new(
            SalesOrderSubmissionId::new("s-v"),
            voucher_header(line_data.clone()),
        )
        .unwrap();
        let line = SalesOrderSubmissionLine::new(
            SalesOrderSubmissionLineId::new("sl-v"),
            SalesOrderSubmissionId::new("s-v"),
            line_data,
        )
        .unwrap();
        (submission, line)
    }

    #[test]
    fn goods_service_sums_base_unit_quantities() {
        let (single, single_lines) = goods_submission(vec![goods_line_data(1, "2")]);
        assert_eq!(single.approval_total_quantity(&single_lines).unwrap(), qty("2"));

        let (multi, multi_lines) =
            goods_submission(vec![goods_line_data(1, "2.5"), goods_line_data(2, "3.25")]);
        assert_eq!(multi.approval_total_quantity(&multi_lines).unwrap(), qty("5.75"));
    }

    #[test]
    fn voucher_uses_unique_card_count_as_quantity() {
        let (submission, line) = voucher_submission(voucher_line_data(1, 3));
        assert!(line.quantity.is_none());
        assert_eq!(submission.approval_total_quantity(&[line]).unwrap(), qty("3"));
    }

    #[test]
    fn empty_missing_mixed_and_overflow_fail_closed() {
        let (goods, mut goods_lines) = goods_submission(vec![goods_line_data(1, "2")]);
        assert!(goods.approval_total_quantity(&[]).is_err());

        goods_lines[0].quantity = None;
        assert!(goods
            .approval_total_quantity(&goods_lines)
            .unwrap_err()
            .to_string()
            .contains("缺少数量"));

        let (voucher, mut voucher_line) = voucher_submission(voucher_line_data(1, 3));
        voucher_line.card_count = None;
        assert!(voucher
            .approval_total_quantity(&[voucher_line.clone()])
            .unwrap_err()
            .to_string()
            .contains("缺少卡张数"));

        voucher_line.card_count = Some(0);
        assert!(voucher
            .approval_total_quantity(&[voucher_line])
            .unwrap_err()
            .to_string()
            .contains("必须为正整数"));

        let (_, extra_voucher) = voucher_submission(voucher_line_data(1, 2));
        let (goods_for_mix, goods_mix_lines) = goods_submission(vec![goods_line_data(1, "1")]);
        assert!(goods_for_mix.approval_total_quantity(&[extra_voucher]).is_err());
        assert!(voucher.approval_total_quantity(&goods_mix_lines).is_err());

        let (overflow_submission, mut overflow_lines) =
            goods_submission(vec![goods_line_data(1, "1"), goods_line_data(2, "1")]);
        overflow_lines[0].quantity = Some(Quantity::try_from(Decimal::MAX).unwrap());
        overflow_lines[1].quantity = Some(Quantity::try_from(Decimal::MAX).unwrap());
        assert_eq!(
            overflow_submission
                .approval_total_quantity(&overflow_lines)
                .unwrap_err()
                .to_string(),
            "审批总数量溢出"
        );
    }

    #[test]
    fn quantity_from_total_rejects_excess_scale() {
        let too_precise = Decimal::new(1, 7);
        assert!(quantity_from_total(too_precise)
            .unwrap_err()
            .to_string()
            .contains("超出精度"));
    }

    #[test]
    fn required_single_line_rejects_empty_or_multiple() {
        let (_, line) = voucher_submission(voucher_line_data(1, 2));
        assert!(required_single_line(&[]).is_err());
        assert!(required_single_line(&[line.clone(), line]).is_err());
    }
}
