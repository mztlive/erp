//! 导入批次应用结果集（INT-E25）。
//!
//! 在查库前强制行 ID 唯一，并按 `imported` / `failed` / `skipped` 精确字段形状
//! 收紧输入。不访问数据库、时钟或 HTTP；ID 是否属于目标批次由 Service /
//! Repository 再确认。

use crate::errors::{Error, Result};
use crate::ids::{ExternalIdentityMapId, LegacyImportRowId};
use crate::validation::{normalize_optional_text, normalize_required_text};

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
    /// 返回该结果对应的导入行 ID。
    ///
    /// # 返回
    /// 返回行 ID 借用。
    pub fn row_id(&self) -> &LegacyImportRowId {
        match self {
            Self::Imported { row_id, .. } | Self::Failed { row_id, .. } | Self::Skipped { row_id, .. } => {
                row_id
            }
        }
    }

    /// 返回来源稳定身份。
    ///
    /// # 返回
    /// 待映射行可能携带身份；已映射行可为空。
    pub fn external_identity_map_id(&self) -> Option<&ExternalIdentityMapId> {
        match self {
            Self::Imported {
                external_identity_map_id,
                ..
            }
            | Self::Failed {
                external_identity_map_id,
                ..
            }
            | Self::Skipped {
                external_identity_map_id,
                ..
            } => external_identity_map_id.as_ref(),
        }
    }
}

/// 已通过唯一 ID 与 outcome 字段形状校验的应用结果集。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApplyResultSet {
    items: Vec<ApplyResultItem>,
}

impl ApplyResultSet {
    /// 从行级草稿构造结果集。
    ///
    /// 先拒绝空集、超长集合与重复行 ID（含同 ID 冲突 outcome），再按
    /// `imported` / `failed` / `skipped` 精确字段形状收紧每一行。
    ///
    /// # 参数
    /// * `drafts` - 请求中的行级结果（顺序保留）
    ///
    /// # 返回
    /// 返回已去重且字段形状合法的结果集。
    ///
    /// # 错误
    /// 数量越界、行 ID 重复、必填字段缺失或禁止字段出现时返回 [`Error::LogicError`]。
    ///
    /// # 约束
    /// 纯内存确定性校验；不查询批次归属，不访问数据库。
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

    /// 返回已校验的行结果切片。
    ///
    /// # 返回
    /// 返回按请求顺序排列的结果项。
    pub fn items(&self) -> &[ApplyResultItem] {
        &self.items
    }

    /// 返回按请求顺序排列的行 ID。
    ///
    /// # 返回
    /// 返回与结果项一一对应的行 ID 列表，供仓储按 ID 且受批次约束读取。
    pub fn row_ids(&self) -> Vec<LegacyImportRowId> {
        self.items.iter().map(|item| item.row_id().clone()).collect()
    }
}

impl ApplyResultItem {
    /// 按 outcome 收紧单行字段形状。
    ///
    /// # 参数
    /// * `draft` - 尚未按 outcome 校验的行结果
    ///
    /// # 返回
    /// 返回精确字段形状的结果项。
    ///
    /// # 错误
    /// 必填字段缺失、禁止字段出现或文本非法时返回错误。
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
        let target_document_id = present_text(target_document_id);
        let target_object_reference = present_text(target_object_reference);
        let error_code = present_text(error_code);
        let error_detail = present_text(error_detail);
        match outcome {
            ApplyResultOutcome::Imported => {
                ensure_absent(error_code.as_deref(), "导入成功结果不得携带失败错误码")?;
                ensure_absent(error_detail.as_deref(), "导入成功结果不得携带失败明细")?;
                Ok(Self::Imported {
                    row_id,
                    external_identity_map_id,
                    target_document_id: required_target(
                        target_document_id,
                        "导入成功结果必须提供目标单据 ID",
                    )?,
                    target_object_reference: optional_target(target_object_reference)?,
                })
            }
            ApplyResultOutcome::Failed => {
                ensure_absent(target_document_id.as_deref(), "失败结果不得携带目标单据 ID")?;
                ensure_absent(target_object_reference.as_deref(), "失败结果不得携带目标对象引用")?;
                Ok(Self::Failed {
                    row_id,
                    external_identity_map_id,
                    error_code: required_error_code(error_code, "失败结果必须提供错误码")?,
                    error_detail: optional_error_detail(error_detail)?,
                })
            }
            ApplyResultOutcome::Skipped => {
                ensure_absent(target_document_id.as_deref(), "跳过结果不得携带目标单据 ID")?;
                ensure_absent(target_object_reference.as_deref(), "跳过结果不得携带目标对象引用")?;
                Ok(Self::Skipped {
                    row_id,
                    external_identity_map_id,
                    error_code: required_error_code(error_code, "跳过结果必须提供原因错误码")?,
                    error_detail: optional_error_detail(error_detail)?,
                })
            }
        }
    }
}

/// 拒绝重复行 ID（含同 ID 冲突 outcome）。
///
/// # 参数
/// * `drafts` - 请求中的行级结果
///
/// # 错误
/// 任一行 ID 出现两次及以上时返回错误。
fn ensure_unique_row_ids(drafts: &[ApplyResultDraft]) -> Result<()> {
    let mut seen = std::collections::HashSet::with_capacity(drafts.len());
    for draft in drafts {
        if !seen.insert(draft.row_id.as_ref()) {
            return Err(Error::from(format!("行结果 ID 重复: {}", draft.row_id)));
        }
    }
    Ok(())
}

