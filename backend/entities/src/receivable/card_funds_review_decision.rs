//! W13 卡券票款正式复核决定值对象（FIN-E10）。
//!
//! 将 `validate_card_funds_decision`、`validate_card_funds_evidence_facts`、
//! `canonical_review_evidence` 与 `workflow_decision_comment` 四处 Service 私有规则
//! 收敛到实体层：`ValidatedCardFundsReviewDecision` 统一校验受控理由、证据去重、
//! 长度边界与规范化文本；`CardFundsReviewEvidence` 负责证据 `file_asset` 与
//! 引用字符串的空白／重复／超长校验及字节级稳定的 canonical 排序；
//! `FileAsset::is_usable_at` / `validate_usable_at` 提供指定时点的可用性判断
//! （扫描、销毁、过期）。Service 仅保留批量文件读取、当前时间注入、授权与审批提交。

use std::collections::{HashMap, HashSet};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::file_asset::FileAsset;
use crate::ids::FileAssetId;

/// 受控驳回原因白名单（与 Service 原 `validate_card_funds_decision` 完全一致）。
const ALLOWED_REASON_CODES: &[&str] = &[
    "EVIDENCE_INSUFFICIENT",
    "FACTS_MISMATCH",
    "COUNTERPARTY_UNCLEAR",
    "OTHER",
];

/// 证据规范化后最大字符数（与原 `canonical_review_evidence` 一致）。
const CANONICAL_MAX_CHARS: usize = 512;
/// 工作流意见最大字符数（与原 `workflow_decision_comment` 一致）。
const COMMENT_MAX_CHARS: usize = 512;

/// W13 复核类型（实体侧镜像，与 `services::dto::CardFundsReviewType` 语义对齐）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsReviewType {
    /// 卡券期初票款复核。
    Opening,
    /// 商城同步差额复核。
    SyncDelta,
}

/// W13 复核结果（实体侧镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsReviewResult {
    /// 通过。
    Approved,
    /// 驳回。
    Rejected,
}

/// W13 复核结论（实体侧镜像）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsReviewConclusion {
    /// 已核实不存在上线前历史票款，从零起算。
    NoHistoryFromZero,
    /// 已登记正式票款事实且核对一致。
    RecordedFactsReconciled,
    /// 驳回。
    Rejected,
}

impl CardFundsReviewConclusion {
    /// 返回 HTTP 稳定代码（与服务侧 `as_str` 一致，供 `workflow_decision_comment` 使用）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回稳定字符串。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::NoHistoryFromZero => "NO_HISTORY_FROM_ZERO",
            Self::RecordedFactsReconciled => "RECORDED_FACTS_RECONCILED",
            Self::Rejected => "REJECTED",
        }
    }
}

/// W13 受控证据值对象。
///
/// 负责 `evidence_document_ids` 与 `evidence_references` 的空白／重复／超长校验、
/// 至少一项非空、以及字节级稳定的 canonical 文本生成（排序后 `"; "` 连接）。
/// 纯内存、确定性、无 I/O。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsReviewEvidence {
    document_ids: Vec<FileAssetId>,
    references: Vec<String>,
    canonical: Option<String>,
}

