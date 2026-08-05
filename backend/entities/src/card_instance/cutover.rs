//! `mall_consumption_cutover`：商城消费回流与自动履约上线切换（数据模型 §6.17）。
//!
//! 每个商城只能有一个已启用 `T`；`enabled_at` 一经启用不可修改或删除（§6.17）。
//! 启用动作对应事务不变量 §8.4 第 8 条（校验一期轮询封存与 checklist 核对记录后
//! 原子写唯一 `T`），状态机为固定的「准备 → 已启用」，已启用为不可逆终态。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::MallConsumptionCutoverId;
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 目标商城代码最大长度。
const MALL_ID_MAX_LEN: usize = 64;
/// 上线核对文档引用最大长度。
const CHECKLIST_REFERENCE_MAX_LEN: usize = 512;

/// 切换状态（数据模型 §6.17：准备、已启用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CutoverStatus {
    /// 准备：正在执行上线核对文档 checklist。
    Preparing,
    /// 已启用：唯一 `T` 已登记，`enabled_at` 不可修改。
    Enabled,
}

impl CutoverStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Preparing => "准备",
            Self::Enabled => "已启用",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Preparing => "preparing",
            Self::Enabled => "enabled",
        }
    }

    /// 判断是否已启用。
    ///
    /// # 返回
    /// 状态为 `Enabled` 时返回 `true`。
    pub fn is_enabled(&self) -> bool {
        matches!(self, Self::Enabled)
    }
}

impl DocumentState for CutoverStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Preparing => &[Self::Enabled],
            Self::Enabled => &[],
        }
    }
}

/// 切换记录创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionCutoverData {
    /// 目标商城（`source_system.code`）。
    pub mall_id: String,
    /// 上线核对文档引用（`document_attachment` 或外部链接），可空。
    pub checklist_reference: Option<String>,
}

/// 切换记录实体（数据模型 §6.17）。
///
/// 创建时状态为 `Preparing`，`enabled_at`/`enabled_by` 为空；启用后进入不可逆
/// `Enabled` 终态，`enabled_at` 不得再修改。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct MallConsumptionCutover {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 目标商城（`source_system.code`）。
    pub mall_id: String,
    /// 消费回流和自动履约启用时间 `T`；启用前为空，启用后不可修改。
    pub enabled_at: Option<Instant>,
    /// 上线负责人。
    pub enabled_by: Option<String>,
    /// 切换状态。
    pub status: CutoverStatus,
    /// 上线核对文档引用。
    pub checklist_reference: Option<String>,
}

