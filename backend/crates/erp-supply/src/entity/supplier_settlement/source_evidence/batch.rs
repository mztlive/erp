//! 不可变结算来源证据批次。

use std::collections::HashSet;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::ids::{SupplierAccountId, SupplierFulfillmentItemId};
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    ACTOR_MAX_LEN, BILL_VALUE_MAX_LEN, COMMAND_ID_MAX_LEN, EVIDENCE_REFERENCE_MAX_LEN, MAX_LINES,
    POLICY_VALUE_MAX_LEN, SettlementPeriod, SupplierSettlementSourceEvidenceLine, TIMEZONE_MAX_LEN,
    normalize_hash,
};
use crate::entity::supplier_settlement::statement_digest_parts;

/// 不可变结算来源证据批次创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceData {
    /// 客户端稳定请求 ID。
    pub request_id: String,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 期间策略冻结版本。
    pub period_policy_version: String,
    /// 期间策略时区。
    pub timezone: String,
    /// 同一供应商、期间和策略下单调递增的来源版本。
    pub source_version: u64,
    /// 外部账单号。
    pub external_bill_no: String,
    /// 外部账单版本。
    pub external_bill_version: String,
    /// 外部账单头证据引用。
    pub external_bill_evidence_reference_id: String,
    /// 逐行来源证据。
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    /// 本批次覆盖的正式事实水位。
    pub source_as_of: Instant,
    /// 记录人。
    pub recorded_by: String,
    /// 覆盖本批次全部输入与服务端派生金额的摘要。
    pub source_hash: String,
    /// 仅覆盖客户端强命令的幂等指纹。
    pub request_hash: String,
}

impl SupplierSettlementSourceEvidenceData {
    /// 计算覆盖批次身份、策略、账单与全部冻结行的稳定来源摘要。
    ///
    /// 摘要排除请求 ID、幂等键、记录人和记录时间，只覆盖可复核的业务事实；行、
    /// 事实类别和证据引用均按稳定顺序编码，调用顺序不影响结果。
    ///
    /// # 返回
    /// 返回 64 位小写 SHA-256 十六进制摘要。
    pub fn canonical_source_hash(&self) -> String {
        let mut parts = vec![
            "supplier-settlement-authoritative-source-v1".to_string(),
            self.supplier_id.to_string(),
            self.period_start.to_string(),
            self.period_end.to_string(),
            self.period_policy_id.trim().to_string(),
            self.period_policy_version.trim().to_string(),
            self.timezone.trim().to_string(),
            self.source_version.to_string(),
            self.external_bill_no.trim().to_string(),
            self.external_bill_version.trim().to_string(),
            self.external_bill_evidence_reference_id.trim().to_string(),
        ];
        let mut lines = self.lines.iter().collect::<Vec<_>>();
        lines.sort_by(|left, right| {
            left.supplier_fulfillment_item_id.as_ref().cmp(right.supplier_fulfillment_item_id.as_ref())
        });
        for line in lines {
            append_line_digest_parts(&mut parts, line);
        }
        statement_digest_parts(&parts)
    }
}

/// 不可变结算来源证据批次。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidence {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_id: String,
    pub supplier_id: SupplierAccountId,
    pub period_start: BusinessDate,
    pub period_end: BusinessDate,
    pub period_policy_id: String,
    pub period_policy_version: String,
    pub timezone: String,
    pub source_version: u64,
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub external_bill_evidence_reference_id: String,
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    pub source_as_of: Instant,
    pub recorded_by: String,
    pub source_hash: String,
    pub request_hash: String,
}

impl SupplierSettlementSourceEvidence {
    /// 校验来源命令没有重复提交同一履约明细。
    ///
    /// # 参数
    /// * `item_ids` - 客户端来源行中的履约明细主键
    ///
    /// # 返回
    /// 全部明细主键唯一时返回 `Ok(())`。
    ///
    /// # 错误
    /// 存在重复履约明细时返回领域错误。
    pub fn ensure_unique_item_ids(item_ids: &[SupplierFulfillmentItemId]) -> Result<()> {
        let mut unique = HashSet::with_capacity(item_ids.len());
        if item_ids.iter().any(|item_id| !unique.insert(item_id)) {
            return Err(Error::from("来源证据不得重复同一供应商履约明细"));
        }
        Ok(())
    }

    /// 校验候选来源版本严格高于当前批次。
    ///
    /// # 参数
    /// * `candidate` - 新来源批次声明的版本
    ///
    /// # 返回
    /// 候选版本更高时返回 `Ok(())`。
    ///
    /// # 错误
    /// 候选版本小于等于当前版本时返回领域错误。
    pub fn ensure_newer_source_version(&self, candidate: u64) -> Result<()> {
        if candidate <= self.source_version {
            return Err(Error::from(format!("来源版本必须高于当前版本 {}", self.source_version)));
        }
        Ok(())
    }

