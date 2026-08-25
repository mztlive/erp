//! `bulk_selection_item`：批量选择快照的逐项冻结目标（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{BulkSelectionItemId, BulkSelectionSnapshotId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 对象类型代码最大长度。
const OBJECT_TYPE_MAX_LEN: usize = 64;
/// 对象 ID 最大长度。
const OBJECT_ID_MAX_LEN: usize = 128;
/// 预览版本标识最大长度。
const VERSION_MAX_LEN: usize = 64;
/// 内容摘要最大长度。
const HASH_MAX_LEN: usize = 128;
/// 结果代码最大长度。
const RESULT_CODE_MAX_LEN: usize = 64;

/// 逐项执行结果（数据模型 §6.1：成功、跳过、失败；`None` 表示尚未执行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionItemStatus {
    /// 成功。
    Success,
    /// 跳过。
    Skipped,
    /// 失败。
    Failed,
}

impl SelectionItemStatus {
    /// 返回结果状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Success => "成功",
            Self::Skipped => "跳过",
            Self::Failed => "失败",
        }
    }

    /// 返回结果状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Success => "success",
            Self::Skipped => "skipped",
            Self::Failed => "failed",
        }
    }
}

/// 选择项创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionItemData {
    /// 所属选择快照。
    pub selection_snapshot_id: BulkSelectionSnapshotId,
    /// 目标对象类型代码（跨域开放目录）。
    pub object_type: String,
    /// 目标对象 ID。
    pub object_id: String,
    /// 预览时版本（执行前重验）。
    pub expected_version: Option<String>,
    /// 预览时内容摘要（执行前重验）。
    pub expected_hash: Option<String>,
}

/// 批量选择项实体（数据模型 §6.1）。
///
/// 预览时冻结目标；快照确认后目标集合与预期版本不可修改（§6.1，由 P3 在
/// 确认前完成冻结）；`(selection_snapshot_id, object_type, object_id)` 唯一
/// 由 P2 索引保证。执行逐项重验当前权限、数据范围、状态和版本（§6.1，
/// P3 服务编排）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BulkSelectionItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属选择快照。
    pub selection_snapshot_id: BulkSelectionSnapshotId,
    /// 目标对象类型代码。
    pub object_type: String,
    /// 目标对象 ID。
    pub object_id: String,
    /// 预览时版本。
    pub expected_version: Option<String>,
    /// 预览时内容摘要。
    pub expected_hash: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub result_status: Option<SelectionItemStatus>,
    /// 失败原因代码（适用时）。
    pub result_code: Option<String>,
}