impl CardFundsReviewEvidence {
    /// 校验并构造受控证据。
    ///
    /// 逐项检查 `document_ids` 的非空与长度上限（128）、`references` 的 trim 后非空
    /// 与长度上限（256）、跨集合至少一项、以及去重；随后生成排序后的 canonical
    /// 文本并校验 512 字符上限。
    ///
    /// # 参数
    /// * `document_ids` - 受控证据文件 ID（保持 DTO 原始顺序，首个为主证据）
    /// * `references` - 受控证据引用（原始输入，含空白需 trim）
    ///
    /// # 返回
    /// 校验通过的证据值对象；canonical 文本已预计算并排序。
    ///
    /// # 错误
    /// - 任一文件 ID 为空白或超过 128 字符时返回 `ValidationError("证据文件 ID 非法")`
    /// - 文件 ID 重复时返回 `ValidationError("证据文件不得重复")`
    /// - 任一引用 trim 后为空时返回 `ValidationError("证据引用不能为空白")`
    /// - 引用超过 256 字符时返回 `ValidationError("单条证据引用不能超过 256 个字符")`
    /// - 引用重复时返回 `ValidationError("证据引用不得重复")`
    /// - 两类证据均为空时返回 `ValidationError("正式复核证据不能为空")`
    /// - 规范化后超过 512 字符时返回 `ValidationError("规范化后的复核证据引用不能超过 512 个字符")`
    ///
    /// # 约束
    /// 纯内存确定性校验；不执行 I/O、时钟或加密；错误文案与首错顺序与原 Service 保持一致。
    pub fn new(document_ids: &[FileAssetId], references: &[String]) -> Result<Self> {
        let mut seen_ids = HashSet::new();
        for id in document_ids {
            let raw = id.as_ref();
            if raw.trim().is_empty() || raw.chars().count() > 128 {
                return Err(Error::from("证据文件 ID 非法".to_string()));
            }
            if !seen_ids.insert(raw.to_string()) {
                return Err(Error::from("证据文件不得重复".to_string()));
            }
        }
        let mut normalized_refs = Vec::with_capacity(references.len());
        let mut seen_refs = HashSet::new();
        for reference in references {
            let normalized = reference.trim();
            if normalized.is_empty() {
                return Err(Error::from("证据引用不能为空白".to_string()));
            }
            if normalized.chars().count() > 256 {
                return Err(Error::from("单条证据引用不能超过 256 个字符".to_string()));
            }
            if !seen_refs.insert(normalized.to_string()) {
                return Err(Error::from("证据引用不得重复".to_string()));
            }
            normalized_refs.push(normalized.to_string());
        }
        if document_ids.is_empty() && normalized_refs.is_empty() {
            return Err(Error::from("正式复核证据不能为空".to_string()));
        }
        let canonical = Self::build_canonical(document_ids, &normalized_refs)?;
        Ok(Self {
            document_ids: document_ids.to_vec(),
            references: normalized_refs,
            canonical,
        })
    }

    /// 生成字节级稳定的 canonical 证据文本。
    ///
    /// 将 `references` 与 `document_ids[1..]` 的 `file_asset:<id>` 形式合并后按字典序排序，
    /// 再以 `"; "` 连接；空集合返回 `None`。
    ///
    /// # 参数
    /// * `document_ids` - 原始文件 ID 列表（首个为主证据，不计入引用）
    /// * `references` - 已 trim 的引用列表
    ///
    /// # 返回
    /// 排序后的 canonical 文本或 `None`。
    ///
    /// # 错误
    /// 连接后超过 512 字符时返回 `ValidationError`。
    fn build_canonical(document_ids: &[FileAssetId], references: &[String]) -> Result<Option<String>> {
        let mut parts = references.to_vec();
        parts.extend(
            document_ids
                .iter()
                .skip(1)
                .map(|id| format!("file_asset:{}", id.as_ref())),
        );
        if parts.is_empty() {
            return Ok(None);
        }
        parts.sort();
        let canonical = parts.join("; ");
        if canonical.chars().count() > CANONICAL_MAX_CHARS {
            return Err(Error::from(
                "规范化后的复核证据引用不能超过 512 个字符".to_string(),
            ));
        }
        Ok(Some(canonical))
    }

    /// 返回受控文件 ID（保持输入顺序，含主证据）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 文件 ID 切片。
    pub fn document_ids(&self) -> &[FileAssetId] {
        &self.document_ids
    }

    /// 返回已规范化的引用列表（trim 后、去重、保持输入顺序）。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 引用切片。
    pub fn references(&self) -> &[String] {
        &self.references
    }

    /// 返回字节级稳定的 canonical 证据文本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `None` 或排序后 `"; "` 连接的文本；相同输入必得相同输出。
    pub fn canonical(&self) -> Option<&str> {
        self.canonical.as_deref()
    }

