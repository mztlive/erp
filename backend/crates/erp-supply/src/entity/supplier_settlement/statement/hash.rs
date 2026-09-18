//! 结算主题摘要：复核主题哈希与摘要规范化。

use super::*;

impl SupplierSettlementStatement {
    /// 计算当前冻结结算事实和差异正式结论的复核主题摘要。
    ///
    /// 摘要不包含可变状态、实体版本或复核结果，因此提交复核和正式决定不会改变
    /// 同一业务主题。
    ///
    /// # 参数
    /// * `differences` - 当前结算单的全部差异
    ///
    /// # 返回
    /// 返回 64 位小写 SHA-256 十六进制摘要。
    pub fn review_subject_hash(&self, differences: &[SupplierSettlementDifference]) -> String {
        let mut parts = vec![
            "supplier-settlement-review-subject-v1".to_string(),
            self.base.id.clone(),
            self.statement_no.clone(),
            self.supplier_id.to_string(),
            self.period_start.to_string(),
            self.period_end.to_string(),
            self.period_policy_id.clone(),
            self.period_policy_version.clone(),
            self.period_timezone.clone(),
            self.external_bill_no.clone().unwrap_or_default(),
            self.external_bill_version.clone().unwrap_or_default(),
            self.erp_amount.to_string(),
            self.supplier_amount.to_string(),
            self.difference_amount.to_string(),
            self.source_as_of.unix_secs().to_string(),
            self.source_snapshot_at.unix_secs().to_string(),
            self.source_snapshot_hash.clone(),
            self.refresh_cutoff_policy_id.clone(),
            self.refresh_cutoff_policy_version.clone(),
        ];
        let mut differences = differences.iter().collect::<Vec<_>>();
        differences.sort_by(|left, right| left.base.id.cmp(&right.base.id));
        for difference in differences {
            parts.extend([
                difference.base.id.clone(),
                difference.statement_item_id.to_string(),
                difference.difference_type.as_str().to_string(),
                difference.difference_amount.to_string(),
                difference.status.as_str().to_string(),
                difference.resolution.clone().unwrap_or_default(),
                difference.resolved_by.clone().unwrap_or_default(),
                difference.resolved_at.map(|value| value.unix_secs().to_string()).unwrap_or_default(),
            ]);
        }
        digest_parts(&parts)
    }
    /// 更新差异结论变化后的主题摘要。
    ///
    /// 来源快照保持冻结；只有覆盖当前结算明细与差异正式结论的主题摘要允许推进。
    /// 待复核、已确认和已作废状态均拒绝修改。
    ///
    /// # 错误
    /// 当前状态不可编辑或摘要不是规范 SHA-256 十六进制值时返回错误。
    pub fn update_subject_hash(&mut self, subject_hash: impl Into<String>) -> Result<()> {
        if matches!(
            self.status,
            SettlementStatus::PendingReview | SettlementStatus::Confirmed | SettlementStatus::Voided
        ) {
            return Err(Error::from("当前结算状态禁止改变复核主题"));
        }
        self.subject_hash = normalize_sha256(subject_hash.into(), "主题摘要")?;
        Ok(())
    }
}

/// 对字段逐项加入长度前缀后计算稳定摘要，消除字符串拼接歧义。
pub(crate) fn digest_parts(parts: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    digest.finalize().iter().map(|byte| format!("{byte:02x}")).collect()
}

/// 规范化服务端生成的 SHA-256 十六进制摘要。
pub(crate) fn normalize_sha256(value: String, field: &str) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != HASH_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::from(format!("{field}必须是64位SHA-256十六进制摘要")));
    }
    Ok(value)
}
