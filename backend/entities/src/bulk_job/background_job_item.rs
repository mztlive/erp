//! `background_job_item`：后台任务逐项结果（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::errors::{Error, Result};
use crate::ids::{BackgroundJobId, BackgroundJobItemId};
use crate::validation::normalize_optional_text;

/// 对象类型代码最大长度。
const OBJECT_TYPE_MAX_LEN: usize = 64;
/// 对象 ID 最大长度。
const OBJECT_ID_MAX_LEN: usize = 128;
/// 版本标识最大长度。
const VERSION_MAX_LEN: usize = 64;
/// 内容摘要最大长度。
const HASH_MAX_LEN: usize = 128;
/// 工作表名最大长度。
const WORKSHEET_NAME_MAX_LEN: usize = 128;
/// 列名最大长度。
const COLUMN_NAME_MAX_LEN: usize = 128;
/// 结果代码最大长度。
const RESULT_CODE_MAX_LEN: usize = 64;
/// 结果摘要最大长度。
const RESULT_SUMMARY_MAX_LEN: usize = 512;

/// 逐项执行结果（数据模型 §6.1：成功、跳过、失败；`None` 表示尚未执行）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    /// 成功。
    Success,
    /// 跳过。
    Skipped,
    /// 失败。
    Failed,
}

impl ItemStatus {
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

/// 后台任务逐项创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BackgroundJobItemData {
    /// 所属后台任务。
    pub background_job_id: BackgroundJobId,
    /// 稳定逐项序号（从 1 递增，任务内唯一）。
    pub item_no: u32,
    /// 已有对象类型代码（可空）。
    pub object_type: Option<String>,
    /// 已有对象 ID（可空）。
    pub object_id: Option<String>,
    /// 执行前必须重验的预览版本。
    pub expected_version: Option<String>,
    /// 执行前必须重验的内容摘要。
    pub expected_hash: Option<String>,
    /// 导入错误定位：工作表名（适用时）。
    pub worksheet_name: Option<String>,
    /// 导入错误定位：源行号（适用时）。
    pub source_row_no: Option<u32>,
    /// 导入错误定位：源列名（适用时）。
    pub source_column_name: Option<String>,
}

/// 后台任务逐项实体（数据模型 §6.1）。
///
/// `(background_job_id, item_no)` 唯一由 P2 索引保证；任务执行逐项重验当前
/// 权限、数据范围、状态和版本（§6.1，P3 服务编排）。失败原因只保存脱敏
/// 摘要，不保存敏感原值（§4.5）。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BackgroundJobItem {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属后台任务。
    pub background_job_id: BackgroundJobId,
    /// 稳定逐项序号。
    pub item_no: u32,
    /// 已有对象类型代码。
    pub object_type: Option<String>,
    /// 已有对象 ID。
    pub object_id: Option<String>,
    /// 执行前必须重验的预览版本。
    pub expected_version: Option<String>,
    /// 执行前必须重验的内容摘要。
    pub expected_hash: Option<String>,
    /// 导入错误定位：工作表名。
    pub worksheet_name: Option<String>,
    /// 导入错误定位：源行号。
    pub source_row_no: Option<u32>,
    /// 导入错误定位：源列名。
    pub source_column_name: Option<String>,
    /// 逐项执行结果（未执行为 `None`）。
    pub status: Option<ItemStatus>,
    /// 脱敏原因代码。
    pub result_code: Option<String>,
    /// 脱敏结果摘要。
    pub result_summary: Option<String>,
    /// 成功形成的对象类型代码。
    pub result_object_type: Option<String>,
    /// 成功形成的对象 ID。
    pub result_object_id: Option<String>,
}

