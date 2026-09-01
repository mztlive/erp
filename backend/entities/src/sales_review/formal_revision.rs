//! 由销售变更提交构造正式销售版本聚合。
//!
//! 变更路径只负责 D14→D13 字段组转换，公共行、快照和卡券单行约束复用销售单域工厂。

use crate::errors::{Error, Result};
use crate::sales_order::formal_revision::{
    submission_content_hash, FormalRevisionHeader, PreparedRevisionLine,
};
use crate::sales_order::{
    FormalRevisionContext, FormalRevisionIdentities, LineType as SalesLineType, SalesOrderRevisionAggregate,
};

use super::sales_change_submission::{SalesChangeSubmission, SalesChangeSubmissionLine};
use super::types::{BusinessType, LineType};

impl FormalRevisionHeader {
    /// 从销售变更提交复制表头快照、金额和内容指纹。
    ///
    /// # 参数
    /// * `submission` - 已冻结变更提交
    ///
    /// # 返回
    /// 返回销售单域正式版本表头输入。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 指纹与首次提交共用 `sub:{id}` 形态，来源提交主键为变更提交 ID。
    fn from_sales_change_submission(submission: &SalesChangeSubmission) -> Self {
        Self {
            sales_order_id: submission.sales_order_id.clone(),
            content_hash: submission_content_hash(&submission.base.id),
            contract_revision_id: submission.contract_revision_id.clone(),
            snapshot: submission.sales_order_header_snapshot(),
            project_name: submission.project_name.clone(),
            business_remark: submission.business_remark.clone(),
            voucher_category_sku_id: submission.voucher_category_sku_id.clone(),
            voucher_expiry_at: submission.voucher_expiry_at,
            gross_amount: submission.gross_amount,
            net_amount: submission.net_amount,
            tax_amount: submission.tax_amount,
        }
    }
}

impl PreparedRevisionLine {
    /// 从变更提交行还原销售单域正式版本行输入。
    ///
    /// # 参数
    /// * `line` - 已冻结变更提交行
    ///
    /// # 返回
    /// 返回已转换为 D13 字段组的行输入。
    ///
    /// # 错误
    /// 行类型与字段组不一致或必填字段缺失时返回领域错误。
    ///
    /// # 关键业务约束
    /// 必须通过变更提交行 `goods_fields`/`voucher_fields` 还原，禁止再拆 Optional。
    fn from_sales_change_submission_line(line: &SalesChangeSubmissionLine) -> Result<Self> {
        let line_type = SalesLineType::from(line.line_type);
        let (goods, voucher) = match line.line_type {
            LineType::GoodsService => (Some(line.goods_fields()?.into()), None),
            LineType::Voucher => (None, Some(line.voucher_fields()?.into())),
        };
        Ok(Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type,
            gross_amount: line.gross_amount,
            net_amount: line.net_amount,
            tax_amount: line.tax_amount,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods,
            voucher,
        })
    }
}

impl SalesOrderRevisionAggregate {
    /// 由销售变更提交构造正式版本聚合。
    ///
    /// # 参数
    /// * `identities` - 调用方注入的版本与行身份
    /// * `context` - 版本号、来源、上一版本与业务性质
    /// * `submission` - 已冻结变更提交
    /// * `lines` - 该提交的全部目标明细行
    ///
    /// # 返回
    /// 返回与首次形式化共用规则的正式版本聚合。
    ///
    /// # 错误
    /// 业务性质漂移、空行、混合行、多卡券行、字段组缺失、身份不符或金额/快照
    /// 不合法时返回领域错误。
    ///
    /// # 关键业务约束
    /// 本方法不得查询 latest revision no；版本号必须由调用方传入。
    pub fn from_sales_change_submission(
        identities: FormalRevisionIdentities,
        context: FormalRevisionContext,
        submission: &SalesChangeSubmission,
        lines: &[SalesChangeSubmissionLine],
    ) -> Result<Self> {
        ensure_change_business_type(context.business_type, submission.business_type)?;
        let prepared = lines
            .iter()
            .map(PreparedRevisionLine::from_sales_change_submission_line)
            .collect::<Result<Vec<_>>>()?;
        Self::from_prepared(
            identities,
            context,
            FormalRevisionHeader::from_sales_change_submission(submission),
            prepared,
        )
    }
}

