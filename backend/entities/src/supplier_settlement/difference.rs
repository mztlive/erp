//! `supplier_settlement_difference`（数据模型 §6.20 供应商结算差异）。
//!
//! 对账只生成差异，不直接修正式事实（§9.4）；未解决差异不得直接修改供应商订单或原
//! 成本（§6.20，P3 校验）。差异类型与状态是固定枚举（§4.6、§13.3）；处理结果三元组
//! （`resolution`/`resolved_by`/`resolved_at`）成组出现，待处理不得填写，已补偿/已关闭
//! 必填。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierSettlementDifferenceId, SupplierSettlementItemId};
use crate::money::Amount;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 处理结果文本最大长度。
const RESOLUTION_MAX_LEN: usize = 512;
/// 处理人标识最大长度。
const ACTOR_MAX_LEN: usize = 128;

/// 结算差异类型（数据模型 §6.20：漏单、重复、金额、退款、状态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceType {
    /// 漏单。
    Missing,
    /// 重复。
    Duplicate,
    /// 金额。
    Amount,
    /// 退款。
    Refund,
    /// 状态。
    Status,
}

impl SettlementDifferenceType {
    /// 返回差异类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Missing => "漏单",
            Self::Duplicate => "重复",
            Self::Amount => "金额",
            Self::Refund => "退款",
            Self::Status => "状态",
        }
    }

    /// 返回差异类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Missing => "MISSING",
            Self::Duplicate => "DUPLICATE",
            Self::Amount => "AMOUNT",
            Self::Refund => "REFUND",
            Self::Status => "STATUS",
        }
    }
}

/// 结算差异状态（数据模型 §6.20：待处理、供应商认可、ERP 认可、已补偿、关闭）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementDifferenceStatus {
    /// 待处理。
    Pending,
    /// 供应商认可。
    SupplierAcknowledged,
    /// ERP 认可。
    ErpAcknowledged,
    /// 已补偿。
    Compensated,
    /// 关闭。
    Closed,
}

impl SettlementDifferenceStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::SupplierAcknowledged => "供应商认可",
            Self::ErpAcknowledged => "ERP 认可",
            Self::Compensated => "已补偿",
            Self::Closed => "关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::SupplierAcknowledged => "SUPPLIER_ACKNOWLEDGED",
            Self::ErpAcknowledged => "ERP_ACKNOWLEDGED",
            Self::Compensated => "COMPENSATED",
            Self::Closed => "CLOSED",
        }
    }
}

/// 结算差异正式结论类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SettlementDifferenceConclusionKind {
    /// 供应商接受 ERP 口径。
    SupplierAccepted,
    /// ERP 接受供应商账单口径。
    ErpAccepted,
    /// 已通过独立补偿事实处理。
    Compensated,
    /// 有正式证据证明无需金额调整并关闭。
    ClosedNoAdjustment,
}

impl SettlementDifferenceConclusionKind {
    /// 返回结论对应的持久化差异状态。
    ///
    /// # 返回
    /// 返回供应商认可、ERP 认可、已补偿或已关闭状态。
    pub fn status(self) -> SettlementDifferenceStatus {
        match self {
            Self::SupplierAccepted => SettlementDifferenceStatus::SupplierAcknowledged,
            Self::ErpAccepted => SettlementDifferenceStatus::ErpAcknowledged,
            Self::Compensated => SettlementDifferenceStatus::Compensated,
            Self::ClosedNoAdjustment => SettlementDifferenceStatus::Closed,
        }
    }

    /// 返回审计与命令摘要使用的稳定代码。
    ///
    /// # 返回
    /// 返回大写稳定代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::SupplierAccepted => "SUPPLIER_ACCEPTED",
            Self::ErpAccepted => "ERP_ACCEPTED",
            Self::Compensated => "COMPENSATED",
            Self::ClosedNoAdjustment => "CLOSED_NO_ADJUSTMENT",
        }
    }

    /// 返回该结论允许的受控原因代码。
    fn allowed_reason_codes(self) -> &'static [&'static str] {
        match self {
            Self::SupplierAccepted => &["BILL_ALIGNED", "NO_BUSINESS_IMPACT", "OTHER"],
            Self::ErpAccepted => &["ACCEPT_BILL", "AMOUNT_MISMATCH", "OTHER"],
            Self::Compensated => &["COMPENSATED_ELSEWHERE", "OTHER"],
            Self::ClosedNoAdjustment => &["NO_BUSINESS_IMPACT", "OTHER"],
        }
    }

    /// 判断结论是否必须携带正式证据引用。
    fn requires_evidence(self) -> bool {
        matches!(self, Self::Compensated | Self::ClosedNoAdjustment)
    }
}