    /// 按命令原始顺序校验批量读取的资产事实。
    ///
    /// 批量仓储结果不承诺顺序；本方法按 `document_ids` 逐项解释首个缺失与首个不可用，
    /// 保持既有首错语义（先 `NotFound`，后 `BusinessLogicError`）。
    ///
    /// # 参数
    /// * `assets` - 仓储一次批量读取的资产事实（无序）
    /// * `now` - 本次校验的统一时点
    ///
    /// # 返回
    /// 全部文件均存在且在 `now` 可用时返回 `Ok(())`。
    ///
    /// # 错误
    /// 首个缺失文件返回 `NotFound("复核证据文件不存在: <id>")`；首个不可用文件返回
    /// `BusinessLogicError("复核证据文件未通过安全检查、已销毁或已过期")`。
    ///
    /// # 约束
    /// 纯内存判断；`FileAsset::is_usable_at` 为唯一可用性规则；不执行 I/O。
    pub fn validate_assets(&self, assets: &[FileAsset], now: Instant) -> Result<()> {
        let assets_by_id: HashMap<&str, &FileAsset> = assets
            .iter()
            .map(|asset| (asset.base.id.as_str(), asset))
            .collect();
        for id in &self.document_ids {
            let asset = assets_by_id
                .get(id.as_ref())
                .ok_or_else(|| Error::from(format!("复核证据文件不存在: {}", id.as_ref())))?;
            asset.validate_usable_at(now)?;
        }
        Ok(())
    }
}

/// 已校验的 W13 正式决定（FIN-E10 唯一规则入口）。
///
/// 统一校验 `receivable_account_id`、决策／结论组合、受控驳回原因、证据值对象、
/// canonical 证据与工作流意见；任一失败整体不产生部分结果。所有文本生成均
/// 通过本类型唯一入口，供 Service 批量读取后直接使用。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidatedCardFundsReviewDecision {
    receivable_account_id: String,
    expected_review_chain_tail_id: Option<String>,
    review_type: CardFundsReviewType,
    review_result: CardFundsReviewResult,
    conclusion: CardFundsReviewConclusion,
    evidence: CardFundsReviewEvidence,
    reason_code: Option<String>,
    comment: Option<String>,
    canonical_evidence: Option<String>,
    workflow_comment: Option<String>,
}

