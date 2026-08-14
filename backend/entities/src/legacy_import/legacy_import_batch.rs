//! `legacy_import_batch`：旧数据导入批次（数据模型 §6.12）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::ids::FileAssetId;
use crate::validation::{normalize_optional_text, normalize_required_text};

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
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
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

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
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
    pub source_system_id: crate::ids::SourceSystemId,
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
/// 成功白名单包与失败诊断包必须生成独立 `file_asset`，由 P3 在形成资产时写入。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct LegacyImportBatch {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 导入批次号（创建后不可修改）。
    pub batch_no: String,
    /// 来源系统。
    pub source_system_id: crate::ids::SourceSystemId,
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
    /// 创建导入批次。
    ///
    /// 完成批次号、对象集合、规则版本等文本字段的校验与规范化
    /// （去首尾空白、非空、长度上限），并强制处理统计不变式
    /// `success_rows + failed_rows <= total_rows`。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::LegacyImportBatchId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的导入批次实体。
    ///
    /// # 错误
    /// 当必填文本为空或超长，或处理统计不一致时返回错误。
    pub fn new(id: crate::ids::LegacyImportBatchId, data: LegacyImportBatchData) -> Result<Self> {
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

    /// 推进批次状态。
    ///
    /// 只允许数据模型 §6.12 固定管线内的迁移（含幂等），失败为终态。
    ///
    /// # 参数
    /// * `to` - 目标状态
    ///
    /// # 返回
    /// 迁移合法返回 `Ok(())`。
    ///
    /// # 错误
    /// 目标状态不在固定邻接矩阵中时返回 `InvalidStateTransition`。
    pub fn advance(&mut self, to: LegacyImportBatchStatus) -> Result<()> {
        ensure_transition(self.status, to)?;
        self.status = to;
        Ok(())
    }

    /// 更新处理统计。
    ///
    /// # 参数
    /// * `total_rows` - 处理总行数
    /// * `success_rows` - 成功行数
    /// * `failed_rows` - 失败行数
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当 `success_rows + failed_rows > total_rows` 时返回错误。
    pub fn update_counts(&mut self, total_rows: u64, success_rows: u64, failed_rows: u64) -> Result<()> {
        Self::ensure_counts(total_rows, success_rows, failed_rows)?;
        self.total_rows = total_rows;
        self.success_rows = success_rows;
        self.failed_rows = failed_rows;
        Ok(())
    }

    /// 登记成功对象的独立资产（成功数为零时可空）。
    ///
    /// 成功白名单包与失败诊断包必须生成独立 `file_asset`，不得混用
    /// 保留期（数据模型 §4.5.7）。
    ///
    /// # 参数
    /// * `sanitized_file_asset_id` - 成功白名单包资产
    /// * `manifest_file_asset_id` - manifest 资产
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    pub fn attach_success_assets(
        &mut self,
        sanitized_file_asset_id: FileAssetId,
        manifest_file_asset_id: FileAssetId,
    ) -> Result<()> {
        self.successful_sanitized_file_asset_id = Some(sanitized_file_asset_id);
        self.success_manifest_file_asset_id = Some(manifest_file_asset_id);
        Ok(())
    }

    /// 登记失败对象的合规诊断包资产（按 30 天销毁）。
    ///
    /// # 参数
    /// * `file_asset_id` - 失败诊断包资产
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    pub fn attach_failure_diagnostic_asset(&mut self, file_asset_id: FileAssetId) -> Result<()> {
        self.failure_diagnostic_file_asset_id = Some(file_asset_id);
        Ok(())
    }

    /// 更新脱敏错误码摘要与确认状态摘要。
    ///
    /// 两个摘要均为派生值：错误码摘要不含原值和行列明细，确认状态摘要
    /// 不保存单个确认人作为多范围事实源（§6.12）。
    ///
    /// # 参数
    /// * `failure_code_summary` - 脱敏错误码及计数
    /// * `confirmation_status_summary` - 各必要确认的派生摘要
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 摘要文本超长时返回错误。
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

    /// 校验处理统计不变式。
    ///
    /// # 参数
    /// * `total_rows` - 处理总行数
    /// * `success_rows` - 成功行数
    /// * `failed_rows` - 失败行数
    ///
    /// # 错误
    /// 当成功行与失败行之和超过总行数时返回错误。
    fn ensure_counts(total_rows: u64, success_rows: u64, failed_rows: u64) -> Result<()> {
        if success_rows + failed_rows > total_rows {
            return Err(Error::from("成功行与失败行之和不能超过总行数"));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::{LegacyImportBatchId, SourceSystemId};

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
        let empty_no = LegacyImportBatchData {
            batch_no: "   ".to_string(),
            ..batch_data()
        };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-2"), empty_no).is_err());

        let overlong_set = LegacyImportBatchData {
            source_object_set: "x".repeat(OBJECT_SET_MAX_LEN + 1),
            ..batch_data()
        };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-3"), overlong_set).is_err());

        let overlong_hmac = LegacyImportBatchData {
            source_file_hmac: Some("h".repeat(FILE_HMAC_MAX_LEN + 1)),
            ..batch_data()
        };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-4"), overlong_hmac).is_err());
    }

    #[test]
    fn new_rejects_inconsistent_counts() {
        let bad_counts = LegacyImportBatchData {
            total_rows: 10,
            success_rows: 8,
            failed_rows: 3,
            ..batch_data()
        };
        assert!(LegacyImportBatch::new(LegacyImportBatchId::new("b-5"), bad_counts).is_err());
    }

    #[test]
    fn update_counts_enforces_sum_invariant() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-6"), batch_data()).unwrap();

        batch.update_counts(50, 40, 10).unwrap();
        assert_eq!(batch.total_rows, 50);

        assert!(
            batch.update_counts(10, 8, 3).is_err(),
            "success + failed 不能超过 total"
        );
    }

    #[test]
    fn status_pipeline_transitions_and_terminal_states() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-7"), batch_data()).unwrap();

        batch.advance(LegacyImportBatchStatus::Validating).unwrap();
        batch
            .advance(LegacyImportBatchStatus::PendingConfirmation)
            .unwrap();
        batch.advance(LegacyImportBatchStatus::ReadyToApply).unwrap();
        batch.advance(LegacyImportBatchStatus::Importing).unwrap();
        batch.advance(LegacyImportBatchStatus::Completed).unwrap();

        assert!(
            batch.advance(LegacyImportBatchStatus::Failed).is_err(),
            "完成态为终态"
        );

        let mut failed = LegacyImportBatch::new(LegacyImportBatchId::new("b-8"), batch_data()).unwrap();
        failed.advance(LegacyImportBatchStatus::Validating).unwrap();
        failed.advance(LegacyImportBatchStatus::Failed).unwrap();
        assert!(failed
            .advance(LegacyImportBatchStatus::PendingConfirmation)
            .is_err());
    }

    #[test]
    fn status_machine_directed_edges() {
        assert!(ensure_transition(
            LegacyImportBatchStatus::PendingValidation,
            LegacyImportBatchStatus::Validating
        )
        .is_ok());
        assert!(ensure_transition(
            LegacyImportBatchStatus::Validating,
            LegacyImportBatchStatus::Failed
        )
        .is_ok());
        assert!(ensure_transition(
            LegacyImportBatchStatus::Importing,
            LegacyImportBatchStatus::PartialFailed
        )
        .is_ok());
        assert!(ensure_transition(
            LegacyImportBatchStatus::PendingConfirmation,
            LegacyImportBatchStatus::ReadyToApply
        )
        .is_ok());
        assert!(ensure_transition(
            LegacyImportBatchStatus::PartialFailed,
            LegacyImportBatchStatus::ReadyToApply
        )
        .is_ok());
        assert!(
            ensure_transition(
                LegacyImportBatchStatus::PendingValidation,
                LegacyImportBatchStatus::Completed
            )
            .is_err(),
            "禁止跳级"
        );
        assert!(ensure_transition(
            LegacyImportBatchStatus::PendingConfirmation,
            LegacyImportBatchStatus::Importing
        )
        .is_err());
    }

    #[test]
    fn asset_attachments_and_summaries() {
        let mut batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-9"), batch_data()).unwrap();

        batch
            .attach_success_assets(FileAssetId::new("fa-ok"), FileAssetId::new("fa-manifest"))
            .unwrap();
        batch
            .attach_failure_diagnostic_asset(FileAssetId::new("fa-fail"))
            .unwrap();
        assert_eq!(
            batch.successful_sanitized_file_asset_id,
            Some(FileAssetId::new("fa-ok"))
        );
        assert_eq!(
            batch.failure_diagnostic_file_asset_id,
            Some(FileAssetId::new("fa-fail"))
        );

        batch
            .update_summaries(Some(" CODE-1:3 ".to_string()), Some(" sales:2/3 ".to_string()))
            .unwrap();
        assert_eq!(batch.failure_code_summary.as_deref(), Some("CODE-1:3"));
        assert_eq!(batch.confirmation_status_summary.as_deref(), Some("sales:2/3"));

        assert!(batch
            .update_summaries(Some("x".repeat(SUMMARY_MAX_LEN + 1)), None)
            .is_err());
    }

    #[test]
    fn bson_roundtrip_preserves_entity() {
        let batch = LegacyImportBatch::new(LegacyImportBatchId::new("b-10"), batch_data()).unwrap();
        let roundtrip: LegacyImportBatch = bson::from_document(bson::to_document(&batch).unwrap()).unwrap();
        assert_eq!(roundtrip, batch);
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