impl BulkSelectionItem {
    /// 创建选择项。
    ///
    /// 完成 object_type/object_id 的校验与规范化（trim、非空、长度上限），
    /// `expected_version` / `expected_hash` 可选但必须**同时提供或同时省略**
    /// （预览版本与内容摘要成对出现）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BulkSelectionItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的选择项实体（未执行）。
    ///
    /// # 错误
    /// 当对象类型/ID 为空/超长，或预期版本与摘要不成对时返回错误。
    pub fn new(id: BulkSelectionItemId, data: BulkSelectionItemData) -> Result<Self> {
        let object_type = normalize_required_text(
            data.object_type,
            "对象类型不能为空",
            OBJECT_TYPE_MAX_LEN,
            "对象类型过长",
        )?;
        let object_id =
            normalize_required_text(data.object_id, "对象ID不能为空", OBJECT_ID_MAX_LEN, "对象ID过长")?;
        let expected_version = normalize_optional_text(data.expected_version, "预期版本", VERSION_MAX_LEN)?;
        let expected_hash = normalize_optional_text(data.expected_hash, "内容摘要", HASH_MAX_LEN)?;
        if expected_version.is_some() != expected_hash.is_some() {
            return Err(Error::from("预期版本与内容摘要必须同时提供或同时省略"));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            selection_snapshot_id: data.selection_snapshot_id,
            object_type,
            object_id,
            expected_version,
            expected_hash,
            result_status: None,
            result_code: None,
        })
    }

    /// 记录逐项执行结果。
    ///
    /// 结果只追加不覆盖：`None → Some(status)`；`result_code` 与 `result_status`
    /// 关联一致性（失败必须携带原因代码）在本方法校验。
    ///
    /// # 参数
    /// * `status` - 执行结果
    /// * `result_code` - 原因代码（`Failed` 时必填）
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当该项已记录结果，或 `Failed` 未携带原因代码时返回错误。
    pub fn record_result(&mut self, status: SelectionItemStatus, result_code: Option<String>) -> Result<()> {
        if self.result_status.is_some() {
            return Err(Error::from("选择项结果只能记录一次"));
        }
        let result_code = normalize_optional_text(result_code, "结果代码", RESULT_CODE_MAX_LEN)?;
        if status == SelectionItemStatus::Failed && result_code.is_none() {
            return Err(Error::from("失败项必须记录原因代码"));
        }
        self.result_status = Some(status);
        self.result_code = result_code;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{BulkSelectionItem, BulkSelectionItemData, SelectionItemStatus};
    use crate::ids::{BulkSelectionItemId, BulkSelectionSnapshotId};

    fn data() -> BulkSelectionItemData {
        BulkSelectionItemData {
            selection_snapshot_id: BulkSelectionSnapshotId::new("snap-1"),
            object_type: " sales_order ".to_string(),
            object_id: " SO-1 ".to_string(),
            expected_version: Some(" v3 ".to_string()),
            expected_hash: Some("ab12cd34".to_string()),
        }
    }

    /// happy path：对象字段 trim，初始未执行。
    #[test]
    fn new_trims_fields_and_starts_unexecuted() {
        let item = BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), data()).unwrap();
        assert_eq!(item.object_type, "sales_order");
        assert_eq!(item.object_id, "SO-1");
        assert_eq!(item.expected_version.as_deref(), Some("v3"));
        assert!(item.result_status.is_none());
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_object_type() {
        let payload = BulkSelectionItemData {
            object_type: "  ".to_string(),
            ..data()
        };
        assert!(BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), payload).is_err());
    }

    /// 失败路径：关联不一致（版本与摘要不成对）被拒。
    #[test]
    fn new_rejects_unpaired_version_and_hash() {
        let payload = BulkSelectionItemData {
            expected_hash: None,
            ..data()
        };
        assert!(BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), payload).is_err());
    }

    /// 失败路径：超长对象 ID 被拒。
    #[test]
    fn new_rejects_overlong_object_id() {
        let payload = BulkSelectionItemData {
            object_id: "x".repeat(129),
            ..data()
        };
        assert!(BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), payload).is_err());
    }

    /// 执行结果：成功/跳过可无原因，失败必须带原因代码，结果只记录一次。
    #[test]
    fn record_result_enforces_failure_reason_and_once_only() {
        let mut item = BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), data()).unwrap();
        item.record_result(SelectionItemStatus::Success, None).unwrap();
        assert!(item.record_result(SelectionItemStatus::Skipped, None).is_err());

        let mut failed = BulkSelectionItem::new(BulkSelectionItemId::new("si-2"), data()).unwrap();
        assert!(failed.record_result(SelectionItemStatus::Failed, None).is_err());
        failed
            .record_result(
                SelectionItemStatus::Failed,
                Some(" VERSION_MISMATCH ".to_string()),
            )
            .unwrap();
        assert_eq!(failed.result_code.as_deref(), Some("VERSION_MISMATCH"));
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn result_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&SelectionItemStatus::Skipped).unwrap(),
            "\"skipped\""
        );
        assert_eq!(SelectionItemStatus::Failed.as_str(), "failed");
        assert_eq!(SelectionItemStatus::Success.label(), "成功");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let item = BulkSelectionItem::new(BulkSelectionItemId::new("si-1"), data()).unwrap();
        let roundtrip: BulkSelectionItem =
            bson::deserialize_from_document(bson::serialize_to_document(&item).unwrap()).unwrap();
        assert_eq!(roundtrip, item);
    }
}