impl ValidatedCardFundsReviewDecision {
    /// 校验并构造已验证决定。
    ///
    /// # 参数
    /// * `receivable_account_id` - 应收账户 ID（原始输入）
    /// * `expected_review_chain_tail_id` - 期望复核链尾（可选，空白需拒绝）
    /// * `review_type` - 复核类型
    /// * `review_result` - 复核结果
    /// * `conclusion` - 复核结论
    /// * `evidence_document_ids` - 证据文件 ID
    /// * `evidence_references` - 证据引用
    /// * `reason_code` - 驳回原因代码（原始输入，需 trim）
    /// * `comment` - 补充说明（原始输入，需 trim）
    ///
    /// # 返回
    /// 校验通过的已验证决定，含预计算的 canonical 证据与工作流意见。
    ///
    /// # 错误
    /// - 账户 ID 为空白或超过 128 字符时返回 `ValidationError("应收账户 ID 非法")`
    /// - 链尾为 Some 空白时返回 `ValidationError("复核链尾不能为空白")`
    /// - 决策／结论组合不合法时返回 `ValidationError("复核结果与结论组合不合法")`
    /// - 通过决定携带原因时返回 `ValidationError("通过决定不得携带驳回原因")`
    /// - 驳回决定缺少原因时返回 `ValidationError("驳回决定必须填写原因代码")`
    /// - 驳回原因不在白名单时返回 `ValidationError("驳回原因代码不在受控范围内")`
    /// - 证据校验失败时透传 `CardFundsReviewEvidence` 错误
    /// - canonical 或 workflow 文本超过 512 字符时返回对应 `ValidationError`
    ///
    /// # 约束
    /// 纯内存确定性校验；不依赖 MongoDB、HTTP、时钟或密钥；错误文案与原 Service 一致。
    #[allow(clippy::too_many_arguments)]
    pub fn try_new(
        receivable_account_id: &str,
        expected_review_chain_tail_id: Option<&str>,
        review_type: CardFundsReviewType,
        review_result: CardFundsReviewResult,
        conclusion: CardFundsReviewConclusion,
        evidence_document_ids: &[FileAssetId],
        evidence_references: &[String],
        reason_code: Option<&str>,
        comment: Option<&str>,
    ) -> Result<Self> {
        if receivable_account_id.trim().is_empty() || receivable_account_id.chars().count() > 128 {
            return Err(Error::from("应收账户 ID 非法".to_string()));
        }
        let expected_review_chain_tail_id = match expected_review_chain_tail_id {
            Some(tail) if tail.trim().is_empty() => {
                return Err(Error::from("复核链尾不能为空白".to_string()));
            }
            Some(tail) => Some(tail.to_string()),
            None => None,
        };
        let normalized_reason = reason_code.map(str::trim).filter(|v| !v.is_empty());
        match (review_result, conclusion) {
            (CardFundsReviewResult::Approved, CardFundsReviewConclusion::NoHistoryFromZero)
            | (CardFundsReviewResult::Approved, CardFundsReviewConclusion::RecordedFactsReconciled) => {
                if normalized_reason.is_some() {
                    return Err(Error::from("通过决定不得携带驳回原因".to_string()));
                }
            }
            (CardFundsReviewResult::Rejected, CardFundsReviewConclusion::Rejected) => {
                let reason =
                    normalized_reason.ok_or_else(|| Error::from("驳回决定必须填写原因代码".to_string()))?;
                if !ALLOWED_REASON_CODES.contains(&reason) {
                    return Err(Error::from("驳回原因代码不在受控范围内".to_string()));
                }
            }
            _ => {
                return Err(Error::from("复核结果与结论组合不合法".to_string()));
            }
        }
        let evidence = CardFundsReviewEvidence::new(evidence_document_ids, evidence_references)?;
        let canonical_evidence = evidence.canonical().map(|s| s.to_string());
        let reason_owned = normalized_reason.map(|s| s.to_string());
        let comment_owned = comment
            .map(str::trim)
            .filter(|v| !v.is_empty())
            .map(|s| s.to_string());
        let workflow_comment =
            Self::build_workflow_comment(conclusion, reason_owned.as_deref(), comment_owned.as_deref())?;
        Ok(Self {
            receivable_account_id: receivable_account_id.to_string(),
            expected_review_chain_tail_id,
            review_type,
            review_result,
            conclusion,
            evidence,
            reason_code: reason_owned,
            comment: comment_owned,
            canonical_evidence,
            workflow_comment,
        })
    }

    /// 生成工作流意见（唯一入口）。
    ///
    /// 规则与原 `workflow_decision_comment` 一致：`conclusion=<CODE>` 固定首段，
    /// 随后可选 `reason=<CODE>` 与补充说明，以 `"; "` 连接，超过 512 字符拒绝。
    ///
    /// # 参数
    /// * `conclusion` - 结论
    /// * `reason` - 已 trim 的原因代码（可选）
    /// * `comment` - 已 trim 的补充说明（可选）
    ///
    /// # 返回
    /// 去重后 `Some(comment)` 或超长时的 `ValidationError`。
    fn build_workflow_comment(
        conclusion: CardFundsReviewConclusion,
        reason: Option<&str>,
        comment: Option<&str>,
    ) -> Result<Option<String>> {
        let mut parts = vec![format!("conclusion={}", conclusion.as_str())];
        if let Some(reason) = reason {
            parts.push(format!("reason={reason}"));
        }
        if let Some(comment) = comment {
            parts.push(comment.to_string());
        }
        let joined = parts.join("; ");
        if joined.chars().count() > COMMENT_MAX_CHARS {
            return Err(Error::from("工作流复核意见不能超过 512 个字符".to_string()));
        }
        Ok(Some(joined))
    }

