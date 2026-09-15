//! 供应商模板逐行导入合同；客户端负责工作簿解码，服务端负责全部业务校验。
use std::str::FromStr;

use erp_core::common::time::BusinessDate;
use erp_core::ids::PartyId;
use erp_core::money::Rate;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use super::supplier::{
    SupplierProfileAddressInput, SupplierProfileBankAccountInput, SupplierProfileContactInput,
    SupplierProfileQualificationInput, SupplierProfileRatingInput,
};
use crate::entity::supplier::{
    InvoiceType, QualificationType, ReconciliationCycle, SettlementMode, SupplierPaymentTerm, SupplierRating,
};
use crate::{Error, Result, SaveSupplierProfileRequest};

/// 每次最多 500 行，原文件中空白行不提交。
#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierImportRequest {
    pub rows: Vec<SupplierImportRow>,
}

/// 保留原 Excel 行号；业务编号采用现有生成规则，与表内旧编号分离。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SupplierImportRow {
    pub row_number: u32,
    pub cells: Vec<String>,
    pub party_no: String,
    pub supplier_no: String,
    pub effective_from: BusinessDate,
    #[serde(default)]
    pub parse_errors: Vec<String>,
}

/// 只返回业务名称、结果及错误，不回传明文账户和手机号。
#[derive(Debug, Serialize)]
pub struct SupplierImportResult {
    pub row_number: u32,
    pub name: String,
    pub status: String,
    pub message: String,
    pub supplier_id: Option<String>,
    pub supplier_no: Option<String>,
}

impl SupplierImportRow {
    /// 读取去掉前后空白的模板列。
    pub fn cell(&self, index: usize) -> &str {
        self.cells.get(index).map(String::as_str).unwrap_or("").trim()
    }

    /// 不含源文件编号的稳定导入去重键；同名供应商跨文件只创建一次。
    pub fn command_key(&self) -> String {
        let name: String = self
            .cell(1)
            .chars()
            .filter(|c| !c.is_whitespace())
            .map(|c| match c {
                '（' => '(',
                '）' => ')',
                c => c,
            })
            .collect::<String>()
            .to_lowercase();
        format!("supplier-import-v1:{}", hex::encode(Sha256::digest(name.as_bytes())))
    }

    /// 返回主体互补后的公司名称；两项都缺失时保持空值。
    pub fn company_names(&self) -> (&str, &str) {
        let signing = self.cell(7);
        let payment = self.cell(8);
        (
            if signing.is_empty() { payment } else { signing },
            if payment.is_empty() { signing } else { payment },
        )
    }

    /// 校验必填和不允许静默丢失的数据。
    ///
    /// # Errors
    /// 缺失、格式错误或无法导入的附件返回失败；调用前不写任何业务数据。
    pub fn validate(&self) -> Result<()> {
        if self.cells.len() != 23 || self.row_number < 2 {
            return invalid("模板必须包含原有 23 列并保留数据行号");
        }
        if !self.parse_errors.is_empty() {
            return invalid("工作簿包含无法安全读取的单元格，请检查预览错误");
        }
        if self.cells.iter().any(|s| s.len() > 4096) {
            return invalid("单元格内容过长");
        }
        for (index, label) in [(1, "供应商全称"), (9, "结算方式"), (18, "发票类型")] {
            if self.cell(index).is_empty() {
                return invalid(&format!("缺少{label}"));
            }
        }
        if self.company_names().0.is_empty() {
            return invalid("公司签约主体和付款主体均缺失");
        }
        for (a, b, message) in
            [(2, 3, "联系人与联系方式必须同时填写"), (4, 5, "银行账号与开户行必须同时填写")]
        {
            if self.cell(a).is_empty() != self.cell(b).is_empty() {
                return invalid(message);
            }
        }
        if [13, 14, 16, 17].iter().any(|i| !self.cell(*i).is_empty()) {
            return invalid("附件列含文件引用，请在供应商资料中通过文件上传登记，不能仅导入文件名称");
        }
        if self.cell(11).is_empty() && BusinessDate::from_str(self.cell(12)).is_ok() {
            return invalid("合同有效期已填写但缺少合同编号");
        }
        if !self.cell(15).is_empty() {
            return invalid("授权书有效期需随授权书资料登记，请先补齐该项资料");
        }
        Ok(())
    }

