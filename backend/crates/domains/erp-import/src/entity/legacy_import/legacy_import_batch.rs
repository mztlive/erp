//! `legacy_import_batch`：旧数据导入批次（数据模型 §6.12）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::{DocumentState, ensure_transition};
use erp_core::common::time::BusinessDate;
use erp_core::ids::FileAssetId;
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::ConfirmationScope;

/// 批次号最大长度。
const BATCH_NO_MAX_LEN: usize = 64;
/// 来源对象集合描述最大长度。
const OBJECT_SET_MAX_LEN: usize = 128;
/// 导入规则版本最大长度。
const RULE_VERSION_MAX_LEN: usize = 64;
/// 来源文件 keyed HMAC 最大长度。
const FILE_HMAC_MAX_LEN: usize = 128;
/// 脱敏错误码摘要与确认状态摘要最大长度。
const SUMMARY_MAX_LEN: usize = 2048;

/// 导入批次状态（待校验、校验中、待确认、待应用、导入中及结果态）。
///
/// 最后一项责任确认只推进到 `ReadyToApply`，只有独立的
/// `START_APPLY` 命令才能进入 `Importing`。失败项重试可将失败结果态
/// 重新准备为 `ReadyToApply`，已成功或已跳过的行不回退。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum LegacyImportBatchStatus {
    /// 待校验。
    PendingValidation,
    /// 校验中。
    Validating,
    /// 待确认（等待必要责任范围确认）。
    PendingConfirmation,
    /// 待应用（全部必要责任范围已确认，等待独立提交应用命令）。
    ReadyToApply,
    /// 导入中。
    Importing,
    /// 完成。
    Completed,
    /// 部分失败。
    PartialFailed,
    /// 失败。
    Failed,
}

impl LegacyImportBatchStatus {
    /// 提供批次处理状态的中文文案，供进度展示和业务错误提示。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingValidation => "待校验",
            Self::Validating => "校验中",
            Self::PendingConfirmation => "待确认",
            Self::ReadyToApply => "待应用",
            Self::Importing => "导入中",
            Self::Completed => "完成",
            Self::PartialFailed => "部分失败",
            Self::Failed => "失败",
        }
    }

    /// 提供批次状态的稳定代码，确保存储和按状态查询采用同一口径。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingValidation => "pending_validation",
            Self::Validating => "validating",
            Self::PendingConfirmation => "pending_confirmation",
            Self::ReadyToApply => "ready_to_apply",
            Self::Importing => "importing",
            Self::Completed => "completed",
            Self::PartialFailed => "partial_failed",
            Self::Failed => "failed",
        }
    }
}

impl DocumentState for LegacyImportBatchStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::PendingValidation => &[Self::Validating],
            Self::Validating => &[Self::PendingConfirmation, Self::PartialFailed, Self::Failed],
            Self::PendingConfirmation => &[Self::ReadyToApply],
            Self::ReadyToApply => &[Self::Importing, Self::PartialFailed, Self::Failed],
            Self::Importing => &[Self::Completed, Self::PartialFailed, Self::Failed],
            Self::PartialFailed | Self::Failed => &[Self::ReadyToApply],
            Self::Completed => &[],
        }
    }
}

/// 导入批次创建数据（数据模型 §6.12）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportBatchData {
    /// 导入批次号（唯一）。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: erp_core::ids::SourceSystemId,
    /// 本批来源对象集合，如客户、供应商、SPU、SKU、卡券销售。
    pub source_object_set: String,
    /// 期初业务基准日。
    pub baseline_date: BusinessDate,
    /// 本批解析、清理和映射规则版本。
    pub import_rule_version: String,
    /// 受控临时区计算的 keyed HMAC，仅用于审计去重。
    pub source_file_hmac: Option<String>,
    /// 初始状态。
    pub status: LegacyImportBatchStatus,
    /// 处理统计：总行数。
    pub total_rows: u64,
    /// 处理统计：成功行数。
    pub success_rows: u64,
    /// 处理统计：失败行数。
    pub failed_rows: u64,
    /// 脱敏错误码及计数（不含原值和行列明细）。
    pub failure_code_summary: Option<String>,
    /// 各必要 `legacy_import_confirmation` 的派生摘要。
    pub confirmation_status_summary: Option<String>,
}