    /// 返回应收账户 ID。
    pub fn receivable_account_id(&self) -> &str {
        &self.receivable_account_id
    }

    /// 返回受控证据值对象。
    pub fn evidence(&self) -> &CardFundsReviewEvidence {
        &self.evidence
    }

    /// 返回 canonical 证据文本（已排序、字节级稳定）。
    ///
    /// # 返回
    /// `None` 或 `"; "` 连接的排序文本。
    pub fn canonical_evidence(&self) -> Option<&str> {
        self.canonical_evidence.as_deref()
    }

    /// 返回工作流意见（已按固定顺序生成、字节级稳定）。
    ///
    /// # 返回
    /// `Some(comment)`，与原 `workflow_decision_comment` 语义一致。
    pub fn workflow_comment(&self) -> Option<&str> {
        self.workflow_comment.as_deref()
    }

    /// 返回复核类型。
    pub fn review_type(&self) -> CardFundsReviewType {
        self.review_type
    }

    /// 返回复核结果。
    pub fn review_result(&self) -> CardFundsReviewResult {
        self.review_result
    }

    /// 返回复核结论。
    pub fn conclusion(&self) -> CardFundsReviewConclusion {
        self.conclusion
    }

    /// 返回已规范化的驳回原因（可选）。
    pub fn reason_code(&self) -> Option<&str> {
        self.reason_code.as_deref()
    }

    /// 返回已规范化的补充说明（可选）。
    pub fn comment(&self) -> Option<&str> {
        self.comment.as_deref()
    }

    /// 返回期望复核链尾（可选）。
    pub fn expected_review_chain_tail_id(&self) -> Option<&str> {
        self.expected_review_chain_tail_id.as_deref()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::time::Instant;
    use crate::file_asset::{ContentHmac, FileAsset, FileAssetData, RetentionClass, SensitivityClass};
    use crate::ids::FileAssetId;

    fn decision_ids() -> Vec<FileAssetId> {
        vec![FileAssetId::new("file-1")]
    }

    fn approved_no_history() -> ValidatedCardFundsReviewDecision {
        ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            None,
            Some("已核对"),
        )
        .unwrap()
    }

    fn evidence_asset(id: &str, passed: bool, destroyed: bool, expires_at: Option<Instant>) -> FileAsset {
        let mut asset = FileAsset::new(
            FileAssetId::new(id),
            FileAssetData {
                storage_object_key: format!("k/{id}"),
                file_name: format!("{id}.pdf"),
                content_type: "application/pdf".to_string(),
                byte_size: 1,
                content_hmac: ContentHmac::parse("a".repeat(64)).unwrap(),
                sensitivity_class: SensitivityClass::Sensitive,
                retention_class: RetentionClass::LongTerm,
                expires_at: None,
                created_by: "u1".to_string(),
            },
        )
        .unwrap();
        if passed {
            asset
                .mark_scan_result(crate::file_asset::SecurityScanStatus::Passed)
                .unwrap();
        }
        if destroyed {
            asset.destroy(Instant::from_unix_secs(1_700_000_000)).unwrap();
        }
        if let Some(exp) = expires_at {
            asset.expires_at = Some(exp);
        }
        asset
    }