    /// 将完整模板行转成现有根命令，空税率不推断默认值。
    ///
    /// # Errors
    /// 必填、税率、结算或评级不符合约束时拒绝整行。
    pub fn command(&self, signing: PartyId, payment: PartyId) -> Result<SaveSupplierProfileRequest> {
        self.validate()?;
        let (settlement_mode, reconciliation_cycle, payment_term_snapshot) = import_settlement(self.cell(9))?;
        Ok(SaveSupplierProfileRequest {
            idempotency_key: self.command_key(),
            party_no: Some(self.party_no.clone()),
            supplier_no: Some(self.supplier_no.clone()),
            expected_party_version: None,
            expected_supplier_version: None,
            legal_name: self.cell(1).into(),
            short_name: None,
            unified_credit_code: None,
            contact: self.contact(),
            clear_contact: false,
            address: self.address(),
            clear_address: false,
            tax_no: None,
            clear_tax_profile: false,
            bank_account: self.bank_account(),
            clear_bank_account: false,
            settlement_mode,
            reconciliation_cycle,
            payment_term_snapshot,
            business_category: optional(self.cell(10)),
            invoice_type: import_invoice_type(self.cell(18))?,
            invoice_tax_rate: None,
            invoice_tax_rates: Some(import_tax_rates(self.cell(19))?),
            signing_entity_party_id: signing,
            payment_entity_party_id: payment,
            capability_codes: vec![],
            qualifications: self.qualifications(),
            rating: self.rating()?,
            effective_from: self.effective_from,
            change_reason: "供应商模板导入".into(),
        })
    }

    fn contact(&self) -> Option<SupplierProfileContactInput> {
        optional(self.cell(2)).map(|contact_name| SupplierProfileContactInput {
            contact_name,
            mobile: self.cell(3).into(),
            telephone: None,
            email: None,
        })
    }
    fn address(&self) -> Option<SupplierProfileAddressInput> {
        optional(self.cell(6))
            .map(|address| SupplierProfileAddressInput { address, contact_name: optional(self.cell(2)) })
    }
    fn bank_account(&self) -> Option<SupplierProfileBankAccountInput> {
        optional(self.cell(5)).map(|bank_name| SupplierProfileBankAccountInput {
            bank_name,
            account_number: self.cell(4).chars().filter(|c| !c.is_whitespace()).collect(),
        })
    }
    /// 合同编号可以没有附件；不明确的有效期保持空值。
    fn qualifications(&self) -> Vec<SupplierProfileQualificationInput> {
        if self.cell(11).is_empty() {
            return vec![];
        }
        vec![SupplierProfileQualificationInput {
            qualification_type: QualificationType::Contract,
            certificate_no: self.cell(11).into(),
            issuer: None,
            valid_from: None,
            valid_to: BusinessDate::from_str(self.cell(12)).ok(),
            attachment_id: None,
            capability_codes: vec![],
        }]
    }
    /// 不猜测缺失的评级或当前分数。
    fn rating(&self) -> Result<Option<SupplierProfileRatingInput>> {
        if (20..23).all(|i| self.cell(i).is_empty()) {
            return Ok(None);
        }
        let rating = match self.cell(21) {
            "A" => SupplierRating::A,
            "B" => SupplierRating::B,
            "C" => SupplierRating::C,
            "D" => SupplierRating::D,
            _ => return invalid("评估资料缺少有效评级（A–D）"),
        };
        let current_score =
            score(self.cell(22))?.ok_or_else(|| Error::ValidationError("评估资料缺少合作中评分".into()))?;
        Ok(Some(SupplierProfileRatingInput {
            initial_score: score(self.cell(20))?,
            rating,
            current_score,
            valid_from: self.effective_from,
        }))
    }
}

fn optional(raw: &str) -> Option<String> {
    (!raw.is_empty()).then(|| raw.to_string())
}
fn invalid<T>(message: &str) -> Result<T> {
    Err(Error::ValidationError(message.into()))
}
/// 只接受明确分值，不补零分。
fn score(raw: &str) -> Result<Option<u8>> {
    if raw.is_empty() {
        return Ok(None);
    }
    let score = raw.parse::<u8>().map_err(|_| Error::ValidationError("评分必须是 0–100 的整数".into()))?;
    if score > 100 {
        return invalid("评分必须是 0–100 的整数");
    }
    Ok(Some(score))
}

/// 模板使用百分数；Excel 原生百分比由解析端按显示语义转换。
pub fn import_tax_rates(raw: &str) -> Result<Vec<Rate>> {
    if raw.trim().is_empty() {
        return Ok(vec![]);
    }
    let mut rates = vec![];
    for part in raw.split(['、', ',', '，', ';', '；']).map(str::trim).filter(|s| !s.is_empty()) {
        let number = part.trim_end_matches(['%', '％']).trim();
        let percent = rust_decimal::Decimal::from_str(number)
            .map_err(|_| Error::ValidationError("发票税点格式无效".into()))?;
        let rate = Rate::try_from(percent / rust_decimal::Decimal::from(100))
            .map_err(|e| Error::ValidationError(e.to_string()))?;
        rates.push(rate);
    }
    if rates.is_empty() {
        return invalid("发票税点格式无效");
    }
    crate::entity::supplier::supplier_commercial_profile_revision::normalize_invoice_tax_rates(
        Some(&rates),
        None,
    )
    .map_err(|e| Error::ValidationError(e.to_string()))
}

