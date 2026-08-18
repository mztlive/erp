//! 审批流程定义。只保存流程种类，不保存 ERP 单据类型。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::ids::ApprovalProcessDefinitionId;
use crate::model::types::{
    base_model_at, normalize_required, touch_base, ApprovalDefinitionStatus, ModelError, ModelResult,
    NAME_MAX_LEN, NODE_KEY_MAX_LEN,
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
    /// 创建草稿定义。
    ///
    /// 调用方必须提供主键、时间与处理人；本方法不读取系统时钟。
    ///
    /// # 参数
    /// * `id` - 定义主键
    /// * `process_kind` - 流程种类
    /// * `definition_version` - 业务版本，必须从 1 开始
    /// * `name` - 管理名称
    /// * `entry_node_key` - 入口节点键
    /// * `created_by` - 创建人
    /// * `at` - 创建时间
    ///
    /// # 错误
    /// 版本为零、名称为空/超长或入口键非法时返回错误。
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
            name: normalize_required(name, "定义名称不能为空", NAME_MAX_LEN, "定义名称过长")?,
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

    /// 返回草稿并发修改使用的乐观锁版本。
    ///
    /// # 返回
    /// 返回 `base.version`，不与 `definition_version` 混用。
    pub fn definition_lock_version(&self) -> u64 {
        self.base.version
    }

    /// 草稿仍可修改时返回成功。
    ///
    /// # 错误
    /// 已发布或已退役时返回 [`ModelError::InvalidStatus`]。
    pub fn ensure_mutable(&self) -> ModelResult<()> {
        if self.status.is_draft() {
            return Ok(());
        }
        Err(ModelError::InvalidStatus("只有草稿定义可以修改"))
    }

    /// 重命名草稿。
    ///
    /// # 参数
    /// * `name` - 新名称
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 非草稿或名称为空/超长时返回错误。
    pub fn rename_draft(&mut self, name: impl Into<String>, at: Timestamp) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.name = normalize_required(name, "定义名称不能为空", NAME_MAX_LEN, "定义名称过长")?;
        touch_base(&mut self.base, at)
    }

    /// 设置草稿入口节点。
    ///
    /// # 参数
    /// * `entry_node_key` - 入口节点键
    /// * `at` - 调用方时间
    ///
    /// # 错误
    /// 非草稿或入口键非法时返回错误。
    pub fn set_entry_node_draft(
        &mut self,
        entry_node_key: impl Into<String>,
        at: Timestamp,
    ) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.entry_node_key = normalize_required(
            entry_node_key,
            "入口节点键不能为空",
            NODE_KEY_MAX_LEN,
            "入口节点键过长",
        )?;
        touch_base(&mut self.base, at)
    }

    /// 将草稿发布。只维护本实体状态。
    ///
    /// # 参数
    /// * `actor` - 发布人
    /// * `at` - 发布时间
    ///
    /// # 错误
    /// 当前不是草稿时返回错误。
    pub fn publish(&mut self, actor: ParticipantId, at: Timestamp) -> ModelResult<()> {
        self.ensure_mutable()?;
        self.status = ApprovalDefinitionStatus::Published;
        self.published_by = Some(actor);
        self.published_at = Some(at);
        touch_base(&mut self.base, at)
    }

    /// 将已发布定义退役。只维护本实体状态。
    ///
    /// # 参数
    /// * `actor` - 退役人
    /// * `at` - 退役时间
    ///
    /// # 错误
    /// 当前不是已发布状态时返回错误。
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

    /// 草稿创建成功且锁版本独立于业务版本。
    #[test]
    fn new_draft_starts_mutable() {
        let definition = draft();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Draft);
        assert_eq!(definition.definition_version, 1);
        assert_eq!(definition.definition_lock_version(), 1);
        assert_eq!(definition.process_kind, ProcessKind::StockAdjustment);
        assert!(definition.ensure_mutable().is_ok());
    }

    /// 版本 0 与空名称失败关闭。
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
        assert!(ApprovalProcessDefinition::new_draft(
            id,
            ProcessKind::StockAdjustment,
            1,
            "  ",
            "n1",
            actor,
            at
        )
        .is_err());
    }

    /// 发布后不得再改草稿字段；退役只能从已发布进入。
    #[test]
    fn publish_and_retire_are_stateful() {
        let mut definition = draft();
        let actor = ParticipantId::new("admin").unwrap();
        let at = Timestamp::from_unix_secs(2).unwrap();
        definition.publish(actor.clone(), at).unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Published);
        assert!(definition.rename_draft("x", at).is_err());
        assert!(definition.ensure_mutable().is_err());
        definition
            .retire(actor, Timestamp::from_unix_secs(3).unwrap())
            .unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Retired);
        assert!(definition
            .retire(
                ParticipantId::new("admin").unwrap(),
                Timestamp::from_unix_secs(4).unwrap()
            )
            .is_err());
    }
}
