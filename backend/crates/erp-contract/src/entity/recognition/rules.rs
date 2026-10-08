//! 无 I/O 校验；不猜测缺失字段或修复 OCR 错字。
use ContractField::*;
use erp_core::common::time::BusinessDate;

use super::{ContractField, ContractValues, ImportFailure, MatchedIdentity, OcrDocument};

type Result<T> = std::result::Result<T, ImportFailure>;

impl OcrDocument {
    /// 校验实际 PDF 页数与 OCR 覆盖一致。
    /// # 参数
    /// * `page_count` - 由本地 PDF 解析器独立计算的页数。
    /// # 返回
    /// 全部页面可识别时成功。
    /// # 错误
    /// 缺页、重复页、不可读页或结果过大。
    pub fn validate(&self, page_count: u32) -> Result<()> {
        if page_count == 0 || page_count > 200 || self.pages.len() != usize::try_from(page_count).unwrap_or(0)
        {
            return Err(ImportFailure::new("PAGE_COVERAGE", "合同页数不完整或超过 200 页，请检查文件"));
        }
        if self.provider.trim().is_empty()
            || self.provider.len() > 256
            || self.version.trim().is_empty()
            || self.version.len() > 256
        {
            return Err(ImportFailure::new("OCR_METADATA", "识别服务未返回版本信息，请联系管理员"));
        }
        let mut bytes = 0_usize;
        for (index, page) in self.pages.iter().enumerate() {
            bytes = bytes.saturating_add(page.text.len());
            if usize::try_from(page.number).ok() != Some(index + 1)
                || !page.readable
                || page.blank != page.text.trim().is_empty()
                || page.text.len() > 100_000
                || bytes > 2_000_000
            {
                let mut error =
                    ImportFailure::new("PAGE_UNREADABLE", "页面无法完整识别，请上传清晰、完整的合同");
                error.page = Some(page.number);
                return Err(error);
            }
        }
        Ok(())
    }
}

/// 已通过确认校验的结构化业务值。
pub struct ValidatedFields {
    pub contract_no: String,
    pub payment_code: String,
    pub payment_name: String,
    pub invoice_type: String,
    pub tax_point: String,
    pub signed_at: BusinessDate,
    pub valid_from: BusinessDate,
    pub valid_to: Option<BusinessDate>,
}

impl ContractValues {
    /// 校验确认后的合同字段，不补默认值、不修正识别错字。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 通过校验的结构化业务值。有效期止为「长期」时 `valid_to` 为 `None`。
    ///
    /// # 错误
    /// 必填缺失、开票类型或税率不在允许集合、日期无效，或有效期止早于生效日时返回 `ImportFailure`。
    pub(super) fn validate_values(&self) -> Result<ValidatedFields> {
        for field in [ContractNo, CustomerName, CompanyName, BusinessScope] {
            self.required(field)?;
        }
        let (payment_code, payment_name) = self.payment()?;
        let invoice_type = self.required(InvoiceType)?;
        if !["增值税专用发票", "增值税普通发票", "不开发票"].contains(&invoice_type) {
            return Err(self.failure(InvoiceType, "UNMATCHED_TERMS", "请选择支持的开票类型"));
        }
        let tax_point = self.required(TaxPoint)?.strip_suffix('%').unwrap_or(self.required(TaxPoint)?).trim();
        if !["0", "1", "3", "6", "9", "13"].contains(&tax_point) {
            return Err(self.failure(TaxPoint, "UNMATCHED_TAX", "请选择支持的税率"));
        }
        let signed_at = self.date(SignedAt)?;
        let valid_from = self.date(ValidFrom)?;
        let valid_to = if self.required(ValidTo)? == "长期" { None } else { Some(self.date(ValidTo)?) };
        if valid_to.is_some_and(|end| end < valid_from) {
            return Err(self.failure(ValidTo, "INVALID_DATES", "有效期止不能早于生效日期"));
        }
        Ok(ValidatedFields {
            contract_no: self.required(ContractNo)?.into(),
            payment_code,
            payment_name,
            invoice_type: invoice_type.into(),
            tax_point: tax_point.into(),
            signed_at,
            valid_from,
            valid_to,
        })
    }

