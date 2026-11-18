//! `legacy_import_row`：旧数据导入行（数据模型 §6.12）。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::state::{DocumentState, ensure_transition};
use erp_core::ids::{ExternalIdentityMapId, LegacyImportBatchId};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

/// 来源对象类型最大长度（客户、供应商、SPU、SKU、卡券销售等）。
const OBJECT_TYPE_MAX_LEN: usize = 64;
/// 来源行键最大长度。
const ROW_KEY_MAX_LEN: usize = 256;
/// 规范化载荷引用最大长度（仅含白名单字段的规范化行）。
const PAYLOAD_MAX_LEN: usize = 65536;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 64;
/// 错误明细最大长度。
const ERROR_DETAIL_MAX_LEN: usize = 1024;
/// 目标对象 ID 与引用最大长度。
const TARGET_MAX_LEN: usize = 512;

/// 解析状态（数据模型 §6.12：待解析、有效、无效）。
///
/// 固定状态机：待解析单向推进到有效或无效；无效为终态，
/// 修复后重跑使用原批次或明确的修复批次（§6.12）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ParseStatus {
    /// 待解析。
    PendingParse,
    /// 有效。
    Valid,
    /// 无效。
    Invalid,
}

impl ParseStatus {
    /// 提供行解析状态的中文文案，供进度及差异展示。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingParse => "待解析",
            Self::Valid => "有效",
            Self::Invalid => "无效",
        }
    }

    /// 提供解析状态的稳定代码，供行存储和按解析进度筛选复用。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingParse => "pending_parse",
            Self::Valid => "valid",
            Self::Invalid => "invalid",
        }
    }
}

impl DocumentState for ParseStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::PendingParse => &[Self::Valid, Self::Invalid],
            Self::Valid | Self::Invalid => &[],
        }
    }
}

/// 映射状态（数据模型 §6.12：待映射、已映射、冲突）。
///
/// 固定状态机：待映射单向推进到已映射或冲突。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingStatus {
    /// 待映射。
    PendingMapping,
    /// 已映射。
    Mapped,
    /// 冲突。
    Conflict,
}

impl MappingStatus {
    /// 提供行身份映射状态的中文文案，供识别映射成功与冲突。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingMapping => "待映射",
            Self::Mapped => "已映射",
            Self::Conflict => "冲突",
        }
    }

    /// 提供映射状态的稳定代码，供仓储查询及处理队列识别。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingMapping => "pending_mapping",
            Self::Mapped => "mapped",
            Self::Conflict => "conflict",
        }
    }
}

impl DocumentState for MappingStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::PendingMapping => &[Self::Mapped, Self::Conflict],
            Self::Mapped | Self::Conflict => &[],
        }
    }
}

/// 导入状态（数据模型 §6.12：待导入、已导入、失败、跳过）。
///
/// 固定状态机：待导入推进到已导入、失败或跳过；失败行可由
/// `RETRY_FAILED` 强命令重新准备为待导入，已导入与已跳过行不回退。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportStatus {
    /// 待导入。
    PendingImport,
    /// 已导入。
    Imported,
    /// 失败。
    Failed,
    /// 跳过。
    Skipped,
}

impl ImportStatus {
    /// 提供行应用状态的中文文案，供展示成功、失败和跳过等结果。
    pub fn label(&self) -> &'static str {
        match self {
            Self::PendingImport => "待导入",
            Self::Imported => "已导入",
            Self::Failed => "失败",
            Self::Skipped => "跳过",
        }
    }

    /// 提供行应用状态的稳定代码，供状态写入、筛选及重试查询复用。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::PendingImport => "pending_import",
            Self::Imported => "imported",
            Self::Failed => "failed",
            Self::Skipped => "skipped",
        }
    }
}

impl DocumentState for ImportStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::PendingImport => &[Self::Imported, Self::Failed, Self::Skipped],
            Self::Failed => &[Self::PendingImport],
            Self::Imported | Self::Skipped => &[],
        }
    }
}

/// 导入行创建数据（数据模型 §6.12）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct LegacyImportRowData {
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 来源对象类型（客户、供应商、SPU、SKU、卡券销售等）。
    pub source_object_type: String,
    /// 批次内来源行身份。
    pub source_row_key: String,
    /// 仅含白名单字段的规范化行（引用或内联）。
    pub normalized_payload_reference: String,
}