/// 旧数据导入批次实体（数据模型 §6.12）。
///
/// 本表是唯一持久兼容层，不为旧五张表各建一套 ERP 影子业务表（§6.12）；
/// 成功白名单包与失败诊断包必须生成独立 `file_asset`，由形成资产的组合层写入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct LegacyImportBatch {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 导入批次号（创建后不可修改）。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: erp_core::ids::SourceSystemId,
    /// 本批来源对象集合。
    pub source_object_set: String,
    /// 期初业务基准日。
    pub baseline_date: BusinessDate,
    /// 成功对象的白名单包（成功数为零时可空）。
    pub successful_sanitized_file_asset_id: Option<FileAssetId>,
    /// 成功对象的 manifest（成功数为零时可空）。
    pub success_manifest_file_asset_id: Option<FileAssetId>,
    /// 失败对象的合规诊断包（按 30 天销毁）。
    pub failure_diagnostic_file_asset_id: Option<FileAssetId>,
    /// 本批解析、清理和映射规则版本。
    pub import_rule_version: String,
    /// 受控临时区计算的 keyed HMAC，仅用于审计去重。
    pub source_file_hmac: Option<String>,
    /// 批次状态。
    pub status: LegacyImportBatchStatus,
    /// 处理统计：总行数。
    pub total_rows: u64,
    /// 处理统计：成功行数。
    pub success_rows: u64,
    /// 处理统计：失败行数。
    pub failed_rows: u64,
    /// 脱敏错误码及计数（不含原值和行列明细）。
    pub failure_code_summary: Option<String>,
    /// 各必要 `legacy_import_confirmation` 的派生摘要。
    pub confirmation_status_summary: Option<String>,
}