    /// 读取必需字段，不补默认值。
    /// # 参数
    /// * `field` - 字段类型。
    /// # 返回
    /// 用户确认值。
    /// # 错误
    /// 必填信息缺失。
    pub fn required(&self, field: ContractField) -> Result<&str> {
        self.fields
            .get(&field)
            .map(|value| value.trim())
            .filter(|s| !s.is_empty())
            .ok_or_else(|| self.failure(field, "MISSING_FIELD", "合同缺少必需信息，请补充后确认"))
    }

    /// 把确认值解析为业务日。`年`、`月`、`/` 换成 `-`，去掉 `日`；不猜测残缺日期。
    ///
    /// # 参数
    /// * `field` - 日期字段。
    ///
    /// # 返回
    /// 解析后的 [`BusinessDate`]。
    ///
    /// # 错误
    /// 字段缺失，或不是可解析的年月日三段时返回 `ImportFailure`。
    pub(super) fn date(&self, field: ContractField) -> Result<BusinessDate> {
        let raw = self.required(field)?;
        let normalized = raw.replace(['年', '月', '/'], "-").replace('日', "");
        let parts: Vec<_> = normalized.split('-').collect();
        let parsed = (|| {
            if parts.len() != 3 {
                return None;
            }
            BusinessDate::from_ymd(parts[0].parse().ok()?, parts[1].parse().ok()?, parts[2].parse().ok()?)
        })();
        parsed.ok_or_else(|| self.failure(field, "INVALID_DATE", "日期格式无效，请填写有效日期"))
    }

    /// 把确认的付款条件匹配到固定代码与展示名。比较前去掉空白。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `(代码, 展示名)`。
    ///
    /// # 错误
    /// 付款条件缺失或不在支持列表时返回 `ImportFailure`。
    pub(super) fn payment(&self) -> Result<(String, String)> {
        let raw = self.required(PaymentTerms)?;
        let compact = raw.split_whitespace().collect::<String>();
        let terms = [
            ("先款100%", "PREPAY_100", "先款 100%"),
            ("先款50%", "PREPAY_50", "先款 50%"),
            ("先款30%", "PREPAY_30", "先款 30%"),
            ("货到15天", "POSTPAY_NET15", "货到 15 天"),
            ("货到30天", "POSTPAY_NET30", "货到 30 天"),
        ];
        terms
            .iter()
            .find(|(label, _, _)| *label == compact)
            .map(|(_, code, name)| ((*code).into(), (*name).into()))
            .ok_or_else(|| self.failure(PaymentTerms, "UNMATCHED_TERMS", "请选择支持的付款条件"))
    }

    fn failure(&self, field: ContractField, code: &str, message: &str) -> ImportFailure {
        ImportFailure { code: code.into(), message: message.into(), field: Some(field), page: None }
    }
}

/// 对有界候选执行名称和信用代码双重一致性检查，拒绝模糊或多义匹配。
/// # 参数
/// * `name` / `credit` - 用户确认的法定名称及可选信用代码。
/// * `candidates` - 当前未删除、启用主数据候选。
/// # 返回
/// 唯一身份及其版本。
/// # 错误
/// 无匹配、多匹配或信用代码冲突。
pub fn match_identity(
    name: &str,
    credit: Option<&str>,
    candidates: Vec<MatchedIdentity>,
) -> Result<MatchedIdentity> {
    let credit = credit.map(str::trim).filter(|value| !value.is_empty()).map(str::to_ascii_uppercase);
    let has_candidates = !candidates.is_empty();
    let mut matches = candidates.into_iter().filter(|item| {
        credit.as_ref().map_or_else(
            || item.legal_name.trim() == name.trim(),
            |code| item.credit_code.as_ref().is_some_and(|value| value.trim().eq_ignore_ascii_case(code)),
        )
    });
    let matched = matches.next().ok_or_else(|| {
        if credit.is_some() && has_candidates {
            ImportFailure::new("IDENTITY_CONFLICT", "合同名称与信用代码不一致，请核对合同和主数据")
        } else {
            ImportFailure::new("MASTER_NOT_FOUND", "未找到对应主体，请核对名称或确认创建客户")
        }
    })?;
    if matches.next().is_some() {
        return Err(ImportFailure::new(
            "MASTER_AMBIGUOUS",
            "名称对应多个主数据，无法自动选择，请先整理主数据",
        ));
    }
    if matched.legal_name.trim() != name.trim() {
        return Err(ImportFailure::new("IDENTITY_CONFLICT", "合同名称与信用代码不一致，请核对合同和主数据"));
    }
    Ok(matched)
}