/// 受支持模板结算名称；周期条件默认期末后 15 天。
pub fn import_settlement(raw: &str) -> Result<(SettlementMode, ReconciliationCycle, String)> {
    let (mode, cycle, code) = match raw {
        "周结" => (SettlementMode::Weekly, ReconciliationCycle::Weekly, "PERIOD_WEEK_15"),
        "月结" | "代发月结" => {
            (SettlementMode::Monthly, ReconciliationCycle::Monthly, "PERIOD_MONTH_15")
        },
        "季结" => (SettlementMode::Quarterly, ReconciliationCycle::Quarterly, "PERIOD_QUARTER_15"),
        "半年结" => (SettlementMode::HalfYearly, ReconciliationCycle::HalfYearly, "PERIOD_HALF_YEAR_15"),
        "年结" => (SettlementMode::Yearly, ReconciliationCycle::Yearly, "PERIOD_YEAR_15"),
        "现结" => (SettlementMode::CashSettlement, ReconciliationCycle::None, "CASH_ON_APPROVAL"),
        "预付款" => (SettlementMode::Prepayment, ReconciliationCycle::None, "PREPAY_100"),
        _ => return invalid("结算方式无法识别，请使用预付款、现结、周结、月结、季结、半年结或年结"),
    };
    let term = SupplierPaymentTerm::parse(code).map_err(|e| Error::ValidationError(e.to_string()))?;
    Ok((mode, cycle, term.code()))
}

fn import_invoice_type(raw: &str) -> Result<InvoiceType> {
    match raw {
        "专票" | "增值税专用发票" => Ok(InvoiceType::VatSpecial),
        "普票" | "增值税普通发票" => Ok(InvoiceType::VatNormal),
        "电子发票" => Ok(InvoiceType::Electronic),
        _ => invalid("发票类型无法识别"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn taxes_preserve_unknown_and_multiple_rates() {
        assert!(import_tax_rates("").unwrap().is_empty());
        assert_eq!(
            import_tax_rates("9%，13%,9%").unwrap().iter().map(ToString::to_string).collect::<Vec<_>>(),
            vec!["0.09", "0.13"]
        );
        assert!(import_tax_rates("错误").is_err());
        assert!(import_tax_rates("100%").is_err());
    }
    #[test]
    fn periods_have_explicit_default_terms() {
        assert_eq!(import_settlement("月结").unwrap().2, "PERIOD_MONTH_15");
        assert!(import_settlement("").is_err());
    }
    fn complete_row() -> SupplierImportRow {
        let mut cells = vec![String::new(); 23];
        for (index, value) in [(1, "示例供应商（上海）有限公司"), (7, "示例公司"), (9, "月结"), (18, "专票")]
        {
            cells[index] = value.into();
        }
        SupplierImportRow {
            row_number: 2,
            cells,
            party_no: "PTY-test".into(),
            supplier_no: "SUP-test".into(),
            effective_from: BusinessDate::from_ymd(2026, 9, 10).unwrap(),
            parse_errors: vec![],
        }
    }
    #[test]
    fn complete_row_keeps_optional_data_empty_and_complements_company() {
        let row = complete_row();
        assert_eq!(row.company_names(), ("示例公司", "示例公司"));
        let command = row.command(PartyId::new("signing"), PartyId::new("payment")).unwrap();
        assert_eq!(command.supplier_no.as_deref(), Some("SUP-test"));
        assert!(command.invoice_tax_rates.unwrap().is_empty());
        assert!(command.contact.is_none());
        assert!(command.rating.is_none());
        let mut reverse = row.clone();
        reverse.cells.swap(7, 8);
        assert_eq!(reverse.company_names(), row.company_names());
        reverse.cells[1] = " 示例供应商 (上海) 有限公司 ".into();
        reverse.cells[0] = "不同旧编号".into();
        assert_eq!(reverse.command_key(), row.command_key());
    }
    #[test]
    fn incomplete_rows_and_unreadable_cells_are_rejected_without_defaults() {
        for index in [1, 7, 9, 18] {
            let mut row = complete_row();
            row.cells[index].clear();
            assert!(row.validate().is_err());
        }
        for index in [2, 3, 4, 5, 13, 14, 15, 16, 17] {
            let mut row = complete_row();
            row.cells[index] = "仅填写一项".into();
            assert!(row.validate().is_err());
        }
        let mut row = complete_row();
        row.parse_errors.push("公式".into());
        assert!(row.validate().is_err());
        assert!(import_tax_rates("，、").is_err());
    }
    #[test]
    fn ambiguous_contract_expiry_is_blank_but_clear_orphan_date_fails() {
        let mut row = complete_row();
        row.cells[11] = "合同-1".into();
        row.cells[12] = "一年".into();
        assert!(row.qualifications()[0].valid_from.is_none());
        assert!(row.qualifications()[0].valid_to.is_none());
        row.cells[12] = "2027-12-31".into();
        assert_eq!(row.qualifications()[0].valid_to.unwrap().to_string(), "2027-12-31");
        row.cells[12] = "2025-12-31".into();
        assert!(row.validate().is_ok());
        assert_eq!(row.qualifications()[0].valid_to.unwrap().to_string(), "2025-12-31");
        row.cells[11].clear();
        assert!(row.validate().is_err());
    }
}