    /// 决策／结论组合：合法通过与受控驳回均可构造，非法组合拒绝。
    #[test]
    fn decision_conclusion_matrix() {
        // Approved + NoHistoryFromZero 合法
        assert!(ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            None,
            None
        )
        .is_ok());
        // Approved + RecordedFactsReconciled 合法
        assert!(ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::RecordedFactsReconciled,
            &decision_ids(),
            &[],
            None,
            None
        )
        .is_ok());
        // Rejected + Rejected 合法（需原因）
        assert!(ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            Some("OTHER"),
            None
        )
        .is_ok());
        // 非法组合：Approved + Rejected
        assert!(ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            None,
            None
        )
        .is_err());
        // 非法组合：Rejected + NoHistoryFromZero
        assert!(ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            Some("OTHER"),
            None
        )
        .is_err());
    }

    /// 受控理由：通过不得携带、驳回应为白名单、空白拒绝。
    #[test]
    fn controlled_reason() {
        // 通过携带原因拒绝
        let err = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            Some("OTHER"),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("通过决定不得携带驳回原因"));
        // 驳回缺少原因拒绝
        let err = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("驳回决定必须填写原因代码"));
        // 驳回原因不在白名单拒绝
        let err = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            Some("UNKNOWN"),
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("驳回原因代码不在受控范围内"));
        // 白名单四项均可通过
        for code in ALLOWED_REASON_CODES {
            assert!(ValidatedCardFundsReviewDecision::try_new(
                "ra-1",
                None,
                CardFundsReviewType::Opening,
                CardFundsReviewResult::Rejected,
                CardFundsReviewConclusion::Rejected,
                &decision_ids(),
                &[],
                Some(code),
                None
            )
            .is_ok());
        }
    }

    /// 证据空白、重复、超长及至少一项校验。
    #[test]
    fn evidence_blank_duplicate_overlong() {
        // 空白引用拒绝
        assert!(CardFundsReviewEvidence::new(&decision_ids(), &["  ".to_string()]).is_err());
        // 重复文件 ID 拒绝
        assert!(CardFundsReviewEvidence::new(&[FileAssetId::new("a"), FileAssetId::new("a")], &[]).is_err());
        // 重复引用拒绝（trim 后相等）
        assert!(CardFundsReviewEvidence::new(&[], &["ref-1".to_string(), " ref-1 ".to_string()]).is_err());
        // 单条引用超 256 拒绝
        assert!(CardFundsReviewEvidence::new(&[], &["a".repeat(257)]).is_err());
        // 文件 ID 超 128 拒绝
        assert!(CardFundsReviewEvidence::new(&[FileAssetId::new("a".repeat(129))], &[]).is_err());
        // 两类均空拒绝
        assert!(CardFundsReviewEvidence::new(&[], &[]).is_err());
        // 单条恰好 256 通过
        assert!(CardFundsReviewEvidence::new(&[], &["a".repeat(256)]).is_ok());
    }

    /// 长度边界：canonical 超 512 拒绝；workflow 意见超 512 拒绝。
    #[test]
    fn length_boundaries() {
        // canonical 边界：构造恰好 512 与超限
        let refs = vec!["a".repeat(256), "b".repeat(256)];
        // "a"*256 + "; " + "b"*256 = 514 >512 拒绝
        assert!(CardFundsReviewEvidence::new(&[], &refs).is_err());
        // workflow 边界：conclusion + reason + comment 超 512
        let long_comment = "c".repeat(600);
        let err = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            Some("OTHER"),
            Some(&long_comment),
        )
        .unwrap_err();
        assert!(err.to_string().contains("工作流复核意见不能超过 512"));
    }

    /// scan 状态、destroyed、expired 通过 FileAsset 可用性判断。
    #[test]
    fn scan_destroyed_expired() {
        let evidence = CardFundsReviewEvidence::new(&decision_ids(), &[]).unwrap();
        let now = Instant::from_unix_secs(1_700_000_000);
        // 通过状态
        let asset_ok = evidence_asset("file-1", true, false, None);
        assert!(evidence.validate_assets(&[asset_ok], now).is_ok());
        // 未通过扫描
        let asset_pending = evidence_asset("file-1", false, false, None);
        assert!(evidence.validate_assets(&[asset_pending], now).is_err());
        // 已销毁
        let asset_destroyed = evidence_asset("file-1", true, true, None);
        assert!(evidence.validate_assets(&[asset_destroyed], now).is_err());
        // 已过期（expires_at <= now）
        let asset_expired = evidence_asset(
            "file-1",
            true,
            false,
            Some(Instant::from_unix_secs(1_699_999_999)),
        );
        assert!(evidence.validate_assets(&[asset_expired], now).is_err());
        // 未过期（expires_at > now）通过
        let asset_future = evidence_asset(
            "file-1",
            true,
            false,
            Some(Instant::from_unix_secs(1_700_000_001)),
        );
        assert!(evidence.validate_assets(&[asset_future], now).is_ok());
        // 边界：expires_at == now 视为过期
        let asset_boundary = evidence_asset("file-1", true, false, Some(now));
        assert!(evidence.validate_assets(&[asset_boundary], now).is_err());
    }

    /// 首错顺序：缺失文件先于扫描错误。
    #[test]
    fn first_error_missing_before_scan() {
        let evidence = CardFundsReviewEvidence::new(
            &[FileAssetId::new("file-missing"), FileAssetId::new("file-pending")],
            &[],
        )
        .unwrap();
        let assets = vec![evidence_asset("file-pending", false, false, None)];
        let err = evidence
            .validate_assets(&assets, Instant::from_unix_secs(1_700_000_000))
            .unwrap_err();
        assert!(err.to_string().contains("复核证据文件不存在: file-missing"));
    }

    /// canonical 排序：相同输入不同顺序产出相同文本（字节级稳定）。
    #[test]
    fn canonical_sorting_is_stable() {
        let ids = vec![
            FileAssetId::new("file-1"),
            FileAssetId::new("file-3"),
            FileAssetId::new("file-2"),
        ];
        let refs = vec!["z-ref".to_string(), "a-ref".to_string()];
        let evidence = CardFundsReviewEvidence::new(&ids, &refs).unwrap();
        // canonical 应为排序后结果：a-ref, file_asset:file-2, file_asset:file-3, z-ref (按字典序，首个 file-1 为主证据不计入)
        let canonical = evidence.canonical().unwrap();
        let mut expected_parts = [
            "a-ref".to_string(),
            "z-ref".to_string(),
            "file_asset:file-2".to_string(),
            "file_asset:file-3".to_string(),
        ];
        expected_parts.sort();
        assert_eq!(canonical, expected_parts.join("; "));
        // 再次构造不同输入顺序，但主证据相同、附加证据集合相同，canonical 相同
        let ids2 = vec![
            FileAssetId::new("file-1"),
            FileAssetId::new("file-2"),
            FileAssetId::new("file-3"),
        ];
        let refs2 = vec!["a-ref".to_string(), "z-ref".to_string()];
        let evidence2 = CardFundsReviewEvidence::new(&ids2, &refs2).unwrap();
        assert_eq!(evidence.canonical(), evidence2.canonical());
    }

    /// 相同完整决定产出字节级稳定文本。
    #[test]
    fn validated_decision_is_byte_stable() {
        let d1 = approved_no_history();
        let d2 = approved_no_history();
        assert_eq!(d1.canonical_evidence(), d2.canonical_evidence());
        assert_eq!(d1.workflow_comment(), d2.workflow_comment());
        // 改变原因或补充说明则文本变化
        let d3 = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Rejected,
            CardFundsReviewConclusion::Rejected,
            &decision_ids(),
            &[],
            Some("OTHER"),
            Some("补充说明"),
        )
        .unwrap();
        assert!(d3.workflow_comment().unwrap().contains("reason=OTHER"));
        assert!(d3.workflow_comment().unwrap().contains("补充说明"));
    }

    /// 账户 ID 与链尾空白校验。
    #[test]
    fn account_and_chain_tail_validation() {
        let err = ValidatedCardFundsReviewDecision::try_new(
            "  ",
            None,
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("应收账户 ID 非法"));
        let err = ValidatedCardFundsReviewDecision::try_new(
            "ra-1",
            Some("  "),
            CardFundsReviewType::Opening,
            CardFundsReviewResult::Approved,
            CardFundsReviewConclusion::NoHistoryFromZero,
            &decision_ids(),
            &[],
            None,
            None,
        )
        .unwrap_err();
        assert!(err.to_string().contains("复核链尾不能为空白"));
    }
}
