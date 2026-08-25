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
    /// 规范化流程定义名称。
    ///
    /// # 参数
    /// * `name` - 调用方提供的定义名称
    ///
    /// # 返回
    /// 返回去除首尾空白后的有效名称。
    ///
    /// # 错误
    /// 名称为空或超过模型长度上限时返回错误。
    ///
    /// # 关键业务约束
    /// 幂等摘要与实体构造必须复用同一规范化结果，避免空白差异形成双份规则源。
    pub fn normalize_name(name: impl Into<String>) -> ModelResult<String> {
        normalize_required(name, "定义名称不能为空", NAME_MAX_LEN, "定义名称过长")
    }

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

    /// 返回草稿并发修改使用的乐观锁版本。
    ///
    /// # 返回
    /// 返回 `base.version`，不与 `definition_version` 混用。
    pub fn definition_lock_version(&self) -> u64 {
        self.base.version
    }

    /// 校验定义乐观锁版本与调用方预期一致。
    ///
    /// # 参数
    /// * `expected` - 调用方持有的定义锁版本
    ///
    /// # 返回
    /// 当前锁版本一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 锁版本不一致时返回状态错误。
    ///
    /// # 关键业务约束
    /// `definition_version` 是业务版本，不得用于替代持久化乐观锁版本。
    pub fn ensure_lock_version(&self, expected: u64) -> ModelResult<()> {
        if self.definition_lock_version() == expected {
            return Ok(());
        }
        Err(ModelError::InvalidStatus("定义锁版本已过期"))
    }

    /// 由历史最高业务版本计算下一定义版本。
    ///
    /// # 参数
    /// * `current_max` - 同流程种类历史最高业务版本；无历史时传入 `0`
    ///
    /// # 返回
    /// 返回单调递增的下一业务版本。
    ///
    /// # 错误
    /// `u32` 溢出时返回模型计数错误。
    ///
    /// # 关键业务约束
    /// 版本不得回绕或复用，调用方必须在持久化新草稿前完成计算。
    pub fn next_version_after(current_max: u32) -> ModelResult<u32> {
        current_max.checked_add(1).ok_or(ModelError::Overflow("定义版本"))
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

    /// 发布当前草稿并同步退役此前的已发布定义。
    ///
    /// # 参数
    /// * `previous` - 同流程种类当前已发布定义；没有历史发布版时为空
    /// * `actor` - 发布与退役操作人
    /// * `at` - 同一状态切换时间
    ///
    /// # 返回
    /// 返回已发布的新定义与可选已退役旧定义。
    ///
    /// # 错误
    /// 当前定义不是草稿或旧定义不是已发布状态时返回模型错误。
    ///
    /// # 关键业务约束
    /// 旧定义必须先按模型退役，新定义再发布；Repository 负责把两次写入置于同一事务。
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
            }
            None => None,
        };
        self.publish(actor, at)?;
        Ok((self, previous))
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

    /// 名称规范化、下一版本与锁校验由定义模型统一提供。
    #[test]
    fn definition_helpers_are_deterministic_and_fail_closed() {
        assert_eq!(
            ApprovalProcessDefinition::normalize_name(" 库存调整 ").unwrap(),
            "库存调整"
        );
        assert!(ApprovalProcessDefinition::normalize_name("   ").is_err());
        assert_eq!(ApprovalProcessDefinition::next_version_after(0).unwrap(), 1);
        assert_eq!(ApprovalProcessDefinition::next_version_after(7).unwrap(), 8);
        assert!(ApprovalProcessDefinition::next_version_after(u32::MAX).is_err());

        let definition = draft();
        assert!(definition
            .ensure_lock_version(definition.definition_lock_version())
            .is_ok());
        assert!(definition
            .ensure_lock_version(definition.definition_lock_version() + 1)
            .is_err());
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

        let replacement = draft();
        let mut previous = draft();
        previous.base.id = "def-old".to_string();
        previous.publish(actor.clone(), at).unwrap();
        let (published, retired) = replacement
            .publish_replacing(
                Some(previous),
                actor.clone(),
                Timestamp::from_unix_secs(3).unwrap(),
            )
            .unwrap();
        assert_eq!(published.status, ApprovalDefinitionStatus::Published);
        assert_eq!(retired.unwrap().status, ApprovalDefinitionStatus::Retired);

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