    /// 判断幂等请求指纹是否与当前不可变批次一致。
    ///
    /// # 参数
    /// * `request_hash` - 当前命令计算出的稳定指纹
    ///
    /// # 返回
    /// 指纹一致时返回 `true`。
    pub fn matches_request_hash(&self, request_hash: &str) -> bool {
        self.request_hash == request_hash
    }

    /// 创建并完整校验一个不可变来源证据批次。
    ///
    /// # 错误
    /// 身份、周期、策略、账单、行数、逐行配对唯一性或摘要不合法时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierSettlementSourceEvidenceData) -> Result<Self> {
        SettlementPeriod::new(data.period_start, data.period_end, &data.timezone)?;
        if data.source_version == 0 {
            return Err(Error::from("来源版本必须大于零"));
        }
        if data.lines.is_empty() || data.lines.len() > MAX_LINES {
            return Err(Error::from("来源证据行数必须在 1-1000 之间"));
        }
        let request_id = normalize_required_text(
            data.request_id,
            "来源证据请求ID不能为空",
            COMMAND_ID_MAX_LEN,
            "来源证据请求ID过长",
        )?;
        let period_policy_id = normalize_required_text(
            data.period_policy_id,
            "结算期间策略不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略过长",
        )?;
        let period_policy_version = normalize_required_text(
            data.period_policy_version,
            "结算期间策略版本不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略版本过长",
        )?;
        let timezone = normalize_required_text(
            data.timezone,
            "结算期间策略时区不能为空",
            TIMEZONE_MAX_LEN,
            "结算期间策略时区过长",
        )?;
        let external_bill_no = normalize_required_text(
            data.external_bill_no,
            "外部账单号不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单号过长",
        )?;
        let external_bill_version = normalize_required_text(
            data.external_bill_version,
            "外部账单版本不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单版本过长",
        )?;
        let external_bill_evidence_reference_id = normalize_required_text(
            data.external_bill_evidence_reference_id,
            "外部账单证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "外部账单证据引用过长",
        )?;
        let recorded_by = normalize_required_text(
            data.recorded_by,
            "来源证据记录人不能为空",
            ACTOR_MAX_LEN,
            "来源证据记录人过长",
        )?;
        let source_hash = normalize_hash(data.source_hash)?;
        let request_hash = normalize_hash(data.request_hash)?;
        let mut lines = data.lines;
        let mut identities = HashSet::with_capacity(lines.len());
        for line in &mut lines {
            line.validate()?;
            let identity = (
                line.supplier_fulfillment_order_id.to_string(),
                line.supplier_fulfillment_item_id.to_string(),
            );
            if !identities.insert(identity) {
                return Err(Error::from("来源证据不得重复同一供应商订单明细"));
            }
        }
        lines.sort_by(|left, right| {
            left.supplier_fulfillment_item_id.as_ref().cmp(right.supplier_fulfillment_item_id.as_ref())
        });

        Ok(Self {
            base: BaseModel::new(id.into()),
            request_id,
            supplier_id: data.supplier_id,
            period_start: data.period_start,
            period_end: data.period_end,
            period_policy_id,
            period_policy_version,
            timezone,
            source_version: data.source_version,
            external_bill_no,
            external_bill_version,
            external_bill_evidence_reference_id,
            lines,
            source_as_of: data.source_as_of,
            recorded_by,
            source_hash,
            request_hash,
        })
    }
}

/// 将一条冻结来源行按稳定字段顺序加入摘要输入。
///
/// # 参数
/// * `parts` - 待追加的长度前缀摘要字段
/// * `line` - 冻结来源证据行
///
/// # 返回
/// 无；事实类别和证据引用会先排序，保证摘要与集合输入顺序无关。
fn append_line_digest_parts(parts: &mut Vec<String>, line: &SupplierSettlementSourceEvidenceLine) {
    let mut fact_types = line.source_fact_types.iter().map(|value| value.as_str()).collect::<Vec<_>>();
    fact_types.sort_unstable();
    fact_types.dedup();
    let mut references = line.evidence_reference_ids.iter().map(String::as_str).collect::<Vec<_>>();
    references.sort_unstable();
    references.dedup();
    parts.extend([
        line.supplier_fulfillment_order_id.to_string(),
        line.supplier_fulfillment_item_id.to_string(),
        line.quantity.to_string(),
        fact_types.join(","),
        references.join(","),
        line.order_gross.to_string(),
        line.order_net.to_string(),
        line.order_tax.to_string(),
        line.freight_gross.to_string(),
        line.freight_net.to_string(),
        line.freight_tax.to_string(),
        line.service_fee_gross.to_string(),
        line.service_fee_net.to_string(),
        line.service_fee_tax.to_string(),
        line.refund_gross.to_string(),
        line.refund_net.to_string(),
        line.refund_tax.to_string(),
        line.erp_gross.to_string(),
        line.erp_net.to_string(),
        line.erp_tax.to_string(),
        line.supplier_billed_gross.to_string(),
        line.supplier_billed_net.to_string(),
        line.supplier_billed_tax.to_string(),
    ]);
}
