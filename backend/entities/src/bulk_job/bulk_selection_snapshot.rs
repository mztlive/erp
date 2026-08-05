//! `bulk_selection_snapshot`：批量预览时冻结目标、截止水位和逐项版本（数据模型 §6.1）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::BulkSelectionSnapshotId;
use crate::validation::normalize_required_text;

/// 创建人标识最大长度。
const CREATED_BY_MAX_LEN: usize = 128;
/// 单次冻结目标数上限（防越界滥用；§6.1 逐项执行重验）。
const MAX_ITEM_COUNT: u32 = 100_000;

/// 选择类型（数据模型 §6.1：导出、责任人分配、导入应用、映射、补拉等；
/// 固定枚举，其余类型属二期扩展的地基修订候选）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum SelectionType {
    /// 导出。
    Export,
    /// 责任人分配。
    OwnershipAssignment,
    /// 导入应用。
    ImportApply,
    /// 映射。
    Mapping,
    /// 补拉。
    RePull,
}

impl SelectionType {
    /// 返回选择类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Export => "导出",
            Self::OwnershipAssignment => "责任人分配",
            Self::ImportApply => "导入应用",
            Self::Mapping => "映射",
            Self::RePull => "补拉",
        }
    }

    /// 返回选择类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Export => "export",
            Self::OwnershipAssignment => "ownership_assignment",
            Self::ImportApply => "import_apply",
            Self::Mapping => "mapping",
            Self::RePull => "re_pull",
        }
    }
}

/// 快照状态（数据模型 §6.1：待确认、已确认、执行中、完成、失效）。
///
/// 固定状态机（无运行时扩展）：
/// `PENDING → CONFIRMED → EXECUTING → COMPLETED`；`PENDING` / `CONFIRMED`
/// 可失效（`EXPIRED`）。`COMPLETED` / `EXPIRED` 是终态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelectionStatus {
    /// 待确认。
    #[default]
    Pending,
    /// 已确认。
    Confirmed,
    /// 执行中。
    Executing,
    /// 完成。
    Completed,
    /// 失效。
    Expired,
}

impl SelectionStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待确认",
            Self::Confirmed => "已确认",
            Self::Executing => "执行中",
            Self::Completed => "完成",
            Self::Expired => "失效",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Confirmed => "confirmed",
            Self::Executing => "executing",
            Self::Completed => "completed",
            Self::Expired => "expired",
        }
    }
}

impl DocumentState for SelectionStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Confirmed, Self::Expired],
            Self::Confirmed => &[Self::Executing, Self::Expired],
            Self::Executing => &[Self::Completed],
            Self::Completed | Self::Expired => &[],
        }
    }
}

/// 选择快照创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BulkSelectionSnapshotData {
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 选择范围的数据截止水位。
    pub data_cutoff_at: Instant,
    /// 冻结目标数。
    pub item_count: u32,
    /// 创建人。
    pub created_by: String,
    /// 有效期截止时间。
    pub expires_at: Instant,
}

/// 批量选择快照实体（数据模型 §6.1）。
///
/// 快照确认后目标集合、截止水位和预期版本不可修改（§6.1）；实体层不提供
/// 目标字段的变更方法，`confirm` 之后的任何状态迁移都不改写冻结内容。
/// `(selection_snapshot_id, object_type, object_id)` 唯一由 P2 索引保证。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct BulkSelectionSnapshot {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 选择类型。
    pub selection_type: SelectionType,
    /// 选择范围的数据截止水位。
    pub data_cutoff_at: Instant,
    /// 冻结目标数。
    pub item_count: u32,
    /// 创建人。
    pub created_by: String,
    /// 有效期截止时间。
    pub expires_at: Instant,
    /// 快照状态。
    pub status: SelectionStatus,
}