/// 已规范化的差异正式结论、原因与证据组合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementDifferenceConclusion {
    kind: SettlementDifferenceConclusionKind,
    reason_code: String,
    evidence_reference_ids: Vec<String>,
    encoded: String,
}

impl SettlementDifferenceConclusion {
    /// 创建并校验差异正式结论。
    ///
    /// 原因代码按大写稳定格式规范化，并校验与结论类别的固定组合；证据引用去除
    /// 首尾空白、排序去重，禁止分隔符注入。已补偿与无需调整关闭必须携带证据。
    ///
    /// # 参数
    /// * `kind` - 正式结论类别
    /// * `reason_code` - 受控原因代码
    /// * `evidence_reference_ids` - 正式证据引用
    ///
    /// # 返回
    /// 返回可直接用于差异状态推进与持久化的规范化结论。
    ///
    /// # 错误
    /// 原因格式或组合非法、证据超限/非法/缺失，或编码结果超过 512 字节时返回错误。
    pub fn new(
        kind: SettlementDifferenceConclusionKind,
        reason_code: impl Into<String>,
        evidence_reference_ids: Vec<String>,
    ) -> Result<Self> {
        let reason_code = normalize_reason_code(reason_code.into())?;
        if !kind.allowed_reason_codes().contains(&reason_code.as_str()) {
            return Err(Error::from("差异结论与原因代码组合不受支持"));
        }
        let evidence_reference_ids = normalize_evidence_references(evidence_reference_ids)?;
        if kind.requires_evidence() && evidence_reference_ids.is_empty() {
            return Err(Error::from("已补偿或无需调整关闭必须提供正式证据引用"));
        }
        let encoded = format!(
            "reason={reason_code};evidence={}",
            evidence_reference_ids.join(",")
        );
        if encoded.len() > RESOLUTION_MAX_LEN {
            return Err(Error::from("差异结论的原因与证据引用合计不能超过512字节"));
        }
        Ok(Self {
            kind,
            reason_code,
            evidence_reference_ids,
            encoded,
        })
    }

    /// 返回结论对应的差异状态。
    ///
    /// # 返回
    /// 返回正式持久化状态。
    pub fn status(&self) -> SettlementDifferenceStatus {
        self.kind.status()
    }

    /// 返回正式结论类别。
    ///
    /// # 返回
    /// 返回构造时的强类型结论类别。
    pub fn kind(&self) -> SettlementDifferenceConclusionKind {
        self.kind
    }

    /// 返回规范化原因代码。
    ///
    /// # 返回
    /// 返回大写受控原因代码。
    pub fn reason_code(&self) -> &str {
        &self.reason_code
    }

    /// 返回排序去重后的证据引用。
    ///
    /// # 返回
    /// 返回只读证据引用切片。
    pub fn evidence_reference_ids(&self) -> &[String] {
        &self.evidence_reference_ids
    }

    /// 返回稳定持久化文本。
    ///
    /// # 返回
    /// 返回 `reason=<code>;evidence=<refs>` 格式文本。
    pub fn encoded(&self) -> &str {
        &self.encoded
    }
}

/// 结算差异创建数据（不含系统字段）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementDifferenceData {
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额（有符号，负数表示 ERP 金额大于供应商金额）。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
}

/// 结算差异更新数据（不含系统字段与关键字段）。
///
/// 结算明细、差异类型与差异金额创建后不可修改；处理结果三元组以 `Some` 设置、
/// `None` 保持原值。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierSettlementDifferenceUpdate {
    /// 差异状态；`None` 表示不修改。
    pub status: Option<SettlementDifferenceStatus>,
    /// 处理结果文本；`None` 表示不修改。
    pub resolution: Option<String>,
    /// 处理人；`None` 表示不修改。
    pub resolved_by: Option<String>,
    /// 处理时间；`None` 表示不修改。
    pub resolved_at: Option<Instant>,
}