impl MallConsumptionCutover {
    /// 创建切换记录。
    ///
    /// 完成 mall_id 的完整校验与规范化；状态固定为 `Preparing`，
    /// `enabled_at`/`enabled_by` 为空，等待 [`MallConsumptionCutover::enable`]。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MallConsumptionCutoverId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的切换记录实体。
    ///
    /// # 错误
    /// 当 mall_id 为空/超长时返回错误。
    pub fn new(id: MallConsumptionCutoverId, data: MallConsumptionCutoverData) -> Result<Self> {
        let mall_id = normalize_required_text(
            data.mall_id,
            "目标商城不能为空",
            MALL_ID_MAX_LEN,
            "目标商城代码过长",
        )?;
        let checklist_reference = normalize_optional_text(
            data.checklist_reference,
            "上线核对文档引用",
            CHECKLIST_REFERENCE_MAX_LEN,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            mall_id,
            enabled_at: None,
            enabled_by: None,
            status: CutoverStatus::Preparing,
            checklist_reference,
        })
    }

    /// 启用切换（登记唯一 `T`）。
    ///
    /// 对应事务不变量 §8.4 第 8 条：仅当处于 `Preparing` 时允许启用；启用后状态进入
    /// 不可逆 `Enabled`，`enabled_at`/`enabled_by` 一经写入不得修改或删除。
    ///
    /// # 参数
    /// * `enabled_at` - 启用时间 `T`
    /// * `enabled_by` - 上线负责人
    ///
    /// # 返回
    /// 启用成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 状态不是 `Preparing`（如已启用）时返回 `InvalidStateTransition`。
    pub fn enable(&mut self, enabled_at: Instant, enabled_by: impl Into<String>) -> Result<()> {
        if self.status.is_enabled() {
            return Err(Error::from("切换已启用，T 不得重复登记"));
        }
        ensure_transition(self.status, CutoverStatus::Enabled)?;
        self.status = CutoverStatus::Enabled;
        self.enabled_at = Some(enabled_at);
        self.enabled_by = Some(enabled_by.into());
        Ok(())
    }

    /// 更新上线核对文档引用。
    ///
    /// # 参数
    /// * `checklist_reference` - 核对文档引用；`None` 或空字符串表示清除
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 切换已启用（`Enabled` 终态）或引用超长时返回错误。
    pub fn set_checklist_reference(&mut self, checklist_reference: Option<String>) -> Result<()> {
        if self.status.is_enabled() {
            return Err(Error::from("切换已启用，上线核对文档引用不可再修改"));
        }
        self.checklist_reference = normalize_optional_text(
            checklist_reference,
            "上线核对文档引用",
            CHECKLIST_REFERENCE_MAX_LEN,
        )?;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{CutoverStatus, MallConsumptionCutover, MallConsumptionCutoverData};
    use crate::common::state::{ensure_transition, DocumentState};
    use crate::common::time::Instant;
    use crate::ids::MallConsumptionCutoverId;

    fn data() -> MallConsumptionCutoverData {
        MallConsumptionCutoverData {
            mall_id: " mall-a ".to_string(),
            checklist_reference: Some(" attachment-1 ".to_string()),
        }
    }

    /// happy path：创建后为准备状态，字段被规范化；启用后 T 与负责人落库且不可再改。
    #[test]
    fn new_trims_fields_and_enable_immutably_writes_t() {
        let mut cutover =
            MallConsumptionCutover::new(MallConsumptionCutoverId::new("cutover-1"), data()).unwrap();

        assert_eq!(cutover.mall_id, "mall-a");
        assert_eq!(cutover.checklist_reference.as_deref(), Some("attachment-1"));
        assert_eq!(cutover.status, CutoverStatus::Preparing);
        assert!(cutover.enabled_at.is_none());
        assert!(cutover.enabled_by.is_none());

        let t = Instant::from_unix_secs(1_700_000_000);
        cutover.enable(t, "owner-1").unwrap();
        assert!(cutover.status.is_enabled());
        assert_eq!(cutover.enabled_at, Some(t));
        assert_eq!(cutover.enabled_by.as_deref(), Some("owner-1"));

        let error = cutover.enable(t, "owner-2").unwrap_err();
        assert!(
            error.to_string().contains("T 不得重复登记"),
            "已启用后 T 不可重复登记"
        );
    }

    /// 失败路径：mall_id 为空/超长均被拒绝；引用超长被拒绝。
    #[test]
    fn new_rejects_empty_and_overlong_text_fields() {
        let empty = MallConsumptionCutoverData {
            mall_id: "   ".to_string(),
            ..data()
        };
        assert!(MallConsumptionCutover::new(MallConsumptionCutoverId::new("c2"), empty).is_err());

        let overlong = MallConsumptionCutoverData {
            mall_id: "x".repeat(65),
            ..data()
        };
        assert!(MallConsumptionCutover::new(MallConsumptionCutoverId::new("c3"), overlong).is_err());

        let overlong_reference = MallConsumptionCutoverData {
            checklist_reference: Some("r".repeat(513)),
            ..data()
        };
        assert!(
            MallConsumptionCutover::new(MallConsumptionCutoverId::new("c4"), overlong_reference).is_err()
        );
    }

    /// 状态机：准备 → 已启用合法且定向；已启用为终态（无后继）；幂等合法。
    #[test]
    fn status_machine_directed_edges() {
        assert!(ensure_transition(CutoverStatus::Preparing, CutoverStatus::Enabled).is_ok());
        assert!(ensure_transition(CutoverStatus::Preparing, CutoverStatus::Preparing).is_ok());
        assert!(ensure_transition(CutoverStatus::Enabled, CutoverStatus::Enabled).is_ok());

        assert!(
            ensure_transition(CutoverStatus::Enabled, CutoverStatus::Preparing).is_err(),
            "已启用是终态，禁止回退"
        );
        assert_eq!(CutoverStatus::Enabled.allowed_next(), &[] as &[CutoverStatus]);
        assert_eq!(CutoverStatus::Preparing.allowed_next(), &[CutoverStatus::Enabled]);
    }

    /// 更新核对引用：准备期可更新并规范化；启用后拒绝修改。
    #[test]
    fn checklist_reference_updates_before_enable_only() {
        let mut cutover =
            MallConsumptionCutover::new(MallConsumptionCutoverId::new("cutover-5"), data()).unwrap();

        cutover
            .set_checklist_reference(Some("  doc-b  ".to_string()))
            .unwrap();
        assert_eq!(cutover.checklist_reference.as_deref(), Some("doc-b"));

        cutover
            .enable(Instant::from_unix_secs(1_700_000_000), "owner-1")
            .unwrap();
        assert!(cutover
            .set_checklist_reference(Some("doc-c".to_string()))
            .is_err());
        assert!(cutover.set_checklist_reference(None).is_err());
    }
}