impl BulkSelectionSnapshot {
    /// 创建选择快照。
    ///
    /// 完成 created_by 的校验与规范化，并强制 `item_count` 不超过冻结目标数
    /// 上限（`0 < item_count <= MAX_ITEM_COUNT`）。目标集合与逐项版本由
    /// `bulk_selection_item` 保存；快照 `item_count` 与实际行数的核对是跨表
    /// 不变量，由 P3 事务保证（§6.1）。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::BulkSelectionSnapshotId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的快照实体（状态 `PENDING`）。
    ///
    /// # 错误
    /// 当创建人为空/超长、`item_count` 为零或超过上限时返回错误。
    pub fn new(id: BulkSelectionSnapshotId, data: BulkSelectionSnapshotData) -> Result<Self> {
        let created_by = normalize_required_text(
            data.created_by,
            "创建人不能为空",
            CREATED_BY_MAX_LEN,
            "创建人过长",
        )?;
        if data.item_count == 0 {
            return Err(Error::from("冻结目标数必须大于零"));
        }
        if data.item_count > MAX_ITEM_COUNT {
            return Err(Error::from(format!("冻结目标数不能超过 {MAX_ITEM_COUNT}")));
        }
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            selection_type: data.selection_type,
            data_cutoff_at: data.data_cutoff_at,
            item_count: data.item_count,
            created_by,
            expires_at: data.expires_at,
            status: SelectionStatus::Pending,
        })
    }

    /// 确认快照。
    ///
    /// 快照确认后目标集合、截止水位和预期版本不可修改（§6.1）。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当快照不是 `PENDING` 状态（如已确认、已失效）时返回错误。
    pub fn confirm(&mut self) -> Result<()> {
        ensure_transition(self.status, SelectionStatus::Confirmed)?;
        self.status = SelectionStatus::Confirmed;
        Ok(())
    }

    /// 开始执行。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当快照不是 `CONFIRMED` 状态时返回错误。
    pub fn start_execution(&mut self) -> Result<()> {
        ensure_transition(self.status, SelectionStatus::Executing)?;
        self.status = SelectionStatus::Executing;
        Ok(())
    }

    /// 标记完成。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当快照不是 `EXECUTING` 状态时返回错误。
    pub fn complete(&mut self) -> Result<()> {
        ensure_transition(self.status, SelectionStatus::Completed)?;
        self.status = SelectionStatus::Completed;
        Ok(())
    }

    /// 标记失效。
    ///
    /// 仅 `PENDING` / `CONFIRMED` 可失效；执行中的快照只能走向完成。
    ///
    /// # 返回
    /// 无返回值。
    ///
    /// # 错误
    /// 当快照不是 `PENDING` / `CONFIRMED` 状态时返回错误。
    pub fn expire(&mut self) -> Result<()> {
        ensure_transition(self.status, SelectionStatus::Expired)?;
        self.status = SelectionStatus::Expired;
        Ok(())
    }

    /// 判断快照是否仍在有效期内。
    ///
    /// # 参数
    /// * `now` - 当前时刻
    ///
    /// # 返回
    /// 未到 `expires_at` 且状态不是终态时返回 `true`。
    pub fn is_valid_at(&self, now: Instant) -> bool {
        !self.is_terminal() && now <= self.expires_at
    }

    /// 判断快照是否已处于终态。
    ///
    /// # 返回
    /// `COMPLETED` / `EXPIRED` 时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        matches!(self.status, SelectionStatus::Completed | SelectionStatus::Expired)
    }
}

#[cfg(test)]
mod tests {
    use super::{BulkSelectionSnapshot, BulkSelectionSnapshotData, SelectionStatus, SelectionType};
    use crate::common::state::ensure_transition;
    use crate::common::time::Instant;
    use crate::ids::BulkSelectionSnapshotId;

    fn data() -> BulkSelectionSnapshotData {
        BulkSelectionSnapshotData {
            selection_type: SelectionType::Export,
            data_cutoff_at: Instant::from_unix_secs(1_700_000_000),
            item_count: 3,
            created_by: " admin-1 ".to_string(),
            expires_at: Instant::from_unix_secs(1_700_604_800),
        }
    }

