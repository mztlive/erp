//! 审批流程定义。只保存流程种类，不保存 ERP 单据类型。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::ApprovalProcessDefinitionId;
use crate::model::types::{
    ApprovalDefinitionStatus, ModelError, ModelResult, NAME_MAX_LEN, NODE_KEY_MAX_LEN, base_model_at,
    normalize_required, touch_base,
};
use crate::model::{ParticipantId, ProcessKind, Timestamp};

/// 审批流程定义版本。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalProcessDefinition {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 同一流程种类内从 1 单调递增的业务版本。
    pub definition_version: u32,
    /// 管理与审计名称。
    pub name: String,
    /// 定义状态。
    pub status: ApprovalDefinitionStatus,
    /// 入口节点键。
    pub entry_node_key: String,
    /// 草稿创建人。
    pub created_by: ParticipantId,
    /// 发布人。
    pub published_by: Option<ParticipantId>,
    /// 发布时间。
    pub published_at: Option<Timestamp>,
    /// 退役人。
    pub retired_by: Option<ParticipantId>,
    /// 退役时间。
    pub retired_at: Option<Timestamp>,
}

impl ApprovalProcessDefinition {
    /// 新建或重命名草稿前裁剪定义名，使命令摘要和实体存储使用相同名称；空白或超过 UTF-8 字节上限时返回错误。
    pub fn normalize_name(name: impl Into<String>) -> ModelResult<String> {
        normalize_required(name, "定义名称不能为空", NAME_MAX_LEN, "定义名称过长")
    }

    /// 新建流程版本时生成可编辑的定义实体，保存调用方指定的种类、业务版本、入口键、创建人和时间。
    /// 业务版本从 1 起；名称和入口键必须是有效文本，入口是否实际命中节点由构图阶段检查。
    /// 初始没有发布或退役审计记录，不在这里生成 ID 或读取时钟。
    pub fn new_draft(
        id: ApprovalProcessDefinitionId,
        process_kind: ProcessKind,
        definition_version: u32,
        name: impl Into<String>,
        entry_node_key: impl Into<String>,
        created_by: ParticipantId,
        at: Timestamp,
    ) -> ModelResult<Self> {
        if definition_version == 0 {
            return Err(ModelError::InvalidField("定义版本必须从 1 开始"));
        }

        Ok(Self {
            base: base_model_at(id.to_string(), at)?,
            process_kind,
            definition_version,
            name: Self::normalize_name(name)?,
            status: ApprovalDefinitionStatus::Draft,
            entry_node_key: normalize_required(
                entry_node_key,
                "入口节点键不能为空",
                NODE_KEY_MAX_LEN,
                "入口节点键过长",
            )?,
            created_by,
            published_by: None,
            published_at: None,
            retired_by: None,
            retired_at: None,
        })
    }

    /// 提交草稿编辑或状态流转时读取用于 CAS 的乐观锁版本；它不是审批实例冻结的定义业务版本。
    pub fn definition_lock_version(&self) -> u64 {
        self.base.version
    }

    /// 接受带预期版本的定义命令前识别已过期输入；版本不符返回状态错误，实际写入仍需调用方做 CAS。
    pub fn ensure_lock_version(&self, expected: u64) -> ModelResult<()> {
        if self.definition_lock_version() == expected {
            return Ok(());
        }
        Err(ModelError::InvalidStatus("定义锁版本已过期"))
    }

    /// 保存新草稿前，从同流程种类的历史最大业务版本递增（无历史传 `0`），不复用旧版本或溢出回绕。
    pub fn next_version_after(current_max: u32) -> ModelResult<u32> {
        current_max.checked_add(1).ok_or(ModelError::Overflow("定义版本"))
    }

    /// 改名、替换节点或重设入口之前确认仍是草稿，避免改写已冻结定义。
    /// 已发布或已退役时返回 [`ModelError::InvalidStatus`]。
    pub fn ensure_mutable(&self) -> ModelResult<()> {
        if self.status.is_draft() {
            return Ok(());
        }
        Err(ModelError::InvalidStatus("只有草稿定义可以修改"))
    }

    /// 绑定流程定义前复核它是否仍已发布；这里只查生命周期状态，绑定入口还须校验完整图。
    pub fn is_published(&self) -> bool {
        self.status.is_published()
    }

