//! 导入批次应用结果集（INT-E25）。
//!
//! 在查库前强制行 ID 唯一，并按 `imported` / `failed` / `skipped` 精确字段形状
//! 收紧输入。不访问数据库、时钟或 HTTP；ID 是否属于目标批次由 Service /
//! Repository 再确认。

use erp_core::ids::{ExternalIdentityMapId, LegacyImportRowId};
use erp_core::validation::{normalize_optional_text, normalize_required_text};
use erp_core::{Error, Result};

use super::first_duplicate_key;

/// 单次应用请求允许的最大行结果数（与 HTTP 契约 1–1000 对齐）。
const APPLY_RESULT_MAX_LEN: usize = 1000;
/// 目标对象 ID 与引用最大长度（与导入行实体一致）。
const TARGET_MAX_LEN: usize = 512;
/// 错误码最大长度。
const ERROR_CODE_MAX_LEN: usize = 64;
/// 错误明细最大长度。
const ERROR_DETAIL_MAX_LEN: usize = 1024;

/// 行级导入结果草稿（尚未按 outcome 收紧字段形状）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResultDraft {
    /// 目标导入行。
    pub row_id: LegacyImportRowId,
    /// 行级结果。
    pub outcome: ApplyResultOutcome,
    /// 来源稳定身份（待映射行必填，由行实体在应用时校验）。
    pub external_identity_map_id: Option<ExternalIdentityMapId>,
    /// 成功结果目标单据 ID。
    pub target_document_id: Option<String>,
    /// 成功结果目标对象引用。
    pub target_object_reference: Option<String>,
    /// 失败或跳过原因错误码。
    pub error_code: Option<String>,
    /// 失败或跳过原因明细。
    pub error_detail: Option<String>,
}

/// 行级导入结果（与 wire `snake_case` 语义对齐，不含 HTTP 形态）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApplyResultOutcome {
    /// 已导入。
    Imported,
    /// 失败。
    Failed,
    /// 跳过。
    Skipped,
}

/// 已按 outcome 收紧字段形状的单行应用结果。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ApplyResultItem {
    /// 导入成功，必须携带目标单据 ID；禁止错误码/明细。
    Imported {
        /// 目标导入行。
        row_id: LegacyImportRowId,
        /// 来源稳定身份。
        external_identity_map_id: Option<ExternalIdentityMapId>,
        /// 成功结果目标单据 ID。
        target_document_id: String,
        /// 成功结果目标对象引用。
        target_object_reference: Option<String>,
    },
    /// 导入失败，必须携带错误码；禁止目标单据字段。
    Failed {
        /// 目标导入行。
        row_id: LegacyImportRowId,
        /// 来源稳定身份。
        external_identity_map_id: Option<ExternalIdentityMapId>,
        /// 失败原因错误码。
        error_code: String,
        /// 失败原因明细。
        error_detail: Option<String>,
    },
    /// 跳过，必须携带原因错误码；禁止目标单据字段。
    Skipped {
        /// 目标导入行。
        row_id: LegacyImportRowId,
        /// 来源稳定身份。
        external_identity_map_id: Option<ExternalIdentityMapId>,
        /// 跳过原因错误码。
        error_code: String,
        /// 跳过原因明细。
        error_detail: Option<String>,
    },
}

impl ApplyResultItem {
    /// 提供结果关联的导入行 ID，便于在批次内定位待写回的行。
    pub fn row_id(&self) -> &LegacyImportRowId {
        match self {
            Self::Imported { row_id, .. } | Self::Failed { row_id, .. } | Self::Skipped { row_id, .. } => {
                row_id
            },
        }
    }

    /// 提供成功、失败或跳过结果携带的来源映射 ID，供追溯原对象；未提供则返回 `None`。
    pub fn external_identity_map_id(&self) -> Option<&ExternalIdentityMapId> {
        match self {
            Self::Imported { external_identity_map_id, .. }
            | Self::Failed { external_identity_map_id, .. }
            | Self::Skipped { external_identity_map_id, .. } => external_identity_map_id.as_ref(),
        }
    }
}

/// 已通过唯一 ID 与 outcome 字段形状校验的应用结果集。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResultSet {
    items: Vec<ApplyResultItem>,
}

impl ApplyResultSet {
    /// 将行级草稿按请求顺序收敛为可应用的结果集。先拒绝空集、超量或重复行 ID（包括同 ID 不同结果），再校验 `imported`、`failed`、`skipped` 各自字段；缺必填项、传入禁用字段或文本超长均返回 [`Error::LogicError`]。这里只做确定性校验，不访问数据库或核对行归属。
    pub fn try_from_drafts(drafts: Vec<ApplyResultDraft>) -> Result<Self> {
        if drafts.is_empty() || drafts.len() > APPLY_RESULT_MAX_LEN {
            return Err(Error::from("行结果数量必须在1-1000之间"));
        }
        ensure_unique_row_ids(&drafts)?;

        let mut items = Vec::with_capacity(drafts.len());
        for draft in drafts {
            items.push(ApplyResultItem::try_from_draft(draft)?);
        }
        Ok(Self { items })
    }