/// 空白视为缺省。
///
/// # 参数
/// * `value` - 可选文本
///
/// # 返回
/// 去空白后非空时返回原字符串，否则 `None`。
fn present_text(value: Option<String>) -> Option<String> {
    value.filter(|text| !text.trim().is_empty())
}

/// 禁止字段必须缺省。
///
/// # 参数
/// * `value` - 已按空白折叠的可选文本
/// * `message` - 出现时的错误文案
///
/// # 错误
/// 字段存在时返回错误。
fn ensure_absent(value: Option<&str>, message: &str) -> Result<()> {
    if value.is_some() {
        return Err(Error::from(message));
    }
    Ok(())
}

/// 规范化成功目标单据 ID。
///
/// # 参数
/// * `value` - 已按空白折叠的目标 ID
/// * `empty_message` - 缺失时的错误文案
///
/// # 错误
/// 缺失或超长时返回错误。
fn required_target(value: Option<String>, empty_message: &str) -> Result<String> {
    normalize_required_text(
        value.unwrap_or_default(),
        empty_message,
        TARGET_MAX_LEN,
        "目标单据 ID 过长",
    )
}

/// 规范化可选目标对象引用。
///
/// # 参数
/// * `value` - 已按空白折叠的引用
///
/// # 错误
/// 超长时返回错误。
fn optional_target(value: Option<String>) -> Result<Option<String>> {
    normalize_optional_text(value, "目标对象引用", TARGET_MAX_LEN)
}

/// 规范化必填错误码。
///
/// # 参数
/// * `value` - 已按空白折叠的错误码
/// * `empty_message` - 缺失时的错误文案
///
/// # 错误
/// 缺失或超长时返回错误。
fn required_error_code(value: Option<String>, empty_message: &str) -> Result<String> {
    normalize_required_text(
        value.unwrap_or_default(),
        empty_message,
        ERROR_CODE_MAX_LEN,
        "错误码过长",
    )
}

/// 规范化可选错误明细。
///
/// # 参数
/// * `value` - 已按空白折叠的明细
///
/// # 错误
/// 超长时返回错误。
fn optional_error_detail(value: Option<String>) -> Result<Option<String>> {
    normalize_optional_text(value, "错误明细", ERROR_DETAIL_MAX_LEN)
}

#[cfg(test)]
mod tests {
    use super::{ApplyResultDraft, ApplyResultItem, ApplyResultOutcome, ApplyResultSet};
    use crate::ids::{ExternalIdentityMapId, LegacyImportRowId};

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
            ApplyResultItem::Imported {
                target_document_id,
                target_object_reference,
                ..
            } => {
                assert_eq!(target_document_id, "SO-1");
                assert_eq!(target_object_reference.as_deref(), Some("sales/so-1"));
            }
            other => panic!("expected imported, got {other:?}"),
        }
        match &set.items()[1] {
            ApplyResultItem::Failed {
                error_code,
                error_detail,
                ..
            } => {
                assert_eq!(error_code, "TEMPORARY");
                assert_eq!(error_detail.as_deref(), Some("可重试"));
            }
            other => panic!("expected failed, got {other:?}"),
        }
        match &set.items()[2] {
            ApplyResultItem::Skipped { error_code, .. } => {
                assert_eq!(error_code, "DUPLICATE");
            }
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
        let imported_with_error = ApplyResultDraft {
            error_code: Some("X".to_string()),
            ..imported("row-1")
        };
        assert!(
            ApplyResultSet::try_from_drafts(vec![imported_with_error]).is_err(),
            "imported 禁止错误码"
        );
        let imported_with_detail = ApplyResultDraft {
            error_detail: Some("明细".to_string()),
            ..imported("row-1")
        };
        assert!(
            ApplyResultSet::try_from_drafts(vec![imported_with_detail]).is_err(),
            "imported 禁止失败明细"
        );

        assert!(
            ApplyResultSet::try_from_drafts(vec![draft("row-1", ApplyResultOutcome::Failed)]).is_err(),
            "failed 缺失错误码"
        );
        let failed_with_target = ApplyResultDraft {
            target_document_id: Some("SO-1".to_string()),
            ..failed("row-1")
        };
        assert!(
            ApplyResultSet::try_from_drafts(vec![failed_with_target]).is_err(),
            "failed 禁止目标单据"
        );

        assert!(
            ApplyResultSet::try_from_drafts(vec![draft("row-1", ApplyResultOutcome::Skipped)]).is_err(),
            "skipped 缺失原因码"
        );
        let skipped_with_target = ApplyResultDraft {
            target_object_reference: Some("sales/so-1".to_string()),
            ..skipped("row-1")
        };
        assert!(
            ApplyResultSet::try_from_drafts(vec![skipped_with_target]).is_err(),
            "skipped 禁止目标引用"
        );
    }

    #[test]
    fn rejects_empty_set() {
        assert!(ApplyResultSet::try_from_drafts(Vec::new()).is_err());
    }

    #[test]
    fn accepts_max_len_and_rejects_over_limit() {
        let max = (0..super::APPLY_RESULT_MAX_LEN)
            .map(|index| imported(&format!("row-{index}")))
            .collect();
        assert!(ApplyResultSet::try_from_drafts(max).is_ok());
        let over = (0..=super::APPLY_RESULT_MAX_LEN)
            .map(|index| imported(&format!("row-{index}")))
            .collect();
        assert!(ApplyResultSet::try_from_drafts(over).is_err());
    }
}