    /// 在定义仍可编辑时更新规范化名称，并推进锁版本和更新时间；已发布或退役定义不能重命名。
    pub fn rename_draft(&mut self, name: impl Into<String>, at: Timestamp) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.name = Self::normalize_name(name)?;
        touch_base(&mut self.base, at)
    }

    /// 重建草稿图时写入首位节点的规范键并更新锁版本；键对应的节点及位置由整图校验，不在实体里查询。
    pub fn set_entry_node_draft(
        &mut self,
        entry_node_key: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.entry_node_key =
            normalize_required(entry_node_key, "入口节点键不能为空", NODE_KEY_MAX_LEN, "入口节点键过长")?;
        touch_base(&mut self.base, at)
    }

    /// 上层完成发布所需的图校验后，把草稿切为已发布，记录发布人、时间并推进乐观锁。
    /// 非草稿、时间不可持久化或版本溢出时返回错误；本方法不保存数据或退役旧版本。
    pub fn publish(&mut self, actor: ParticipantId, at: Timestamp) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.status = ApprovalDefinitionStatus::Published;
        self.published_by = Some(actor);
        self.published_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 发布新草稿时先退役调用方提供的同种类现行版本（首次发布可为空），再以同一人和时间发布新版本。
    /// 返回新发布版和可选退役版，供调用方在同一事务内保存；状态、时间或版本非法时不返回迁移结果。
    pub fn publish_replacing(
        mut self,
        previous: Option<Self>,
        actor: ParticipantId,
        at: Timestamp,
    ) -> ModelResult<(Self, Option<Self>)> {
        let previous = match previous {
            Some(mut previous) => {
                previous.retire(actor.clone(), at)?;
                Some(previous)
            },
            None => None,
        };

        self.publish(actor, at)?;
        Ok((self, previous))
    }

    /// 停用当前发布版时将其设为退役，记录退役人、时间和新锁版本，使它不再可绑定。
    /// 只有已发布版本可退役；时间不可持久化或版本溢出时返回错误。
    pub fn retire(&mut self, actor: ParticipantId, at: Timestamp) -> ModelResult<()> {
        if self.status != ApprovalDefinitionStatus::Published {
            return Err(ModelError::InvalidStatus("只有已发布定义可以退役"));
        }

        self.status = ApprovalDefinitionStatus::Retired;
        self.retired_by = Some(actor);
        self.retired_at = Some(at);
        touch_base(&mut self.base, at)
    }
}

#[cfg(test)]
mod tests {
    use super::ApprovalProcessDefinition;
    use crate::ids::ApprovalProcessDefinitionId;
    use crate::model::types::{ApprovalDefinitionStatus, ModelError};
    use crate::model::{ParticipantId, ProcessKind, Timestamp};

    fn draft() -> ApprovalProcessDefinition {
        ApprovalProcessDefinition::new_draft(
            ApprovalProcessDefinitionId::new("def-1"),
            ProcessKind::StockAdjustment,
            1,
            "库存调整",
            "n1",
            ParticipantId::new("admin").unwrap(),
            Timestamp::from_unix_secs(1_700_000_000).unwrap(),
        )
        .unwrap()
    }

    // 验证新草稿能继续编辑，且业务版本不会被当作并发锁版本使用。
    #[test]
    fn new_draft_starts_mutable() {
        let definition = draft();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Draft);
        assert_eq!(definition.definition_version, 1);
        assert_eq!(definition.definition_lock_version(), 1);
        assert_eq!(definition.process_kind, ProcessKind::StockAdjustment);
        assert!(definition.ensure_mutable().is_ok());
    }

    // 覆盖名称裁剪、后继业务版本计算和过期锁拒绝，确保定义命令按统一规则准备。
    #[test]
    fn definition_helpers_are_deterministic_and_fail_closed() {
        assert_eq!(ApprovalProcessDefinition::normalize_name(" 库存调整 ").unwrap(), "库存调整");
        assert!(ApprovalProcessDefinition::normalize_name("   ").is_err());

        assert_eq!(ApprovalProcessDefinition::next_version_after(0).unwrap(), 1);
        assert_eq!(ApprovalProcessDefinition::next_version_after(7).unwrap(), 8);
        assert!(ApprovalProcessDefinition::next_version_after(u32::MAX).is_err());

        let definition = draft();
        assert!(definition.ensure_lock_version(definition.definition_lock_version()).is_ok());
        assert!(definition.ensure_lock_version(definition.definition_lock_version() + 1).is_err());
    }

    // 验证不能为零业务版本或空白定义名建立草稿。
    #[test]
    fn new_draft_rejects_invalid_fields() {
        let at = Timestamp::from_unix_secs(1).unwrap();
        let actor = ParticipantId::new("admin").unwrap();
        let id = ApprovalProcessDefinitionId::new("def-1");
        assert!(matches!(
            ApprovalProcessDefinition::new_draft(
                id.clone(),
                ProcessKind::StockAdjustment,
                0,
                "name",
                "n1",
                actor.clone(),
                at
            ),
            Err(ModelError::InvalidField(_))
        ));
        assert!(
            ApprovalProcessDefinition::new_draft(id, ProcessKind::StockAdjustment, 1, "  ", "n1", actor, at)
                .is_err()
        );
    }

    // 验证发布冻结草稿字段，只有当前发布版能退役，防止定义状态任意回退。
    #[test]
    fn publish_and_retire_are_stateful() {
        let mut definition = draft();
        let actor = ParticipantId::new("admin").unwrap();
        let at = Timestamp::from_unix_secs(2).unwrap();
        definition.publish(actor.clone(), at).unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Published);
        assert!(definition.is_published());
        assert!(definition.rename_draft("x", at).is_err());
        assert!(definition.ensure_mutable().is_err());

        let replacement = draft();
        let mut previous = draft();
        previous.base.id = "def-old".to_string();
        previous.publish(actor.clone(), at).unwrap();
        let (published, retired) = replacement
            .publish_replacing(Some(previous), actor.clone(), Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        assert_eq!(published.status, ApprovalDefinitionStatus::Published);
        assert_eq!(retired.unwrap().status, ApprovalDefinitionStatus::Retired);

        definition.retire(actor, Timestamp::from_unix_secs(3).unwrap()).unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Retired);
        assert!(!definition.is_published());
        assert!(!draft().is_published());
        assert!(
            definition
                .retire(ParticipantId::new("admin").unwrap(), Timestamp::from_unix_secs(4).unwrap())
                .is_err()
        );
    }
}