/// 校验销售单业务性质与变更提交业务性质一致。
///
/// # 参数
/// * `expected` - 销售单业务性质
/// * `actual` - 变更提交业务性质
///
/// # 返回
/// 转换后一致时返回 `Ok(())`。
///
/// # 错误
/// 不一致时返回领域错误。
///
/// # 关键业务约束
/// 变更提交不得把卡券单改成实物单，也不得反向漂移。
fn ensure_change_business_type(
    expected: crate::sales_order::BusinessType,
    actual: BusinessType,
) -> Result<()> {
    if expected != crate::sales_order::BusinessType::from(actual) {
        return Err(Error::from("销售单业务性质与提交不一致"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;
    use crate::common::time::Instant;
    use crate::ids::{
        ContractRevisionId, CustomerAccountId, PartyId, SalesChangeOrderId, SalesChangeSubmissionId,
        SalesChangeSubmissionLineId, SalesOrderId, SalesOrderLineId, SalesOrderRevisionId,
        SalesOrderRevisionLineId, SalesOrderWorkingCopyId, SkuId, SkuRevisionId,
    };
    use crate::money::{Amount, Quantity, Rate, UnitPrice};
    use crate::sales_order::{FormalRevisionLineIdentity, FormalRevisionSubtypeIdentity, RevisionSource};
    use crate::sales_review::{
        CardForm, GoodsLineFields, SalesChangeSubmissionData, SalesChangeSubmissionLineData,
        VoucherLineDraft, WelfareScenario,
    };

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

    fn at() -> Instant {
        Instant::from_unix_secs(1_800_000_000)
    }

    fn goods_fields() -> GoodsLineFields {
        GoodsLineFields {
            sku_id: SkuId::new("sku-1"),
            sku_revision_id: SkuRevisionId::new("skurev-1"),
            welfare_scenario: Some(WelfareScenario::MealSubsidy),
            service_region: Some("east".to_string()),
            fulfillment_due_at: at(),
            quantity: qty("3.000000"),
            base_unit_code: "箱".to_string(),
            unit_price_gross: price("9.9900"),
        }
    }

    fn goods_line_data(line_no: u32) -> SalesChangeSubmissionLineData {
        SalesChangeSubmissionLineData {
            sales_order_line_id: SalesOrderLineId::new(format!("line-{line_no}")),
            line_no,
            line_type: LineType::GoodsService,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "年货礼盒".to_string(),
            spec_snapshot: Some("10kg".to_string()),
            unit_snapshot: Some("箱".to_string()),
            goods: Some(goods_fields()),
            voucher: None,
        }
    }

    fn voucher_line_data(line_no: u32) -> SalesChangeSubmissionLineData {
        SalesChangeSubmissionLineData {
            line_type: LineType::Voucher,
            goods: None,
            voucher: Some(VoucherLineDraft {
                face_value: amt("100.00"),
                card_count: 3,
                unit_price_gross: price("90.0000"),
                face_value_total: amt("300.00"),
                transaction_amount: amt("270.00"),
                gift_amount: amt("30.00"),
                gift_rate: None,
                card_form: CardForm::Physical,
            }),
            ..goods_line_data(line_no)
        }
    }

    fn goods_header(lines: Vec<SalesChangeSubmissionLineData>) -> SalesChangeSubmissionData {
        SalesChangeSubmissionData {
            sales_change_order_id: SalesChangeOrderId::new("co-1"),
            submission_no: 1,
            base_revision_id: SalesOrderRevisionId::new("rev-1"),
            sales_order_id: SalesOrderId::new("o-1"),
            working_copy_id: SalesOrderWorkingCopyId::new("wc-1"),
            working_copy_version: 5,
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: crate::sales_review::HeaderSnapshotData {
                customer_name: "东方企业".to_string(),
                contract_no: Some("HT-2026-0088".to_string()),
                settlement_party_name: Some("集团结算中心".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: Some("端午福利项目".to_string()),
            business_remark: Some("变更后执行".to_string()),
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            submitted_at: at(),
            submitted_by: "sales-1".to_string(),
            lines,
        }
    }

    fn voucher_header(lines: Vec<SalesChangeSubmissionLineData>) -> SalesChangeSubmissionData {
        SalesChangeSubmissionData {
            business_type: BusinessType::Voucher,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            gross_amount: amt("270.00"),
            net_amount: amt("238.94"),
            tax_amount: amt("31.06"),
            lines,
            ..goods_header(vec![goods_line_data(1)])
        }
    }

    fn submission_and_lines(
        data: SalesChangeSubmissionData,
    ) -> (SalesChangeSubmission, Vec<SalesChangeSubmissionLine>) {
        let line_data = data.lines.clone();
        let submission = SalesChangeSubmission::new(SalesChangeSubmissionId::new("cs-1"), data).unwrap();
        let lines = line_data
            .into_iter()
            .enumerate()
            .map(|(index, data)| {
                SalesChangeSubmissionLine::new(
                    SalesChangeSubmissionLineId::new(format!("csl-{index}")),
                    SalesChangeSubmissionId::new("cs-1"),
                    data,
                )
                .unwrap()
            })
            .collect();
        (submission, lines)
    }

    fn identities_for(line_types: &[SalesLineType]) -> FormalRevisionIdentities {
        FormalRevisionIdentities::new(
            SalesOrderRevisionId::new("rev-new"),
            line_types
                .iter()
                .enumerate()
                .map(|(index, line_type)| {
                    FormalRevisionLineIdentity::new(
                        SalesOrderRevisionLineId::new(format!("rl-{index}")),
                        FormalRevisionSubtypeIdentity::from_line_type(*line_type, format!("st-{index}")),
                    )
                })
                .collect(),
        )
    }

    #[test]
    fn change_goods_revision_reuses_shared_snapshot_and_hash_rules() {
        let (submission, lines) = submission_and_lines(goods_header(vec![goods_line_data(1)]));
        let aggregate = SalesOrderRevisionAggregate::from_sales_change_submission(
            identities_for(&[SalesLineType::GoodsService]),
            FormalRevisionContext::new(
                4,
                RevisionSource::SalesChange,
                Some(SalesOrderRevisionId::new("rev-1")),
                crate::sales_order::BusinessType::GoodsService,
                at(),
            ),
            &submission,
            &lines,
        )
        .unwrap();

        assert_eq!(aggregate.revision.revision.revision_no, 4);
        assert_eq!(aggregate.revision.revision_source, RevisionSource::SalesChange);
        assert_eq!(
            aggregate.revision.previous_revision_id,
            Some(SalesOrderRevisionId::new("rev-1"))
        );
        assert_eq!(aggregate.revision.content_hash, "sub:cs-1");
        assert_eq!(aggregate.revision.customer_snapshot.customer_name, "东方企业");
        assert_eq!(aggregate.revision.business_remark.as_deref(), Some("变更后执行"));
        assert_eq!(aggregate.lines.len(), 1);
        assert_eq!(aggregate.goods_lines.len(), 1);
        assert!(aggregate.voucher_lines.is_empty());
        assert_eq!(
            aggregate.goods_lines[0].welfare_scenario,
            Some(crate::sales_order::WelfareScenario::MealSubsidy)
        );
        assert_eq!(aggregate.lines[0].gross_amount, amt("29.97"));
        assert_eq!(aggregate.revision.gross_amount, aggregate.lines[0].gross_amount);
    }

    #[test]
    fn change_voucher_revision_keeps_card_form_and_single_line() {
        let (submission, lines) = submission_and_lines(voucher_header(vec![voucher_line_data(1)]));
        let aggregate = SalesOrderRevisionAggregate::from_sales_change_submission(
            identities_for(&[SalesLineType::Voucher]),
            FormalRevisionContext::new(
                2,
                RevisionSource::SalesChange,
                Some(SalesOrderRevisionId::new("rev-1")),
                crate::sales_order::BusinessType::Voucher,
                at(),
            ),
            &submission,
            &lines,
        )
        .unwrap();

        assert_eq!(aggregate.voucher_lines.len(), 1);
        assert_eq!(
            aggregate.voucher_lines[0].card_form,
            crate::sales_order::CardForm::Physical
        );
        assert_eq!(aggregate.voucher_lines[0].card_count, 3);
        assert!(aggregate.goods_lines.is_empty());
    }

    #[test]
    fn change_factory_rejects_business_type_drift() {
        let (submission, lines) = submission_and_lines(goods_header(vec![goods_line_data(1)]));
        assert!(SalesOrderRevisionAggregate::from_sales_change_submission(
            identities_for(&[SalesLineType::GoodsService]),
            FormalRevisionContext::new(
                2,
                RevisionSource::SalesChange,
                None,
                crate::sales_order::BusinessType::Voucher,
                at(),
            ),
            &submission,
            &lines,
        )
        .is_err());
    }
}