/// 供应商结算差异实体（数据模型 §6.20）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementDifference {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属结算明细。
    pub statement_item_id: SupplierSettlementItemId,
    /// 差异类型。
    pub difference_type: SettlementDifferenceType,
    /// 差异金额。
    pub difference_amount: Amount,
    /// 差异状态。
    pub status: SettlementDifferenceStatus,
    /// 处理结果文本。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
}

impl SupplierSettlementDifference {
    /// 创建结算差异。
    ///
    /// 完成处理结果字段的校验和规范化，并强制三元组成组约束（§6.20）：
    /// 处理结果/处理人/处理时间必须同时提供或同时省略；待处理不得填写；
    /// 已补偿或已关闭必填。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierSettlementDifferenceId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的结算差异实体。
    ///
    /// # 错误
    /// 文本超长或处理结果三元组与状态不一致时返回错误。
    pub fn new(id: SupplierSettlementDifferenceId, data: SupplierSettlementDifferenceData) -> Result<Self> {
        let resolution = normalize_optional_text(data.resolution, "处理结果", RESOLUTION_MAX_LEN)?;
        let resolved_by = normalize_optional_text(data.resolved_by, "处理人", ACTOR_MAX_LEN)?;
        validate_resolution_state(data.status, &resolution, &resolved_by, data.resolved_at)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            statement_item_id: data.statement_item_id,
            difference_type: data.difference_type,
            difference_amount: data.difference_amount,
            status: data.status,
            resolution,
            resolved_by,
            resolved_at: data.resolved_at,
        })
    }

    /// 校验调用方持有的差异版本仍是当前版本。
    ///
    /// # 参数
    /// * `expected` - 调用方读取到的差异版本
    ///
    /// # 返回
    /// 版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 版本不一致时返回领域错误。
    pub fn ensure_version(&self, expected: u64) -> Result<()> {
        if self.base.version != expected {
            return Err(Error::from("结算差异版本不一致"));
        }
        Ok(())
    }

    /// 判断差异是否属于指定结算明细。
    ///
    /// # 参数
    /// * `item_id` - 结算明细主键
    ///
    /// # 返回
    /// 归属一致时返回 `true`。
    pub fn belongs_to_item(&self, item_id: &SupplierSettlementItemId) -> bool {
        self.statement_item_id == *item_id
    }

    /// 判断差异是否仍等待正式结论。
    ///
    /// # 返回
    /// 状态为 `PENDING` 时返回 `true`。
    pub fn is_pending(&self) -> bool {
        self.status == SettlementDifferenceStatus::Pending
    }

    /// 登记强类型正式结论。
    ///
    /// 只有待处理差异可以首次登记正式结论；状态、持久化文本、处理人和处理时间
    /// 作为一个整体推进，调用方不能分别拼装。
    ///
    /// # 参数
    /// * `conclusion` - 已规范化的原因与证据组合
    /// * `resolved_by` - 处理人
    /// * `resolved_at` - 处理时间
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 差异不是待处理状态，或处理字段校验失败时返回错误。
    pub fn record_conclusion(
        &mut self,
        conclusion: &SettlementDifferenceConclusion,
        resolved_by: impl Into<String>,
        resolved_at: Instant,
    ) -> Result<()> {
        if self.status != SettlementDifferenceStatus::Pending {
            return Err(Error::from("结算差异已有正式结论"));
        }
        self.update(SupplierSettlementDifferenceUpdate {
            status: Some(conclusion.status()),
            resolution: Some(conclusion.encoded().to_string()),
            resolved_by: Some(resolved_by.into()),
            resolved_at: Some(resolved_at),
        })
    }

    /// 更新结算差异。
    ///
    /// 复用 `new` 的校验规则；结算明细、差异类型与差异金额不可修改。
    ///
    /// # 参数
    /// * `update` - 更新数据
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 文本为空/超长或处理结果三元组与目标状态不一致时返回错误。
    pub fn update(&mut self, update: SupplierSettlementDifferenceUpdate) -> Result<()> {
        let status = update.status.unwrap_or(self.status);
        let resolution = if let Some(resolution) = update.resolution {
            Some(normalize_required_text(
                resolution,
                "处理结果不能为空",
                RESOLUTION_MAX_LEN,
                "处理结果过长",
            )?)
        } else {
            self.resolution.clone()
        };
        let resolved_by = if let Some(resolved_by) = update.resolved_by {
            Some(normalize_required_text(
                resolved_by,
                "处理人不能为空",
                ACTOR_MAX_LEN,
                "处理人过长",
            )?)
        } else {
            self.resolved_by.clone()
        };
        let resolved_at = update.resolved_at.or(self.resolved_at);
        validate_resolution_state(status, &resolution, &resolved_by, resolved_at)?;

        self.status = status;
        self.resolution = resolution;
        self.resolved_by = resolved_by;
        self.resolved_at = resolved_at;
        Ok(())
    }
}