    /// happy path：创建人 trim、初始 PENDING。
    #[test]
    fn new_trims_creator_and_starts_pending() {
        let snapshot = BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), data()).unwrap();
        assert_eq!(snapshot.created_by, "admin-1");
        assert_eq!(snapshot.item_count, 3);
        assert_eq!(snapshot.status, SelectionStatus::Pending);
        assert!(snapshot.is_valid_at(Instant::from_unix_secs(1_700_000_000)));
        assert!(!snapshot.is_terminal());
    }

    /// 失败路径：必填为空被拒。
    #[test]
    fn new_rejects_empty_creator() {
        let payload = BulkSelectionSnapshotData {
            created_by: "  ".to_string(),
            ..data()
        };
        assert!(BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), payload).is_err());
    }

    /// 失败路径：目标数为零与超上限被拒（列表越界）。
    #[test]
    fn new_rejects_out_of_range_item_count() {
        let zero = BulkSelectionSnapshotData {
            item_count: 0,
            ..data()
        };
        assert!(BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), zero).is_err());

        let over = BulkSelectionSnapshotData {
            item_count: super::MAX_ITEM_COUNT + 1,
            ..data()
        };
        assert!(BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), over).is_err());
    }

    /// 状态机：合法迁移全链路。
    #[test]
    fn lifecycle_confirm_execute_complete() {
        let mut snapshot =
            BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), data()).unwrap();
        snapshot.confirm().unwrap();
        assert_eq!(snapshot.status, SelectionStatus::Confirmed);
        snapshot.start_execution().unwrap();
        assert_eq!(snapshot.status, SelectionStatus::Executing);
        snapshot.complete().unwrap();
        assert_eq!(snapshot.status, SelectionStatus::Completed);
        assert!(snapshot.is_terminal());
    }

    /// 状态机：非法迁移被拒（跳步、终态迁移、执行中失效）。
    #[test]
    fn illegal_transitions_are_rejected() {
        let mut snapshot =
            BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), data()).unwrap();
        assert!(snapshot.start_execution().is_err(), "未确认不能执行");
        assert!(snapshot.complete().is_err(), "未执行不能完成");

        snapshot.confirm().unwrap();
        snapshot.expire().unwrap();
        assert_eq!(snapshot.status, SelectionStatus::Expired);
        assert!(!snapshot.is_valid_at(Instant::from_unix_secs(1_700_000_000)));
        assert!(snapshot.confirm().is_err(), "终态不可回退");

        let mut executing =
            BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-2"), data()).unwrap();
        executing.confirm().unwrap();
        executing.start_execution().unwrap();
        assert!(executing.expire().is_err(), "执行中只能走向完成");
    }

    /// 状态机：逐边定向断言（含不可逆终态）。
    #[test]
    fn directed_edge_assertions() {
        for &(from, to) in &[
            (SelectionStatus::Pending, SelectionStatus::Confirmed),
            (SelectionStatus::Pending, SelectionStatus::Expired),
            (SelectionStatus::Confirmed, SelectionStatus::Executing),
            (SelectionStatus::Confirmed, SelectionStatus::Expired),
            (SelectionStatus::Executing, SelectionStatus::Completed),
        ] {
            assert!(ensure_transition(from, to).is_ok(), "{from:?} → {to:?}");
        }
        assert!(ensure_transition(SelectionStatus::Executing, SelectionStatus::Expired).is_err());
        assert!(ensure_transition(SelectionStatus::Completed, SelectionStatus::Executing).is_err());
        assert!(ensure_transition(SelectionStatus::Expired, SelectionStatus::Pending).is_err());
    }

    /// 枚举序列化与标签稳定。
    #[test]
    fn selection_codes_and_labels_are_stable() {
        assert_eq!(
            serde_json::to_string(&SelectionType::OwnershipAssignment).unwrap(),
            "\"ownership_assignment\""
        );
        assert_eq!(SelectionType::RePull.as_str(), "re_pull");
        assert_eq!(SelectionType::Export.label(), "导出");
        assert_eq!(SelectionStatus::Executing.as_str(), "executing");
        assert_eq!(SelectionStatus::Expired.label(), "失效");
    }

    /// BSON 往返。
    #[test]
    fn entity_roundtrips_through_bson() {
        let snapshot = BulkSelectionSnapshot::new(BulkSelectionSnapshotId::new("snap-1"), data()).unwrap();
        let roundtrip: BulkSelectionSnapshot =
            bson::from_document(bson::to_document(&snapshot).unwrap()).unwrap();
        assert_eq!(roundtrip, snapshot);
    }
}