/// 旧数据导入行实体（数据模型 §6.12）。
///
/// 行是批次内来源行处理轨迹的历史记录：核心内容创建后不可修改，
/// 只允许按固定状态机推进解析/映射/导入三个独立处理维度，
/// 并登记对应的来源身份、错误与成功结果（`update` 受限）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct LegacyImportRow {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属导入批次。
    pub batch_id: LegacyImportBatchId,
    /// 来源对象类型。
    pub source_object_type: String,
    /// 批次内来源行身份（与批次、对象类型构成唯一约束）。
    pub source_row_key: String,
    /// 仅含白名单字段的规范化行。
    pub normalized_payload_reference: String,
    /// 解析状态。
    pub parse_status: ParseStatus,
    /// 映射状态。
    pub mapping_status: MappingStatus,
    /// 导入状态。
    pub import_status: ImportStatus,
    /// 来源稳定身份（映射成功后登记）。
    pub external_identity_map_id: Option<ExternalIdentityMapId>,
    /// 失败原因错误码。
    pub error_code: Option<String>,
    /// 失败原因明细。
    pub error_detail: Option<String>,
    /// 成功结果目标单据 ID。
    pub target_document_id: Option<String>,
    /// 成功结果目标对象引用。
    pub target_object_reference: Option<String>,
}