/// 规范化受控原因代码。
fn normalize_reason_code(value: String) -> Result<String> {
    let value = value.trim().to_ascii_uppercase();
    if value.is_empty() || value.len() > 64 {
        return Err(Error::from("原因代码必须为1-64个字符"));
    }
    if !value
        .bytes()
        .all(|byte| byte.is_ascii_uppercase() || byte.is_ascii_digit() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(Error::from("原因代码只能包含大写字母、数字、下划线、连字符或点"));
    }
    Ok(value)
}

/// 规范化正式证据引用。
fn normalize_evidence_references(mut values: Vec<String>) -> Result<Vec<String>> {
    if values.len() > 20 {
        return Err(Error::from("证据引用最多20项"));
    }
    for value in &mut values {
        *value = value.trim().to_string();
        if value.is_empty()
            || value.len() > 128
            || value
                .chars()
                .any(|character| matches!(character, '|' | ';' | ','))
        {
            return Err(Error::from("证据引用必须非空、长度不超过128且不得包含分隔符"));
        }
    }
    values.sort();
    values.dedup();
    Ok(values)
}

/// 校验处理结果三元组与状态的成组约束。
///
/// # 参数
/// * `status` - 差异状态
/// * `resolution` - 处理结果文本
/// * `resolved_by` - 处理人
/// * `resolved_at` - 处理时间
///
/// # 错误
/// 三元组不完整、待处理填写了处理结果或已补偿/已关闭缺少处理结果时返回错误。
fn validate_resolution_state(
    status: SettlementDifferenceStatus,
    resolution: &Option<String>,
    resolved_by: &Option<String>,
    resolved_at: Option<Instant>,
) -> Result<()> {
    let trio_present = resolution.is_some() || resolved_by.is_some() || resolved_at.is_some();
    let trio_complete = resolution.is_some() && resolved_by.is_some() && resolved_at.is_some();
    if trio_present && !trio_complete {
        return Err(Error::from("处理结果、处理人与处理时间必须同时提供或同时省略"));
    }
    match status {
        SettlementDifferenceStatus::Pending if trio_present => Err(Error::from("待处理差异不得填写处理结果")),
        SettlementDifferenceStatus::Compensated | SettlementDifferenceStatus::Closed if !trio_complete => {
            Err(Error::from("已补偿或已关闭差异必须填写处理结果"))
        }
        _ => Ok(()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ids::SupplierSettlementDifferenceId;
    use std::str::FromStr;

    fn sample_data() -> SupplierSettlementDifferenceData {
        SupplierSettlementDifferenceData {
            statement_item_id: SupplierSettlementItemId::new("statement-item-1"),
            difference_type: SettlementDifferenceType::Amount,
            difference_amount: Amount::from_str("12.00").unwrap(),
            status: SettlementDifferenceStatus::Pending,
            resolution: None,
            resolved_by: None,
            resolved_at: None,
        }
    }

    fn compensated_data() -> SupplierSettlementDifferenceData {
        SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Compensated,
            resolution: Some(" 已追加成本差额 ".to_string()),
            resolved_by: Some(" 财务-1 ".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        }
    }

    #[test]
    fn new_accepts_pending_without_resolution() {
        let difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();

        assert_eq!(difference.difference_type, SettlementDifferenceType::Amount);
        assert_eq!(difference.status, SettlementDifferenceStatus::Pending);
        assert!(difference.resolution.is_none());
    }

    #[test]
    fn new_accepts_compensated_with_resolution() {
        let difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-2"),
            compensated_data(),
        )
        .unwrap();

        assert_eq!(difference.status, SettlementDifferenceStatus::Compensated);
        assert_eq!(difference.resolution.as_deref(), Some("已追加成本差额"));
        assert_eq!(difference.resolved_by.as_deref(), Some("财务-1"));
        assert!(difference.resolved_at.is_some());
    }

    #[test]
    fn new_rejects_inconsistent_resolution_trio() {
        let compensated_without_resolution = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Compensated,
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-3"),
            compensated_without_resolution
        )
        .is_err());

        let pending_with_resolution = SupplierSettlementDifferenceData {
            resolution: Some("补偿完成".to_string()),
            resolved_by: Some("财务-1".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-4"),
            pending_with_resolution
        )
        .is_err());

        let partial_trio = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Closed,
            resolution: Some("关闭".to_string()),
            ..sample_data()
        };
        assert!(SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-5"),
            partial_trio
        )
        .is_err());
    }

    #[test]
    fn new_rejects_overlong_resolution() {
        let data = SupplierSettlementDifferenceData {
            status: SettlementDifferenceStatus::Closed,
            resolution: Some("r".repeat(513)),
            resolved_by: Some("财务-1".to_string()),
            resolved_at: Some(Instant::from_unix_secs(1_700_000_000)),
            ..sample_data()
        };
        assert!(
            SupplierSettlementDifference::new(SupplierSettlementDifferenceId::new("difference-6"), data)
                .is_err()
        );
    }

    #[test]
    fn conclusion_normalizes_reason_and_evidence_as_one_value() {
        let conclusion = SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::ErpAccepted,
            " accept_bill ",
            vec![
                " proof-2 ".to_string(),
                "proof-1".to_string(),
                "proof-1".to_string(),
            ],
        )
        .unwrap();
        assert_eq!(conclusion.reason_code(), "ACCEPT_BILL");
        assert_eq!(
            conclusion.evidence_reference_ids(),
            &["proof-1".to_string(), "proof-2".to_string()]
        );
        assert_eq!(conclusion.status(), SettlementDifferenceStatus::ErpAcknowledged);

        assert!(SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::Compensated,
            "COMPENSATED_ELSEWHERE",
            Vec::new(),
        )
        .is_err());
        assert!(SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::SupplierAccepted,
            "ACCEPT_BILL",
            vec!["proof-1".to_string()],
        )
        .is_err());
    }

    #[test]
    fn record_conclusion_requires_pending_and_sets_resolution_trio() {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();
        let conclusion = SettlementDifferenceConclusion::new(
            SettlementDifferenceConclusionKind::ClosedNoAdjustment,
            "NO_BUSINESS_IMPACT",
            vec!["proof-1".to_string()],
        )
        .unwrap();
        difference
            .record_conclusion(&conclusion, "finance-1", Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert_eq!(difference.status, SettlementDifferenceStatus::Closed);
        assert_eq!(
            difference.resolution.as_deref(),
            Some("reason=NO_BUSINESS_IMPACT;evidence=proof-1")
        );
        assert!(difference
            .record_conclusion(&conclusion, "finance-1", Instant::from_unix_secs(1_700_000_001))
            .is_err());
    }

    #[test]
    fn update_applies_status_and_resolution() {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();
        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::ErpAcknowledged),
                ..Default::default()
            })
            .unwrap();
        assert_eq!(difference.status, SettlementDifferenceStatus::ErpAcknowledged);

        difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::Compensated),
                resolution: Some("补偿".to_string()),
                resolved_by: Some("财务-2".to_string()),
                resolved_at: Some(Instant::from_unix_secs(1_700_000_100)),
            })
            .unwrap();
        assert_eq!(difference.status, SettlementDifferenceStatus::Compensated);
        assert_eq!(difference.resolved_by.as_deref(), Some("财务-2"));
        assert_eq!(
            difference.difference_type,
            SettlementDifferenceType::Amount,
            "关键字段不可修改"
        );
    }

    #[test]
    fn update_rejects_missing_resolution_for_closed() {
        let mut difference = SupplierSettlementDifference::new(
            SupplierSettlementDifferenceId::new("difference-1"),
            sample_data(),
        )
        .unwrap();
        assert!(difference
            .update(SupplierSettlementDifferenceUpdate {
                status: Some(SettlementDifferenceStatus::Closed),
                ..Default::default()
            })
            .is_err());
    }
}