impl LegacyImportBatch {
    /// 构造导入批次，清理并校验批次号、来源对象集、规则版本等必填文本。文本为空或超长、成功与失败行数之和超过总行数时拒绝，以免保存不一致的批次统计。
    pub fn new(id: erp_core::ids::LegacyImportBatchId, data: LegacyImportBatchData) -> Result<Self> {
        let batch_no =
            normalize_required_text(data.batch_no, "批次号不能为空", BATCH_NO_MAX_LEN, "批次号过长")?;
        let source_object_set = normalize_required_text(
            data.source_object_set,
            "来源对象集合不能为空",
            OBJECT_SET_MAX_LEN,
            "来源对象集合过长",
        )?;
        let import_rule_version = normalize_required_text(
            data.import_rule_version,
            "导入规则版本不能为空",
            RULE_VERSION_MAX_LEN,
            "导入规则版本过长",
        )?;
        let source_file_hmac =
            normalize_optional_text(data.source_file_hmac, "来源文件 HMAC", FILE_HMAC_MAX_LEN)?;
        let failure_code_summary =
            normalize_optional_text(data.failure_code_summary, "错误码摘要", SUMMARY_MAX_LEN)?;
        let confirmation_status_summary =
            normalize_optional_text(data.confirmation_status_summary, "确认状态摘要", SUMMARY_MAX_LEN)?;

        Self::ensure_counts(data.total_rows, data.success_rows, data.failed_rows)?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            batch_no,
            source_system_id: data.source_system_id,
            source_object_set,
            baseline_date: data.baseline_date,
            successful_sanitized_file_asset_id: None,
            success_manifest_file_asset_id: None,
            failure_diagnostic_file_asset_id: None,
            import_rule_version,
            source_file_hmac,
            status: data.status,
            total_rows: data.total_rows,
            success_rows: data.success_rows,
            failed_rows: data.failed_rows,
            failure_code_summary,
            confirmation_status_summary,
        })
    }

    /// 由批次来源对象集推导必须确认的责任范围，按稳定顺序去重；对象未登记、集合为空或解析不出范围时沿用 [`ConfirmationScope::required_for_object_set`] 的错误。
    pub fn required_confirmation_scopes(&self) -> Result<std::collections::BTreeSet<ConfirmationScope>> {
        ConfirmationScope::required_for_object_set(&self.source_object_set)
    }

    /// 识别本轮应用已经完成、部分失败或失败的结果状态；这三种状态不继续本轮应用，失败类状态仍可经重试返回待应用。
    pub fn is_terminal(&self) -> bool {
        matches!(
            self.status,
            LegacyImportBatchStatus::Completed
                | LegacyImportBatchStatus::PartialFailed
                | LegacyImportBatchStatus::Failed
        )
    }

    /// 核对批次乐观锁是否仍与客户端冻结版本一致，供命令识别过时提交。
    pub fn has_version(&self, expected: u64) -> bool {
        self.base.version == expected
    }

    /// 将试算冻结的规则版本去首尾空白后与批次当前值比较，避免应用已失效的试算。
    pub fn has_rule_version(&self, expected: &str) -> bool {
        self.import_rule_version == expected.trim()
    }

    /// 让批次进入可登记确认的阶段：待校验须先进入校验中，校验中直接推进，已待确认则幂等成功。已离开该阶段返回 `erp_core::Error::LogicError`；固定路径上的非法迁移返回 `InvalidStateTransition`。
    pub fn prepare_confirmation(&mut self) -> Result<()> {
        match self.status {
            LegacyImportBatchStatus::PendingValidation => {
                self.advance(LegacyImportBatchStatus::Validating)?;
                self.advance(LegacyImportBatchStatus::PendingConfirmation)
            },
            LegacyImportBatchStatus::Validating => self.advance(LegacyImportBatchStatus::PendingConfirmation),
            LegacyImportBatchStatus::PendingConfirmation => Ok(()),
            _ => Err(Error::from("批次已离开可确认阶段，禁止创建确认事实")),
        }
    }

    /// 识别批次是否正在逐行应用，供处理结果的入口核对阶段。
    pub fn is_importing(&self) -> bool {
        self.status == LegacyImportBatchStatus::Importing
    }

    /// 仅待确认时允许接收责任范围的决定，避免更晚阶段改写确认结果。
    pub fn accepts_confirmation_decision(&self) -> bool {
        self.status == LegacyImportBatchStatus::PendingConfirmation
    }

    /// 确认批次已到待应用阶段，供启动正式导入前检查。
    pub fn is_ready_to_apply(&self) -> bool {
        self.status == LegacyImportBatchStatus::ReadyToApply
    }

    /// 仅待应用或导入中允许取消尚未应用的项目，已结束的批次不得再取消。
    pub fn accepts_pending_cancellation(&self) -> bool {
        matches!(self.status, LegacyImportBatchStatus::ReadyToApply | LegacyImportBatchStatus::Importing)
    }

    /// 仅部分失败或失败的批次允许重新准备失败行，不把成功或未开始的批次当作重试对象。
    pub fn accepts_failed_retry(&self) -> bool {
        matches!(self.status, LegacyImportBatchStatus::PartialFailed | LegacyImportBatchStatus::Failed)
    }

    /// 由剩余待导入行与失败行数推导当前批次状态：还有待处理则保持导入中；处理完毕后无失败为完成，否则为部分失败。本规则不产生 `Failed`。
    pub fn application_outcome(pending_rows: u64, failed_rows: u64) -> LegacyImportBatchStatus {
        if pending_rows > 0 {
            LegacyImportBatchStatus::Importing
        } else if failed_rows == 0 {
            LegacyImportBatchStatus::Completed
        } else {
            LegacyImportBatchStatus::PartialFailed
        }
    }

    /// 按业务状态图推进批次，同状态重复调用成功。`Completed` 无后续迁移，`Failed` 和 `PartialFailed` 只可重试回 `ReadyToApply`；其它非法迁移返回 `InvalidStateTransition`。
    pub fn advance(&mut self, to: LegacyImportBatchStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 在落入批次汇总前校验并更新总行数、成功数和失败数；成功与失败之和超过总数时拒绝更新。
    pub fn update_counts(&mut self, total_rows: u64, success_rows: u64, failed_rows: u64) -> Result<()> {
        Self::ensure_counts(total_rows, success_rows, failed_rows)?;
        self.total_rows = total_rows;
        self.success_rows = success_rows;
        self.failed_rows = failed_rows;
        Ok(())
    }

    /// 关联成功对象的白名单包和清单资产，成功数为零时可不设；两者不得复用失败诊断包的文件资产和保留期。
    pub fn attach_success_assets(
        &mut self,
        sanitized_file_asset_id: FileAssetId,
        manifest_file_asset_id: FileAssetId,
    ) -> Result<()> {
        self.successful_sanitized_file_asset_id = Some(sanitized_file_asset_id);
        self.success_manifest_file_asset_id = Some(manifest_file_asset_id);
        Ok(())
    }

    /// 关联失败对象专用的合规诊断包，供按 30 天销毁的保留策略追踪。
    pub fn attach_failure_diagnostic_asset(&mut self, file_asset_id: FileAssetId) -> Result<()> {
        self.failure_diagnostic_file_asset_id = Some(file_asset_id);
        Ok(())
    }

    /// 保存供展示的派生摘要：错误码及计数不含原值和行列明细，确认摘要不以单一确认人替代多范围确认事实。任一摘要超长时返回错误。
    pub fn update_summaries(
        &mut self,
        failure_code_summary: Option<String>,
        confirmation_status_summary: Option<String>,
    ) -> Result<()> {
        self.failure_code_summary =
            normalize_optional_text(failure_code_summary, "错误码摘要", SUMMARY_MAX_LEN)?;
        self.confirmation_status_summary =
            normalize_optional_text(confirmation_status_summary, "确认状态摘要", SUMMARY_MAX_LEN)?;
        Ok(())
    }

    // 拒绝成功与失败行数之和大于总量的统计，保证批次创建与更新遵循同一不变式。
    fn ensure_counts(total_rows: u64, success_rows: u64, failed_rows: u64) -> Result<()> {
        if success_rows + failed_rows > total_rows {
            return Err(Error::from("成功行与失败行之和不能超过总行数"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::ensure_transition;
    use erp_core::ids::{LegacyImportBatchId, SourceSystemId};

    use super::*;

    fn batch_data() -> LegacyImportBatchData {
        LegacyImportBatchData {
            batch_no: " IMP-2026-001 ".to_string(),
            source_system_id: SourceSystemId::new("sys-mall"),
            source_object_set: " 客户,卡券销售 ".to_string(),
            baseline_date: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
            import_rule_version: " v1 ".to_string(),
            source_file_hmac: Some(" hmac-v1:abc ".to_string()),
            status: LegacyImportBatchStatus::PendingValidation,
            total_rows: 100,
            success_rows: 0,
            failed_rows: 0,
            failure_code_summary: None,
            confirmation_status_summary: None,
        }
    }

    #[test]
    fn new_trims_and_normalizes_text_fields() {
        let batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-1"), batch_data()).unwrap();

        assert_eq!(batch.batch_no, "IMP-2026-001");
        assert_eq!(batch.source_object_set, "客户,卡券销售");
        assert_eq!(batch.import_rule_version, "v1");
        assert_eq!(batch.source_file_hmac.as_deref(), Some("hmac-v1:abc"));
        assert_eq!(batch.status, LegacyImportBatchStatus::PendingValidation);
        assert!(batch.successful_sanitized_file_asset_id.is_none());
        assert_eq!(batch.base.id, "b-1");
    }

    #[test]
    fn new_rejects_empty_and_overlong_required_fields() {
        let empty_no = LegacyImportBatchData { batch_no: "   ".to_string(), ..batch_data() };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-2"), empty_no).is_err());

        let overlong_set =
            LegacyImportBatchData { source_object_set: "x".repeat(OBJECT_SET_MAX_LEN + 1), ..batch_data() };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-3"), overlong_set).is_err());

        let overlong_hmac = LegacyImportBatchData {
            source_file_hmac: Some("h".repeat(FILE_HMAC_MAX_LEN + 1)),
            ..batch_data()
        };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-4"), overlong_hmac).is_err());
    }

    #[test]
    fn new_rejects_inconsistent_counts() {
        let bad_counts =
            LegacyImportBatchData { total_rows: 10, success_rows: 8, failed_rows: 3, ..batch_data() };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-5"), bad_counts).is_err());
    }

    #[test]
    fn update_counts_enforces_sum_invariant() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-6"), batch_data()).unwrap();

        batch.update_counts(50, 40, 10).unwrap();
        assert_eq!(batch.total_rows, 50);

        assert!(batch.update_counts(10, 8, 3).is_err(), "success + failed 不能超过 total");
    }

    #[test]
    fn status_pipeline_transitions_and_terminal_states() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-7"), batch_data()).unwrap();

        batch.advance(LegacyImportBatchStatus::Validating).unwrap();
        batch.advance(LegacyImportBatchStatus::PendingConfirmation).unwrap();
        batch.advance(LegacyImportBatchStatus::ReadyToApply).unwrap();
        batch.advance(LegacyImportBatchStatus::Importing).unwrap();
        batch.advance(LegacyImportBatchStatus::Completed).unwrap();
        assert!(batch.advance(LegacyImportBatchStatus::Failed).is_err(), "完成态为终态");

        let mut failed = LegacyImportBatch::new(LegacyImportBatchId::new("b-8"), batch_data()).unwrap();
        failed.advance(LegacyImportBatchStatus::Validating).unwrap();
        failed.advance(LegacyImportBatchStatus::Failed).unwrap();
        assert!(failed.advance(LegacyImportBatchStatus::PendingConfirmation).is_err());
    }

    #[test]
    fn status_machine_directed_edges() {
        assert!(
            ensure_transition(
                LegacyImportBatchStatus::PendingValidation,
                LegacyImportBatchStatus::Validating
            )
            .is_ok()
        );
        assert!(
            ensure_transition(LegacyImportBatchStatus::Validating, LegacyImportBatchStatus::Failed).is_ok()
        );
        assert!(
            ensure_transition(LegacyImportBatchStatus::Importing, LegacyImportBatchStatus::PartialFailed)
                .is_ok()
        );
        assert!(
            ensure_transition(
                LegacyImportBatchStatus::PendingConfirmation,
                LegacyImportBatchStatus::ReadyToApply
            )
            .is_ok()
        );
        assert!(
            ensure_transition(LegacyImportBatchStatus::PartialFailed, LegacyImportBatchStatus::ReadyToApply)
                .is_ok()
        );

        assert!(
            ensure_transition(LegacyImportBatchStatus::PendingValidation, LegacyImportBatchStatus::Completed)
                .is_err(),
            "禁止跳级"
        );
        assert!(
            ensure_transition(
                LegacyImportBatchStatus::PendingConfirmation,
                LegacyImportBatchStatus::Importing
            )
            .is_err()
        );
    }

    #[test]
    fn confirmation_and_application_rules_are_owned_by_batch() {
        let mut batch =
            LegacyImportBatch::new(LegacyImportBatchId::new("batch-rules"), batch_data()).unwrap();
        assert!(!batch.is_terminal());
        assert!(batch.has_version(1));
        assert!(batch.has_rule_version(" v1 "));

        batch.prepare_confirmation().unwrap();
        assert_eq!(batch.status, LegacyImportBatchStatus::PendingConfirmation);
        assert!(batch.accepts_confirmation_decision());
        assert!(!batch.is_ready_to_apply());
        assert!(batch.required_confirmation_scopes().unwrap().contains(&ConfirmationScope::Sales));

        batch.advance(LegacyImportBatchStatus::ReadyToApply).unwrap();
        assert!(batch.is_ready_to_apply());
        assert!(batch.accepts_pending_cancellation());
        batch.advance(LegacyImportBatchStatus::Importing).unwrap();
        assert!(batch.accepts_pending_cancellation());
        batch.advance(LegacyImportBatchStatus::PartialFailed).unwrap();
        assert!(batch.accepts_failed_retry());

        assert_eq!(LegacyImportBatch::application_outcome(0, 0), LegacyImportBatchStatus::Completed);
        assert_eq!(LegacyImportBatch::application_outcome(0, 1), LegacyImportBatchStatus::PartialFailed);
        assert_eq!(LegacyImportBatch::application_outcome(1, 0), LegacyImportBatchStatus::Importing);
    }

    #[test]
    fn asset_attachments_and_summaries() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-9"), batch_data()).unwrap();

        batch.attach_success_assets(FileAssetId::new("fa-ok"), FileAssetId::new("fa-manifest")).unwrap();
        batch.attach_failure_diagnostic_asset(FileAssetId::new("fa-fail")).unwrap();
        assert_eq!(batch.successful_sanitized_file_asset_id, Some(FileAssetId::new("fa-ok")));
        assert_eq!(batch.failure_diagnostic_file_asset_id, Some(FileAssetId::new("fa-fail")));

        batch.update_summaries(Some(" CODE-1:3 ".to_string()), Some(" sales:2/3 ".to_string())).unwrap();
        assert_eq!(batch.failure_code_summary.as_deref(), Some("CODE-1:3"));
        assert_eq!(batch.confirmation_status_summary.as_deref(), Some("sales:2/3"));

        assert!(batch.update_summaries(Some("x".repeat(SUMMARY_MAX_LEN + 1)), None).is_err());
    }

    #[test]
    fn status_serde_uses_stable_codes() {
        assert_eq!(
            serde_json::to_string(&LegacyImportBatchStatus::PendingConfirmation).unwrap(),
            "\"pending_confirmation\""
        );
        assert_eq!(
            serde_json::to_string(&LegacyImportBatchStatus::PartialFailed).unwrap(),
            "\"partial_failed\""
        );
        assert_eq!(LegacyImportBatchStatus::Importing.label(), "导入中");
        assert_eq!(
            serde_json::to_string(&LegacyImportBatchStatus::ReadyToApply).unwrap(),
            "\"ready_to_apply\""
        );
    }
}