impl LegacyImportRow {
    /// 构造批次下的一行导入记录，解析、映射和应用均从待处理开始；来源对象类型、行键及规范化载荷去首尾空白后为空或超长则拒绝。
    pub fn new(id: erp_core::ids::LegacyImportRowId, data: LegacyImportRowData) -> Result<Self> {
        let source_object_type = normalize_required_text(
            data.source_object_type,
            "来源对象类型不能为空",
            OBJECT_TYPE_MAX_LEN,
            "来源对象类型过长",
        )?;
        let source_row_key = normalize_required_text(
            data.source_row_key,
            "来源行键不能为空",
            ROW_KEY_MAX_LEN,
            "来源行键过长",
        )?;
        let normalized_payload_reference = normalize_required_text(
            data.normalized_payload_reference,
            "缺少整理后的导入行内容，请核对导入文件后重新解析",
            PAYLOAD_MAX_LEN,
            "规范化载荷过长",
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            batch_id: data.batch_id,
            source_object_type,
            source_row_key,
            normalized_payload_reference,
            parse_status: ParseStatus::PendingParse,
            mapping_status: MappingStatus::PendingMapping,
            import_status: ImportStatus::PendingImport,
            external_identity_map_id: None,
            error_code: None,
            error_detail: None,
            target_document_id: None,
            target_object_reference: None,
        })
    }

    /// 登记行解析结果，合法结果为有效或无效，同状态可重复登记。有效清空诊断；无效必须有错误码，如类型不合法、金额或税额不平、外部身份重复、明细数量异常或状态不可识别。
    /// 非法迁移返回 `InvalidStateTransition`；无效行缺错误码或诊断超长时返回错误。
    pub fn mark_parse_result(
        &mut self,
        status: ParseStatus,
        error_code: Option<String>,
        error_detail: Option<String>,
    ) -> Result<()> {
        if status == ParseStatus::Invalid {
            return Self::write_diagnostic(
                &mut self.parse_status,
                ParseStatus::Invalid,
                &mut self.error_code,
                &mut self.error_detail,
                error_code.unwrap_or_default(),
                error_detail,
            );
        }

        ensure_transition(self.parse_status, status)?;
        self.error_code = None;
        self.error_detail = None;
        self.parse_status = status;
        Ok(())
    }

    /// 仅为解析有效行绑定 `external_identity_map` 的稳定来源身份，以便正式对象追溯来源。待映射或已映射时可登记，重复登记覆盖来源身份并清空诊断；其它映射状态返回 `InvalidStateTransition`，解析无效也拒绝。
    pub fn mark_mapped(&mut self, external_identity_map_id: ExternalIdentityMapId) -> Result<()> {
        Self::ensure_parseable(self)?;
        ensure_transition(self.mapping_status, MappingStatus::Mapped)?;
        self.mapping_status = MappingStatus::Mapped;
        self.external_identity_map_id = Some(external_identity_map_id);
        self.error_code = None;
        self.error_detail = None;
        Ok(())
    }

    /// 给已解析有效的行登记映射冲突，保留原值、解析错误、来源文件和行号诊断，冲突解决前不形成正式目标。状态非法、缺错误码或诊断超长时失败；已是冲突时允许覆盖诊断。
    pub fn mark_conflict(&mut self, error_code: String, error_detail: Option<String>) -> Result<()> {
        Self::ensure_parseable(self)?;
        Self::write_diagnostic(
            &mut self.mapping_status,
            MappingStatus::Conflict,
            &mut self.error_code,
            &mut self.error_detail,
            error_code,
            error_detail,
        )
    }

    /// 为解析有效且已映射的行记录成功目标并清除诊断；仅待导入或已导入允许登记，否则返回 `InvalidStateTransition`。
    /// 先推进状态再验证目标文本：单据 ID 为空或超长、引用超长时返回错误，行可能已变为已导入，调用方不得保存失败的修改；合法重复调用可成功。
    pub fn mark_imported(
        &mut self,
        target_document_id: String,
        target_object_reference: Option<String>,
    ) -> Result<()> {
        Self::ensure_parseable(self)?;
        Self::ensure_mapped(self)?;
        ensure_transition(self.import_status, ImportStatus::Imported)?;

        self.import_status = ImportStatus::Imported;
        self.target_document_id = Some(normalize_required_text(
            target_document_id,
            "目标单据 ID 不能为空",
            TARGET_MAX_LEN,
            "目标单据 ID 过长",
        )?);
        self.target_object_reference =
            normalize_optional_text(target_object_reference, "目标对象引用", TARGET_MAX_LEN)?;
        self.error_code = None;
        self.error_detail = None;
        Ok(())
    }

    /// 为解析有效且已映射的行登记应用失败，保留规范化载荷和诊断，失败合规包及行列明细保留 30 天。仅待导入或已失败可登记，后者覆盖诊断；非法迁移返回 `InvalidStateTransition`，前置状态无效、缺错误码或文本超长时也拒绝。
    pub fn mark_import_failed(&mut self, error_code: String, error_detail: Option<String>) -> Result<()> {
        Self::ensure_parseable(self)?;
        Self::ensure_mapped(self)?;
        Self::write_diagnostic(
            &mut self.import_status,
            ImportStatus::Failed,
            &mut self.error_code,
            &mut self.error_detail,
            error_code,
            error_detail,
        )
    }

    /// 记录按规则无需应用的行（如重复来源身份的幂等处理）。仅解析有效、已映射且待导入或已跳过的行可登记，重复跳过覆盖诊断。非法迁移返回 `InvalidStateTransition`；缺错误码、诊断超长或前置状态不满足时也拒绝。
    pub fn mark_skipped(&mut self, error_code: String, error_detail: Option<String>) -> Result<()> {
        Self::ensure_parseable(self)?;
        Self::ensure_mapped(self)?;
        Self::write_diagnostic(
            &mut self.import_status,
            ImportStatus::Skipped,
            &mut self.error_code,
            &mut self.error_detail,
            error_code,
            error_detail,
        )
    }

    /// 将失败行重置为待导入，保留解析、映射和来源身份，清除错误与正式目标字段；原为待导入时可重复准备，已导入或已跳过返回 `InvalidStateTransition`。
    pub fn prepare_failed_retry(&mut self) -> Result<()> {
        ensure_transition(self.import_status, ImportStatus::PendingImport)?;
        self.import_status = ImportStatus::PendingImport;
        self.error_code = None;
        self.error_detail = None;
        self.target_document_id = None;
        self.target_object_reference = None;
        Ok(())
    }

    /// 用试算结果把行推进为解析有效、映射完成：待解析行按成功登记，待映射行必须有稳定来源身份。已无效、有映射冲突或缺所需身份时拒绝进入应用。
    pub fn prepare_for_import(
        &mut self,
        external_identity_map_id: Option<ExternalIdentityMapId>,
    ) -> Result<()> {
        if self.parse_status == ParseStatus::PendingParse {
            self.mark_parse_result(ParseStatus::Valid, None, None)?;
        }
        Self::ensure_parseable(self)?;

        if self.mapping_status == MappingStatus::PendingMapping {
            let identity = external_identity_map_id
                .ok_or_else(|| Error::from("待匹配的导入行缺少来源记录标识，请补齐后再处理"))?;
            self.mark_mapped(identity)?;
        }
        Self::ensure_mapped(self)
    }

    /// 统计给定导入行中匹配指定应用状态的数量，供批次汇总处理结果。
    pub fn count_by_import_status(rows: &[Self], status: ImportStatus) -> u64 {
        rows.iter().filter(|row| row.import_status == status).count() as u64
    }

    /// 统计仍为 `PendingImport` 的行，供批次判断是否还有待应用项。
    pub fn pending_import_count(rows: &[Self]) -> u64 {
        Self::count_by_import_status(rows, ImportStatus::PendingImport)
    }

    // 在登记映射或应用结果前要求行解析有效，避免无效数据推进；其它解析状态返回错误。
    fn ensure_parseable(row: &Self) -> Result<()> {
        if row.parse_status != ParseStatus::Valid {
            return Err(Error::from("无效行不能进入映射或导入"));
        }
        Ok(())
    }

    // 在登记应用结果前要求行已完成来源映射，防止未定位正式身份便更新结果。
    fn ensure_mapped(row: &Self) -> Result<()> {
        if row.mapping_status != MappingStatus::Mapped {
            return Err(Error::from("未完成映射的行不能导入"));
        }
        Ok(())
    }

    // 让解析无效、映射冲突、导入失败和跳过共用诊断写入顺序。调用方先校验分支的解析、映射前置状态；本方法先校验迁移，再校验错误码和明细，两项通过后才写状态与诊断，非法输入直接返回错误。
    fn write_diagnostic<S: DocumentState + 'static>(
        status: &mut S,
        next: S,
        error_code_slot: &mut Option<String>,
        error_detail_slot: &mut Option<String>,
        error_code: String,
        error_detail: Option<String>,
    ) -> Result<()> {
        ensure_transition(*status, next)?;
        let error_code =
            normalize_required_text(error_code, "错误码不能为空", ERROR_CODE_MAX_LEN, "错误码过长")?;
        let error_detail = normalize_optional_text(error_detail, "错误明细", ERROR_DETAIL_MAX_LEN)?;

        *status = next;
        *error_code_slot = Some(error_code);
        *error_detail_slot = error_detail;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use erp_core::common::state::ensure_transition;
    use erp_core::ids::{ExternalIdentityMapId, LegacyImportBatchId, LegacyImportRowId};

    use super::*;

    fn row_data() -> LegacyImportRowData {
        LegacyImportRowData {
            batch_id: LegacyImportBatchId::new("batch-1"),
            source_object_type: " 卡券销售 ".to_string(),
            source_row_key: " sell-001 ".to_string(),
            normalized_payload_reference: " {\"sell_order\":\"S1\"} ".to_string(),
        }
    }

    #[test]
    fn new_trims_and_starts_all_pending() {
        let row = LegacyImportRow::new(LegacyImportRowId::new("row-1"), row_data()).unwrap();

        assert_eq!(row.source_object_type, "卡券销售");
        assert_eq!(row.source_row_key, "sell-001");
        assert_eq!(row.normalized_payload_reference, "{\"sell_order\":\"S1\"}");
        assert_eq!(row.parse_status, ParseStatus::PendingParse);
        assert_eq!(row.mapping_status, MappingStatus::PendingMapping);
        assert_eq!(row.import_status, ImportStatus::PendingImport);
        assert!(row.external_identity_map_id.is_none());
        assert!(row.target_document_id.is_none());
    }

    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let empty_key = LegacyImportRowData { source_row_key: "   ".to_string(), ..row_data() };
        assert!(LegacyImportRow::new(LegacyImportRowId::new("row-2"), empty_key).is_err());

        let overlong_type =
            LegacyImportRowData { source_object_type: "x".repeat(OBJECT_TYPE_MAX_LEN + 1), ..row_data() };
        assert!(LegacyImportRow::new(LegacyImportRowId::new("row-3"), overlong_type).is_err());

        let overlong_payload = LegacyImportRowData {
            normalized_payload_reference: "x".repeat(PAYLOAD_MAX_LEN + 1),
            ..row_data()
        };
        assert!(LegacyImportRow::new(LegacyImportRowId::new("row-4"), overlong_payload).is_err());
    }

    #[test]
    fn invalid_row_requires_error_code_and_blocks_mapping() {
        let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-5"), row_data()).unwrap();

        assert!(row.mark_parse_result(ParseStatus::Invalid, None, None).is_err(), "无效行必须给出错误码");
        row.mark_parse_result(
            ParseStatus::Invalid,
            Some(" AMOUNT_NOT_CONSERVED ".to_string()),
            Some(" 金额不守恒 ".to_string()),
        )
        .unwrap();
        assert_eq!(row.error_code.as_deref(), Some("AMOUNT_NOT_CONSERVED"));

        assert!(row.mark_mapped(ExternalIdentityMapId::new("map-1")).is_err(), "无效行不能进入映射");
        assert!(row.mark_imported("SO-1".to_string(), None).is_err());
    }

    #[test]
    fn full_pipeline_to_imported() {
        let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-6"), row_data()).unwrap();

        row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        assert!(row.error_code.is_none(), "有效行清除错误信息");

        row.mark_mapped(ExternalIdentityMapId::new("map-1")).unwrap();
        assert_eq!(row.external_identity_map_id, Some(ExternalIdentityMapId::new("map-1")));

        row.mark_imported(" SO-100 ".to_string(), Some(" sales/so-100 ".to_string())).unwrap();
        assert_eq!(row.target_document_id.as_deref(), Some("SO-100"));
        assert_eq!(row.target_object_reference.as_deref(), Some("sales/so-100"));
        assert_eq!(row.import_status, ImportStatus::Imported);
    }

    #[test]
    fn failed_row_can_be_prepared_for_retry_without_reverting_mapping() {
        let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-retry"), row_data()).unwrap();
        row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        row.mark_mapped(ExternalIdentityMapId::new("map-retry")).unwrap();
        row.mark_import_failed("TEMPORARY_FAILURE".to_string(), Some("可重试".to_string())).unwrap();

        row.prepare_failed_retry().unwrap();
        assert_eq!(row.import_status, ImportStatus::PendingImport);
        assert_eq!(row.parse_status, ParseStatus::Valid);
        assert_eq!(row.mapping_status, MappingStatus::Mapped);
        assert_eq!(row.external_identity_map_id, Some(ExternalIdentityMapId::new("map-retry")));
        assert!(row.error_code.is_none());
        assert!(row.error_detail.is_none());
    }

    #[test]
    fn imported_and_skipped_rows_cannot_be_prepared_for_retry() {
        let mut imported = LegacyImportRow::new(LegacyImportRowId::new("row-imported"), row_data()).unwrap();
        imported.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        imported.mark_mapped(ExternalIdentityMapId::new("map-imported")).unwrap();
        imported.mark_imported("SO-1".to_string(), None).unwrap();
        assert!(imported.prepare_failed_retry().is_err());

        let mut skipped = LegacyImportRow::new(LegacyImportRowId::new("row-skipped"), row_data()).unwrap();
        skipped.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        skipped.mark_mapped(ExternalIdentityMapId::new("map-skipped")).unwrap();
        skipped.mark_skipped("DUPLICATE".to_string(), None).unwrap();
        assert!(skipped.prepare_failed_retry().is_err());
    }

    #[test]
    fn prepare_for_import_advances_pending_row_and_requires_identity() {
        let mut missing =
            LegacyImportRow::new(LegacyImportRowId::new("row-prepare-missing"), row_data()).unwrap();
        assert!(missing.prepare_for_import(None).is_err());
        assert_eq!(missing.parse_status, ParseStatus::Valid);
        assert_eq!(missing.mapping_status, MappingStatus::PendingMapping);

        let mut ready =
            LegacyImportRow::new(LegacyImportRowId::new("row-prepare-ready"), row_data()).unwrap();
        ready.prepare_for_import(Some(ExternalIdentityMapId::new("map-ready"))).unwrap();
        assert_eq!(ready.parse_status, ParseStatus::Valid);
        assert_eq!(ready.mapping_status, MappingStatus::Mapped);
    }

    #[test]
    fn prepare_for_import_rejects_invalid_or_conflicting_rows() {
        let mut invalid =
            LegacyImportRow::new(LegacyImportRowId::new("row-prepare-invalid"), row_data()).unwrap();
        invalid.mark_parse_result(ParseStatus::Invalid, Some("INVALID".to_string()), None).unwrap();
        assert!(invalid.prepare_for_import(None).is_err());

        let mut conflict =
            LegacyImportRow::new(LegacyImportRowId::new("row-prepare-conflict"), row_data()).unwrap();
        conflict.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        conflict.mark_conflict("IDENTITY_CONFLICT".to_string(), None).unwrap();
        assert!(conflict.prepare_for_import(None).is_err());
    }

    #[test]
    fn import_status_counts_are_deterministic() {
        let mut imported =
            LegacyImportRow::new(LegacyImportRowId::new("row-count-imported"), row_data()).unwrap();
        imported.prepare_for_import(Some(ExternalIdentityMapId::new("map-count"))).unwrap();
        imported.mark_imported("SO-COUNT".to_string(), None).unwrap();
        let pending = LegacyImportRow::new(LegacyImportRowId::new("row-count-pending"), row_data()).unwrap();
        let rows = vec![imported, pending];

        assert_eq!(LegacyImportRow::count_by_import_status(&rows, ImportStatus::Imported), 1);
        assert_eq!(LegacyImportRow::pending_import_count(&rows), 1);
    }

    #[test]
    fn import_requires_mapping_first() {
        let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-7"), row_data()).unwrap();
        row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();

        assert!(row.mark_imported("SO-1".to_string(), None).is_err(), "未映射不能导入");
    }

    #[test]
    fn conflict_blocks_import_until_re_run() {
        let mut row = LegacyImportRow::new(LegacyImportRowId::new("row-8"), row_data()).unwrap();
        row.mark_parse_result(ParseStatus::Valid, None, None).unwrap();

        row.mark_conflict(" IDENTITY_CONFLICT ".to_string(), None).unwrap();
        assert_eq!(row.mapping_status, MappingStatus::Conflict);
        assert_eq!(row.error_code.as_deref(), Some("IDENTITY_CONFLICT"));

        assert!(row.mark_imported("SO-1".to_string(), None).is_err(), "冲突未解决不导入");
        assert!(row.mark_mapped(ExternalIdentityMapId::new("map-2")).is_err());
    }

    #[test]
    fn skip_and_fail_require_error_code() {
        let mut failed = LegacyImportRow::new(LegacyImportRowId::new("row-9"), row_data()).unwrap();
        failed.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        failed.mark_mapped(ExternalIdentityMapId::new("map-1")).unwrap();
        failed.mark_import_failed(" IMPORT_IO_ERROR ".to_string(), None).unwrap();
        assert_eq!(failed.import_status, ImportStatus::Failed);

        let mut skipped = LegacyImportRow::new(LegacyImportRowId::new("row-10"), row_data()).unwrap();
        skipped.mark_parse_result(ParseStatus::Valid, None, None).unwrap();
        skipped.mark_mapped(ExternalIdentityMapId::new("map-1")).unwrap();
        assert!(skipped.mark_skipped(String::new(), None).is_err(), "跳过必须说明原因");
        skipped.mark_skipped(" DUPLICATE ".to_string(), None).unwrap();
        assert_eq!(skipped.import_status, ImportStatus::Skipped);
    }

    #[test]
    fn status_machines_are_directed() {
        assert!(ensure_transition(ParseStatus::PendingParse, ParseStatus::Valid).is_ok());
        assert!(ensure_transition(ParseStatus::PendingParse, ParseStatus::Invalid).is_ok());
        assert!(ensure_transition(ParseStatus::Valid, ParseStatus::Invalid).is_err(), "无效为终态");

        assert!(ensure_transition(MappingStatus::PendingMapping, MappingStatus::Conflict).is_ok());
        assert!(ensure_transition(MappingStatus::Conflict, MappingStatus::Mapped).is_err());

        assert!(ensure_transition(ImportStatus::PendingImport, ImportStatus::Skipped).is_ok());
        assert!(ensure_transition(ImportStatus::Failed, ImportStatus::Imported).is_err());
    }

    #[test]
    fn status_serde_uses_stable_codes() {
        assert_eq!(serde_json::to_string(&ParseStatus::PendingParse).unwrap(), "\"pending_parse\"");
        assert_eq!(serde_json::to_string(&MappingStatus::Conflict).unwrap(), "\"conflict\"");
        assert_eq!(serde_json::to_string(&ImportStatus::Skipped).unwrap(), "\"skipped\"");
        assert_eq!(ImportStatus::Imported.label(), "已导入");
        assert_eq!(ParseStatus::Valid.label(), "有效");
    }
}
