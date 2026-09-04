//! 工作副本向提交快照的字段、快照、金额和字段组复制。
//!
//! ID 生成、外部身份解析、提交序号与提交审计由调用方注入；本模块不依赖
//! `services::dto` 或 `id-generator`。

use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::SalesOrderWorkingCopyId;

use super::snapshot::HeaderSnapshotData;
use super::submission::{SalesOrderSubmissionData, SalesOrderSubmissionLineData};
use super::working_copy::SalesOrderWorkingCopy;
use super::working_copy_line::SalesOrderWorkingCopyLine;

impl SalesOrderWorkingCopy {
    /// 将已规范化的表头快照还原为提交构造入参。
    ///
    /// # 返回
    /// 返回与当前工作副本快照字段一一对应的表头入参。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 关键业务约束
    /// 只复制已规范化快照，不再次 trim。
    pub fn header_snapshot_data(&self) -> HeaderSnapshotData {
        HeaderSnapshotData {
            customer_name: self.customer_snapshot.customer_name.clone(),
            contract_no: self
                .contract_snapshot
                .as_ref()
                .map(|snapshot| snapshot.contract_no.clone()),
            settlement_party_name: self
                .settlement_party_snapshot
                .as_ref()
                .map(|snapshot| snapshot.settlement_party_name.clone()),
            payment_term_code: self.payment_term_snapshot.payment_term_code.clone(),
            payment_term_name: self.payment_term_snapshot.payment_term_name.clone(),
            invoice_type: self.invoice_requirement_snapshot.invoice_type.clone(),
            tax_point: self.invoice_requirement_snapshot.tax_point.clone(),
        }
    }
}

impl SalesOrderSubmissionLineData {
    /// 从工作副本行复制提交行字段组与快照。
    ///
    /// # 参数
    /// * `line` - 已冻结的工作副本行
    ///
    /// # 返回
    /// 返回提交行创建数据；金额由提交行 `new` 按字段组重算。
    ///
    /// # 错误
    /// 行类型对应字段组缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 调用方必须保持输入行顺序；本方法不排序、不生成行 ID。
    pub fn from_working_copy_line(line: &SalesOrderWorkingCopyLine) -> Result<Self> {
        Ok(Self {
            sales_order_line_id: line.sales_order_line_id.clone(),
            line_no: line.line_no,
            line_type: line.line_type,
            sales_tax_rate: line.sales_tax_rate,
            item_name_snapshot: line.item_name_snapshot.clone(),
            spec_snapshot: line.spec_snapshot.clone(),
            unit_snapshot: line.unit_snapshot.clone(),
            goods: line.goods_fields()?,
            voucher: line.voucher_fields()?,
        })
    }
}