impl BackgroundJobItem {
    /// 创建逐项记录。
    ///
    /// 完成可选字段的校验与规范化（trim、长度上限），并强制两条关联一致性：
    /// `object_type` 与 `object_id` 必须同时提供或同时省略；`item_no` 从 1 起
    /// 递增（为 0 视为越界拒绝）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BackgroundJobItemId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的逐项记录（未执行）。
    ///
    /// # 错误
    /// 当 `item_no` 为 0、对象类型/ID 不成对，或字段超长时返回错误。
    pub fn new(id: BackgroundJobItemId, data: BackgroundJobItemData) -> Result<Self> {
        if data.item_no == 0 {
            return Err(Error::from("逐项序号必须从 1 开始"));
        }
        let object_type = normalize_optional_text(data.object_type, "对象类型", OBJECT_TYPE_MAX_LEN)?;
        let object_id = normalize_optional_text(data.object_id, "对象ID", OBJECT_ID_MAX_LEN)?;
        if object_type.is_some() != object_id.is_some() {
            return Err(Error::from("对象类型与对象ID必须同时提供或同时省略"));
        }
        let expected_version = normalize_optional_text(data.expected_version, "预期版本", VERSION_MAX_LEN)?;
        let expected_hash = normalize_optional_text(data.expected_hash, "内容摘要", HASH_MAX_LEN)?;
        if expected_version.is_some() != expected_hash.is_some() {
            return Err(Error::from("预期版本与内容摘要必须同时提供或同时省略"));
        }
        let worksheet_name =
            normalize_optional_text(data.worksheet_name, "工作表名", WORKSHEET_NAME_MAX_LEN)?;
        let source_column_name =
            normalize_optional_text(data.source_column_name, "源列名", COLUMN_NAME_MAX_LEN)?;
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            background_job_id: data.background_job_id,
            item_no: data.item_no,
            object_type,
            object_id,
            expected_version,
            expected_hash,
            worksheet_name,
            source_row_no: data.source_row_no,
            source_column_name,
            status: None,
            result_code: None,
            result_summary: None,
            result_object_type: None,
            result_object_id: None,
        })
    }

    /// 记录逐项执行结果。
    ///
    /// 结果只追加不覆盖；`Failed` 必须携带脱敏原因代码；成功时结果对象
    /// 类型与 ID 必须同时提供或同时省略。
    ///
    /// # 参数
    /// * `status` - 执行结果
    /// * `result_code` - 脱敏原因代码（`Failed` 时必填）
    /// * `result_summary` - 脱敏结果摘要
    /// * `result_object_type` - 成功形成的对象类型代码
    /// * `result_object_id` - 成功形成的对象 ID
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当该项已记录结果、`Failed` 未携带原因代码或结果对象不成对时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn record_result(
        &mut self,
        status: ItemStatus,
        result_code: Option<String>,
        result_summary: Option<String>,
        result_object_type: Option<String>,
        result_object_id: Option<String>,
    ) -> Result<()> {
        if self.status.is_some() {
            return Err(Error::from("逐项结果只能记录一次"));
        }
        let result_code = normalize_optional_text(result_code, "结果代码", RESULT_CODE_MAX_LEN)?;
        if status == ItemStatus::Failed && result_code.is_none() {
            return Err(Error::from("失败项必须记录脱敏原因代码"));
        }
        let result_summary = normalize_optional_text(result_summary, "结果摘要", RESULT_SUMMARY_MAX_LEN)?;
        let result_object_type =
            normalize_optional_text(result_object_type, "结果对象类型", OBJECT_TYPE_MAX_LEN)?;
        let result_object_id = normalize_optional_text(result_object_id, "结果对象ID", OBJECT_ID_MAX_LEN)?;
        if result_object_type.is_some() != result_object_id.is_some() {
            return Err(Error::from("结果对象类型与结果对象ID必须同时提供或同时省略"));
        }
        self.status = Some(status);
        self.result_code = result_code;
        self.result_summary = result_summary;
        self.result_object_type = result_object_type;
        self.result_object_id = result_object_id;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{BackgroundJobItem, BackgroundJobItemData, ItemStatus};
    use crate::ids::{BackgroundJobId, BackgroundJobItemId};

    fn data() -> BackgroundJobItemData {
        BackgroundJobItemData {
            background_job_id: BackgroundJobId::new("job-1"),
            item_no: 1,
            object_type: Some(" legacy_import_row ".to_string()),
            object_id: Some(" row-1 ".to_string()),
            expected_version: None,
            expected_hash: None,
            worksheet_name: Some(" Sheet1 ".to_string()),
            source_row_no: Some(2),
            source_column_name: Some("A".to_string()),
        }
    }

    /// happy path：可选字段 trim、初始未执行。
    #[test]
    fn new_trims_fields_and_starts_unexecuted() {
        let item = BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), data()).unwrap();
        assert_eq!(item.item_no, 1);
        assert_eq!(item.object_type.as_deref(), Some("legacy_import_row"));
        assert_eq!(item.object_id.as_deref(), Some("row-1"));
        assert_eq!(item.worksheet_name.as_deref(), Some("Sheet1"));
        assert_eq!(item.source_row_no, Some(2));
        assert!(item.status.is_none());
    }

    /// 失败路径：逐项序号越界（0）被拒。
    #[test]
    fn new_rejects_zero_item_no() {
        let payload = BackgroundJobItemData { item_no: 0, ..data() };
        assert!(BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), payload).is_err());
    }

    /// 失败路径：关联不一致（对象类型/ID 不成对）被拒。
    #[test]
    fn new_rejects_unpaired_object_type_and_id() {
        let payload = BackgroundJobItemData {
            object_id: None,
            ..data()
        };
        assert!(BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), payload).is_err());
    }

    /// 失败路径：超长列名被拒。
    #[test]
    fn new_rejects_overlong_column_name() {
        let payload = BackgroundJobItemData {
            source_column_name: Some("x".repeat(129)),
            ..data()
        };
        assert!(BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), payload).is_err());
    }

    /// 执行结果：失败必须带原因代码，结果对象必须成对，结果只记录一次。
    #[test]
    fn record_result_enforces_consistency_and_once_only() {
        let mut item = BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), data()).unwrap();
        assert!(item
            .record_result(ItemStatus::Failed, None, None, None, None)
            .is_err());
        assert!(
            item.record_result(
                ItemStatus::Success,
                None,
                None,
                Some("sales_order".to_string()),
                None
            )
            .is_err(),
            "结果对象必须成对"
        );

        item.record_result(
            ItemStatus::Success,
            None,
            Some(" 已创建 ".to_string()),
            Some("sales_order".to_string()),
            Some("SO-1".to_string()),
        )
        .unwrap();
        assert_eq!(item.status, Some(ItemStatus::Success));
        assert_eq!(item.result_summary.as_deref(), Some("已创建"));
        assert!(
            item.record_result(ItemStatus::Skipped, None, None, None, None)
                .is_err(),
            "结果只能记录一次"
        );
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn result_codes_and_labels_are_stable() {
        assert_eq!(serde_json::to_string(&ItemStatus::Failed).unwrap(), "\"failed\"");
        assert_eq!(ItemStatus::Success.as_str(), "success");
        assert_eq!(ItemStatus::Skipped.label(), "跳过");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let item = BackgroundJobItem::new(BackgroundJobItemId::new("ji-1"), data()).unwrap();
        let roundtrip: BackgroundJobItem = bson::from_document(bson::to_document(&item).unwrap()).unwrap();
        assert_eq!(roundtrip, item);
    }
}