    /// 按请求原顺序借用已校验行结果，供应用步骤逐行处理。
    pub fn items(&self) -> &[ApplyResultItem] {
        &self.items
    }

    /// 按请求顺序取出与结果一一对应的行 ID，供仓储在目标批次内定位待应用行。
    pub fn row_ids(&self) -> Vec<LegacyImportRowId> {
        self.items.iter().map(|item| item.row_id().clone()).collect()
    }
}

impl ApplyResultItem {
    // 按结果区分单行字段：成功必须有目标单据且不能带错误，失败或跳过必须有原因代码且不能带目标。缺项、多传或文本非法均拒绝。
    fn try_from_draft(draft: ApplyResultDraft) -> Result<Self> {
        let ApplyResultDraft {
            row_id,
            outcome,
            external_identity_map_id,
            target_document_id,
            target_object_reference,
            error_code,
            error_detail,
        } = draft;

        match outcome {
            ApplyResultOutcome::Imported => {
                ensure_absent(error_code.as_deref(), "导入成功结果不得携带失败错误码")?;
                ensure_absent(error_detail.as_deref(), "导入成功结果不得携带失败明细")?;
                Ok(Self::Imported {
                    row_id,
                    external_identity_map_id,
                    target_document_id: normalize_required_text(
                        target_document_id.unwrap_or_default(),
                        "导入成功结果必须提供目标单据 ID",
                        TARGET_MAX_LEN,
                        "目标单据 ID 过长",
                    )?,
                    target_object_reference: normalize_optional_text(
                        target_object_reference,
                        "目标对象引用",
                        TARGET_MAX_LEN,
                    )?,
                })
            },
            ApplyResultOutcome::Failed => {
                ensure_absent(target_document_id.as_deref(), "失败结果不得携带目标单据 ID")?;
                ensure_absent(target_object_reference.as_deref(), "失败结果不得携带目标对象引用")?;
                Ok(Self::Failed {
                    row_id,
                    external_identity_map_id,
                    error_code: required_error_code(error_code, "失败结果必须提供错误码")?,
                    error_detail: optional_error_detail(error_detail)?,
                })
            },
            ApplyResultOutcome::Skipped => {
                ensure_absent(target_document_id.as_deref(), "跳过结果不得携带目标单据 ID")?;
                ensure_absent(target_object_reference.as_deref(), "跳过结果不得携带目标对象引用")?;
                Ok(Self::Skipped {
                    row_id,
                    external_identity_map_id,
                    error_code: required_error_code(error_code, "跳过结果必须提供原因错误码")?,
                    error_detail: optional_error_detail(error_detail)?,
                })
            },
        }
    }
}

// 按原值检查行 ID；同一行出现两次即失败，不能通过去重掩盖重复或矛盾结果。
fn ensure_unique_row_ids(drafts: &[ApplyResultDraft]) -> Result<()> {
    if let Some(duplicate) = first_duplicate_key(drafts, |draft| draft.row_id.as_ref()) {
        return Err(Error::from(format!("行结果 ID 重复: {duplicate}")));
    }
    Ok(())
}

// 校验该结果禁止出现的字段，空白等同未提供，含实际内容则拒绝。
fn ensure_absent(value: Option<&str>, message: &str) -> Result<()> {
    if value.is_some_and(|text| !text.trim().is_empty()) {
        return Err(Error::from(message));
    }
    Ok(())
}

// 清理失败或跳过的原因代码，保证诊断可分类；缺失或超长时返回错误。
fn required_error_code(value: Option<String>, empty_message: &str) -> Result<String> {
    normalize_required_text(value.unwrap_or_default(), empty_message, ERROR_CODE_MAX_LEN, "错误码过长")
}

// 清理可选的行级诊断文本，空白视为无明细，超长则拒绝。
fn optional_error_detail(value: Option<String>) -> Result<Option<String>> {
    normalize_optional_text(value, "错误明细", ERROR_DETAIL_MAX_LEN)
}

#[cfg(test)]
mod tests {
    use erp_core::ids::{ExternalIdentityMapId, LegacyImportRowId};

    use super::{ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet};

    fn draft(id: &str, outcome: ApplyResultOutcome) -> ApplyResultDraft {
        ApplyResultDraft {
            row_id: LegacyImportRowId::new(id),
            outcome,
            external_identity_map_id: None,
            target_document_id: None,
            target_object_reference: None,
            error_code: None,
            error_detail: None,
        }
    }