impl SalesOrderSubmissionData {
    /// 从工作副本头、行复制提交快照创建数据。
    ///
    /// # 参数
    /// * `working_copy` - 已迁移到可提交状态的工作副本
    /// * `lines` - 工作副本行（顺序即提交行顺序）
    /// * `submission_no` - 调用方读取的下一提交序号
    /// * `submitted_at` - 调用方注入的提交时间
    /// * `submitted_by` - 调用方注入的提交人
    ///
    /// # 返回
    /// 返回提交头创建数据（含行创建数据）。
    ///
    /// # 错误
    /// 任一行字段组缺失时返回错误。
    ///
    /// # 关键业务约束
    /// 金额来自行实体已舍入合计；ID 与事务写入仍由 Service 负责。
    pub fn from_working_copy(
        working_copy: &SalesOrderWorkingCopy,
        lines: &[SalesOrderWorkingCopyLine],
        submission_no: u32,
        submitted_at: Instant,
        submitted_by: impl Into<String>,
    ) -> Result<Self> {
        let (gross_amount, net_amount, tax_amount) = SalesOrderWorkingCopyLine::amount_totals(lines);
        let line_datas = lines
            .iter()
            .map(SalesOrderSubmissionLineData::from_working_copy_line)
            .collect::<Result<Vec<_>>>()?;
        Ok(Self {
            sales_order_id: working_copy.sales_order_id.clone(),
            submission_no,
            working_copy_id: SalesOrderWorkingCopyId::new(working_copy.base.id.clone()),
            working_copy_version: working_copy.draft_version,
            business_type: working_copy.business_type,
            customer_id: working_copy.customer_id.clone(),
            contract_revision_id: working_copy.contract_revision_id.clone(),
            settlement_party_id: working_copy.settlement_party_id.clone(),
            snapshot: working_copy.header_snapshot_data(),
            project_name: working_copy.project_name.clone(),
            business_remark: working_copy.business_remark.clone(),
            voucher_category_sku_id: working_copy.voucher_category_sku_id.clone(),
            voucher_expiry_at: working_copy.voucher_expiry_at,
            receivable_due_date: working_copy.receivable_due_date,
            gross_amount,
            net_amount,
            tax_amount,
            submitted_at,
            submitted_by: submitted_by.into(),
            lines: line_datas,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::types::{BusinessType, CardForm, LineType, VoucherLineDraft};
    use super::super::working_copy::{SalesOrderWorkingCopyData, WorkingPurpose};
    use super::super::working_copy_line::SalesOrderWorkingCopyLineData;
    use super::super::working_copy_test_support::{amt, line_data, price, rate};
    use super::*;
    use crate::common::time::{BusinessDate, Instant};
    use crate::ids::{
        ContractId, ContractRevisionId, CustomerAccountId, PartyId, SalesOrderId, SalesOrderLineId,
        SalesOrderWorkingCopyId, SalesOrderWorkingCopyLineId, SkuId,
    };

    fn goods_copy() -> (SalesOrderWorkingCopy, Vec<SalesOrderWorkingCopyLine>) {
        let data = SalesOrderWorkingCopyData {
            sales_order_id: SalesOrderId::new("o-1"),
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version: 3,
            content_hash: "draft:wc-1:3".to_string(),
            editor_user_id: "user-1".to_string(),
            business_type: BusinessType::GoodsService,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: Some(ContractId::new("contract-1")),
            contract_revision_id: Some(ContractRevisionId::new("contract-rev-1")),
            settlement_party_id: PartyId::new("party-1"),
            snapshot: HeaderSnapshotData {
                customer_name: "东方企业".to_string(),
                contract_no: Some("HT-2026-0088".to_string()),
                settlement_party_name: Some("集团结算中心".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: Some("端午福利项目".to_string()),
            business_remark: Some("按合同执行".to_string()),
            voucher_category_sku_id: None,
            voucher_expiry_at: None,
            receivable_due_date: None,
            gross_amount: amt("29.97"),
            net_amount: amt("26.07"),
            tax_amount: amt("3.90"),
            lines: vec![line_data(1)],
        };
        let copy = SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-1"), data, "admin-1").unwrap();
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();
        (copy, vec![line])
    }

    fn assert_line_copied_from_working_copy(
        source: &SalesOrderWorkingCopyLine,
        copied: &SalesOrderSubmissionLineData,
    ) {
        assert_eq!(copied.sales_order_line_id, source.sales_order_line_id);
        assert_eq!(copied.line_no, source.line_no);
        assert_eq!(copied.line_type, source.line_type);
        assert_eq!(copied.sales_tax_rate, source.sales_tax_rate);
        assert_eq!(copied.item_name_snapshot, source.item_name_snapshot);
        assert_eq!(copied.spec_snapshot, source.spec_snapshot);
        assert_eq!(copied.unit_snapshot, source.unit_snapshot);
        assert_eq!(copied.goods, source.goods_fields().unwrap());
        assert_eq!(copied.voucher, source.voucher_fields().unwrap());
    }

    fn voucher_line_data() -> SalesOrderWorkingCopyLineData {
        SalesOrderWorkingCopyLineData {
            sales_order_line_id: SalesOrderLineId::new("line-1"),
            line_no: 1,
            line_type: LineType::Voucher,
            sales_tax_rate: rate("0.130000"),
            item_name_snapshot: "福利卡".to_string(),
            spec_snapshot: None,
            unit_snapshot: Some("张".to_string()),
            goods: None,
            voucher: Some(VoucherLineDraft {
                face_value: amt("100.00"),
                card_count: 3,
                unit_price_gross: price("90.0000"),
                face_value_total: amt("300.00"),
                transaction_amount: amt("270.00"),
                gift_amount: amt("30.00"),
                gift_rate: None,
                card_form: CardForm::Electronic,
            }),
        }
    }

    #[test]
    fn goods_service_copies_snapshot_amounts_and_line_fields() {
        let (copy, lines) = goods_copy();
        let submitted_at = Instant::from_unix_secs(1_790_000_000);
        let data = SalesOrderSubmissionData::from_working_copy(
            &copy,
            &lines,
            2,
            submitted_at,
            "sales-1",
        )
        .unwrap();

        assert_eq!(data.sales_order_id, SalesOrderId::new("o-1"));
        assert_eq!(data.submission_no, 2);
        assert_eq!(data.working_copy_id, SalesOrderWorkingCopyId::new("wc-1"));
        assert_eq!(data.working_copy_version, 3);
        assert_eq!(data.business_type, BusinessType::GoodsService);
        assert_eq!(data.snapshot.customer_name, "东方企业");
        assert_eq!(data.snapshot.contract_no.as_deref(), Some("HT-2026-0088"));
        assert_eq!(data.project_name.as_deref(), Some("端午福利项目"));
        assert_eq!(data.gross_amount, amt("29.97"));
        assert_eq!(data.net_amount, amt("26.07"));
        assert_eq!(data.tax_amount, amt("3.90"));
        assert_eq!(data.submitted_at, submitted_at);
        assert_eq!(data.submitted_by, "sales-1");
        assert_eq!(data.lines.len(), 1);
        assert_line_copied_from_working_copy(&lines[0], &data.lines[0]);
        let goods = data.lines[0].goods.as_ref().expect("实物行必须复制商品字段组");
        assert_eq!(goods.sku_id, lines[0].sku_id.clone().unwrap());
        assert_eq!(goods.quantity, lines[0].quantity.unwrap());
        assert_eq!(goods.unit_price_gross, lines[0].unit_price_gross.unwrap());
        assert!(data.lines[0].voucher.is_none());
    }

    #[test]
    fn voucher_copies_card_fields() {
        let line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-v"),
            SalesOrderWorkingCopyId::new("wc-v"),
            voucher_line_data(),
        )
        .unwrap();
        let data = SalesOrderWorkingCopyData {
            sales_order_id: SalesOrderId::new("o-v"),
            working_purpose: WorkingPurpose::FirstSubmission,
            sales_change_order_id: None,
            base_revision_id: None,
            draft_version: 1,
            content_hash: "draft:wc-v:1".to_string(),
            editor_user_id: "user-1".to_string(),
            business_type: BusinessType::Voucher,
            customer_id: CustomerAccountId::new("cust-1"),
            contract_id: None,
            contract_revision_id: None,
            settlement_party_id: PartyId::new("party-1"),
            snapshot: HeaderSnapshotData {
                customer_name: "东方企业".to_string(),
                contract_no: None,
                settlement_party_name: Some("集团结算中心".to_string()),
                payment_term_code: "NET30".to_string(),
                payment_term_name: "月结 30 天".to_string(),
                invoice_type: "增值税专用发票".to_string(),
                tax_point: "6".to_string(),
            },
            project_name: None,
            business_remark: None,
            voucher_category_sku_id: Some(SkuId::new("vcat-1")),
            voucher_expiry_at: Some(Instant::from_unix_secs(1_850_000_000)),
            receivable_due_date: Some(BusinessDate::from_ymd(2026, 10, 31).unwrap()),
            gross_amount: amt("270.00"),
            net_amount: amt("238.94"),
            tax_amount: amt("31.06"),
            lines: vec![voucher_line_data()],
        };
        let copy = SalesOrderWorkingCopy::new(SalesOrderWorkingCopyId::new("wc-v"), data, "admin-1").unwrap();
        let submission = SalesOrderSubmissionData::from_working_copy(
            &copy,
            std::slice::from_ref(&line),
            1,
            Instant::from_unix_secs(1_790_000_000),
            "sales-1",
        )
        .unwrap();

        assert_eq!(submission.business_type, BusinessType::Voucher);
        assert_eq!(submission.snapshot.contract_no, None);
        assert_eq!(submission.voucher_category_sku_id, Some(SkuId::new("vcat-1")));
        assert_line_copied_from_working_copy(&line, &submission.lines[0]);
        let voucher = submission.lines[0]
            .voucher
            .as_ref()
            .expect("卡券行必须复制卡券字段组");
        assert_eq!(voucher.face_value, line.face_value.unwrap());
        assert_eq!(voucher.card_count, line.card_count.unwrap());
        assert_eq!(voucher.card_form, line.card_form.unwrap());
        assert_eq!(voucher.gift_amount, line.gift_amount.unwrap());
        assert_eq!(voucher.unit_price_gross, line.unit_price_gross.unwrap());
        assert_eq!(voucher.face_value_total, line.face_value_total.unwrap());
        assert_eq!(voucher.transaction_amount, line.transaction_amount.unwrap());
        assert!(submission.lines[0].goods.is_none());
    }

    #[test]
    fn line_order_follows_input_not_line_no() {
        let first = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-2"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(2),
        )
        .unwrap();
        let second = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();
        let converted = [
            SalesOrderSubmissionLineData::from_working_copy_line(&first).unwrap(),
            SalesOrderSubmissionLineData::from_working_copy_line(&second).unwrap(),
        ];
        assert_eq!(converted[0].line_no, 2);
        assert_eq!(converted[1].line_no, 1);
        assert_eq!(converted[0].sales_order_line_id, SalesOrderLineId::new("line-2"));
        assert_line_copied_from_working_copy(&first, &converted[0]);
        assert_line_copied_from_working_copy(&second, &converted[1]);
    }

    #[test]
    fn missing_field_group_fails_closed() {
        let mut line = SalesOrderWorkingCopyLine::new(
            SalesOrderWorkingCopyLineId::new("wcl-1"),
            SalesOrderWorkingCopyId::new("wc-1"),
            line_data(1),
        )
        .unwrap();
        line.sku_id = None;
        assert_eq!(
            SalesOrderSubmissionLineData::from_working_copy_line(&line)
                .unwrap_err()
                .to_string(),
            "第 1 行缺少商品字段组"
        );

        let mut voucher = line;
        voucher.line_type = LineType::Voucher;
        voucher.face_value = None;
        assert_eq!(
            SalesOrderSubmissionLineData::from_working_copy_line(&voucher)
                .unwrap_err()
                .to_string(),
            "第 1 行缺少卡券字段组"
        );
    }
}