    fn imported(id: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            target_document_id: Some(" SO-1 ".to_string()),
            target_object_reference: Some(" sales/so-1 ".to_string()),
            external_identity_map_id: Some(ExternalIdentityMapId::new("map-1")),
            ..draft(id, ApplyResultOutcome::Imported)
        }
    }

    fn failed(id: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            error_code: Some(" TEMPORARY ".to_string()),
            error_detail: Some(" 可重试 ".to_string()),
            ..draft(id, ApplyResultOutcome::Failed)
        }
    }

    fn skipped(id: &str) -> ApplyResultDraft {
        ApplyResultDraft {
            error_code: Some(" DUPLICATE ".to_string()),
            ..draft(id, ApplyResultOutcome::Skipped)
        }
    }

    #[test]
    fn accepts_all_outcomes_with_exact_field_shapes() {
        let set = ApplyResultSet::try_from_drafts(vec![imported("row-1"), failed("row-2"), skipped("row-3")])
            .unwrap();
        assert_eq!(set.row_ids().len(), 3);

        match &set.items()[0] {
            ApplyResultItem::Imported { target_document_id, target_object_reference, .. } => {
                assert_eq!(target_document_id, "SO-1");
                assert_eq!(target_object_reference.as_deref(), Some("sales/so-1"));
            },
            other => panic!("expected imported, got {other:?}"),
        }

        match &set.items()[1] {
            ApplyResultItem::Failed { error_code, error_detail, .. } => {
                assert_eq!(error_code, "TEMPORARY");
                assert_eq!(error_detail.as_deref(), Some("可重试"));
            },
            other => panic!("expected failed, got {other:?}"),
        }

        match &set.items()[2] {
            ApplyResultItem::Skipped { error_code, .. } => {
                assert_eq!(error_code, "DUPLICATE");
            },
            other => panic!("expected skipped, got {other:?}"),
        }
    }

    #[test]
    fn rejects_duplicate_ids_and_conflicting_outcomes() {
        let duplicate = ApplyResultSet::try_from_drafts(vec![imported("row-1"), imported("row-1")]);
        assert!(duplicate.is_err(), "重复 ID 必须在查库前拒绝");

        let conflict = ApplyResultSet::try_from_drafts(vec![imported("row-1"), failed("row-1")]);
        assert!(conflict.is_err(), "同 ID 冲突 outcome 必须拒绝");
    }

    #[test]
    fn rejects_missing_and_forbidden_fields() {
        assert!(
            ApplyResultSet::try_from_drafts(vec![draft("row-1", ApplyResultOutcome::Imported)]).is_err(),
            "imported 缺失目标单据"
        );
        let imported_with_error = ApplyResultDraft { error_code: Some("X".to_string()), ..imported("row-1") };
        assert!(ApplyResultSet::try_from_drafts(vec![imported_with_error]).is_err(), "imported 禁止错误码");
        let imported_with_detail =
            ApplyResultDraft { error_detail: Some("明细".to_string()), ..imported("row-1") };
        assert!(
            ApplyResultSet::try_from_drafts(vec![imported_with_detail]).is_err(),
            "imported 禁止失败明细"
        );

        assert!(
            ApplyResultSet::try_from_drafts(vec![draft("row-1", ApplyResultOutcome::Failed)]).is_err(),
            "failed 缺失错误码"
        );
        let failed_with_target =
            ApplyResultDraft { target_document_id: Some("SO-1".to_string()), ..failed("row-1") };
        assert!(ApplyResultSet::try_from_drafts(vec![failed_with_target]).is_err(), "failed 禁止目标单据");

        assert!(
            ApplyResultSet::try_from_drafts(vec![draft("row-1", ApplyResultOutcome::Skipped)]).is_err(),
            "skipped 缺失原因码"
        );
        let skipped_with_target =
            ApplyResultDraft { target_object_reference: Some("sales/so-1".to_string()), ..skipped("row-1") };
        assert!(ApplyResultSet::try_from_drafts(vec![skipped_with_target]).is_err(), "skipped 禁止目标引用");
    }

    #[test]
    fn rejects_empty_set() {
        assert!(ApplyResultSet::try_from_drafts(Vec::new()).is_err());
    }

    #[test]
    fn accepts_max_len_and_rejects_over_limit() {
        let max = (0..super::APPLY_RESULT_MAX_LEN).map(|index| imported(&format!("row-{index}"))).collect();
        assert!(ApplyResultSet::try_from_drafts(max).is_ok());
        let over = (0..=super::APPLY_RESULT_MAX_LEN).map(|index| imported(&format!("row-{index}"))).collect();
        assert!(ApplyResultSet::try_from_drafts(over).is_err());
    }
}
