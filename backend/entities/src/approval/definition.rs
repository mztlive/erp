//! `approval_definition`：显式版本化的审批定义。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::ApprovalDefinitionId;
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{ApprovalDefinitionStatus, ApprovalRuntimeKind};

const DEFINITION_KEY_MAX_LEN: usize = 128;
const DEFINITION_NAME_MAX_LEN: usize = 256;
const EXTERNAL_DEFINITION_ID_MAX_LEN: usize = 256;
const USER_ID_MAX_LEN: usize = 128;

/// 审批定义创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalDefinitionData {
    /// 稳定定义编码。
    pub definition_key: String,
    /// 同一稳定定义编码内单调递增的业务定义版本。
    ///
    /// 物理字段固定为 `definition_version`；扁平化 `BaseModel.version` 是持久化
    /// 乐观锁版本，二者不得混用。
    pub definition_version: u32,
    /// 管理与审计名称。
    pub name: String,
    /// 审批运行时类型。
    pub runtime_kind: ApprovalRuntimeKind,
    /// BPM 外部定义身份；内部运行时必须为空。
    pub external_definition_id: Option<String>,
}

/// 审批定义实体。
///
/// 定义初始为 `DRAFT`。发布操作只固化定义状态和发布审计；步骤连续性、注册表、
/// 下一步语义和最终强类型领域动作必须由发布 Service 在同一受控操作中先行校验。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalDefinition {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 稳定定义编码。
    pub definition_key: String,
    /// 业务定义版本；物理字段名与 API 均为 `definition_version`。
    pub definition_version: u32,
    /// 管理与审计名称。
    pub name: String,
    /// 审批运行时类型。
    pub runtime_kind: ApprovalRuntimeKind,
    /// 定义状态。
    pub status: ApprovalDefinitionStatus,
    /// BPM 外部定义身份。
    pub external_definition_id: Option<String>,
    /// 发布时间。
    pub published_at: Option<Instant>,
    /// 发布人。
    pub published_by: Option<String>,
}

impl ApprovalDefinition {
    /// 创建草稿审批定义。
    ///
    /// `definition_version` 是业务定义版本；`base.version` 独立作为持久化乐观锁版本。
    ///
    /// # 参数
    /// * `id` - 审批定义主键
    /// * `data` - 定义创建数据
    ///
    /// # 返回
    /// 返回规范化后的草稿定义。
    ///
    /// # 错误
    /// 定义编码或名称为空/超长、定义版本为零，或内部运行时携带外部定义身份时返回错误。
    pub fn new(id: ApprovalDefinitionId, data: ApprovalDefinitionData) -> Result<Self> {
        if data.definition_version == 0 {
            return Err(Error::from("审批定义版本必须从 1 开始"));
        }
        let definition_key = normalize_required_text(
            data.definition_key,
            "审批定义编码不能为空",
            DEFINITION_KEY_MAX_LEN,
            "审批定义编码过长",
        )?;
        let name = normalize_required_text(
            data.name,
            "审批定义名称不能为空",
            DEFINITION_NAME_MAX_LEN,
            "审批定义名称过长",
        )?;
        let external_definition_id = normalize_optional_text(
            data.external_definition_id,
            "外部审批定义身份",
            EXTERNAL_DEFINITION_ID_MAX_LEN,
        )?;
        if data.runtime_kind == ApprovalRuntimeKind::Internal && external_definition_id.is_some() {
            return Err(Error::from("INTERNAL 审批定义不得设置外部定义身份"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            definition_key,
            definition_version: data.definition_version,
            name,
            runtime_kind: data.runtime_kind,
            status: ApprovalDefinitionStatus::Draft,
            external_definition_id,
            published_at: None,
            published_by: None,
        })
    }

    /// 修改草稿定义的管理名称。
    ///
    /// 已发布或已退役定义不可修改；语义变化必须创建新业务版本。
    ///
    /// # 参数
    /// * `name` - 新的管理与审计名称
    ///
    /// # 错误
    /// 定义不是草稿，或名称为空/超长时返回错误。
    pub fn rename(&mut self, name: impl Into<String>) -> Result<()> {
        self.ensure_draft()?;
        self.name = normalize_required_text(
            name.into(),
            "审批定义名称不能为空",
            DEFINITION_NAME_MAX_LEN,
            "审批定义名称过长",
        )?;
        Ok(())
    }

    /// 修改草稿 BPM 定义的外部身份。
    ///
    /// `INTERNAL` 定义始终禁止保存外部定义身份；空白值按清空处理。
    ///
    /// # 参数
    /// * `external_definition_id` - BPM 外部定义身份
    ///
    /// # 错误
    /// 定义不是草稿、值超长，或内部运行时试图设置外部身份时返回错误。
    pub fn set_external_definition_id(&mut self, external_definition_id: Option<String>) -> Result<()> {
        self.ensure_draft()?;
        let external_definition_id = normalize_optional_text(
            external_definition_id,
            "外部审批定义身份",
            EXTERNAL_DEFINITION_ID_MAX_LEN,
        )?;
        if self.runtime_kind == ApprovalRuntimeKind::Internal && external_definition_id.is_some() {
            return Err(Error::from("INTERNAL 审批定义不得设置外部定义身份"));
        }
        self.external_definition_id = external_definition_id;
        Ok(())
    }

    /// 发布审批定义并写入发布审计。
    ///
    /// 本方法只允许 `DRAFT → PUBLISHED`。调用前，Service 必须已完成步骤连续性、
    /// 注册表、路由和强类型终结动作的全部发布校验。
    ///
    /// # 参数
    /// * `published_by` - 发布人
    /// * `at` - 发布时间
    ///
    /// # 错误
    /// 状态不允许发布、发布人非法，或 BPM 定义缺少外部定义身份时返回错误。
    pub fn publish(&mut self, published_by: impl Into<String>, at: Instant) -> Result<()> {
        if self.status != ApprovalDefinitionStatus::Draft {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", ApprovalDefinitionStatus::Published),
            });
        }
        if self.runtime_kind == ApprovalRuntimeKind::Bpm && self.external_definition_id.is_none() {
            return Err(Error::from("BPM 审批定义发布前必须配置外部定义身份"));
        }
        let published_by = normalize_required_text(
            published_by.into(),
            "发布人不能为空",
            USER_ID_MAX_LEN,
            "发布人过长",
        )?;
        self.status = ApprovalDefinitionStatus::Published;
        self.published_at = Some(at);
        self.published_by = Some(published_by);
        Ok(())
    }

    /// 退役已发布审批定义。
    ///
    /// 退役后不再允许启动新实例，既有实例仍永久绑定本定义版本。
    ///
    /// # 错误
    /// 定义不是已发布状态时返回非法状态迁移错误。
    pub fn retire(&mut self) -> Result<()> {
        if self.status != ApprovalDefinitionStatus::Published {
            return Err(Error::InvalidStateTransition {
                from: format!("{:?}", self.status),
                to: format!("{:?}", ApprovalDefinitionStatus::Retired),
            });
        }
        self.status = ApprovalDefinitionStatus::Retired;
        Ok(())
    }

    /// 判断定义内容是否仍允许修改。
    ///
    /// # 返回
    /// 仅草稿定义返回 `true`。
    pub fn is_mutable(&self) -> bool {
        self.status == ApprovalDefinitionStatus::Draft
    }

    fn ensure_draft(&self) -> Result<()> {
        if self.is_mutable() {
            return Ok(());
        }
        Err(Error::from("已发布或已退役审批定义不可修改"))
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalDefinition, ApprovalDefinitionData};
    use crate::approval::{ApprovalDefinitionStatus, ApprovalRuntimeKind};
    use crate::common::time::Instant;
    use crate::ids::ApprovalDefinitionId;

    fn internal_data() -> ApprovalDefinitionData {
        ApprovalDefinitionData {
            definition_key: " SALES_ORDER_APPROVAL ".to_string(),
            definition_version: 2,
            name: " 销售单审批 v2 ".to_string(),
            runtime_kind: ApprovalRuntimeKind::Internal,
            external_definition_id: None,
        }
    }

    #[test]
    fn new_definition_trims_fields_and_separates_definition_from_lock_version() {
        let definition =
            ApprovalDefinition::new(ApprovalDefinitionId::new("definition-2"), internal_data()).unwrap();
        assert_eq!(definition.definition_key, "SALES_ORDER_APPROVAL");
        assert_eq!(definition.definition_version, 2);
        assert_eq!(definition.base.version, 1);
        assert_eq!(definition.status, ApprovalDefinitionStatus::Draft);

        let document = bson::to_document(&definition).unwrap();
        assert_eq!(document.get_i64("version").unwrap(), 1);
        assert_eq!(document.get_i64("definition_version").unwrap(), 2);
    }

    #[test]
    fn definition_version_must_start_at_one() {
        let data = ApprovalDefinitionData {
            definition_version: 0,
            ..internal_data()
        };
        assert!(ApprovalDefinition::new(ApprovalDefinitionId::new("definition-0"), data).is_err());
    }

    #[test]
    fn bpm_definition_requires_external_identity_when_published() {
        let data = ApprovalDefinitionData {
            runtime_kind: ApprovalRuntimeKind::Bpm,
            ..internal_data()
        };
        let mut definition =
            ApprovalDefinition::new(ApprovalDefinitionId::new("definition-2"), data).unwrap();
        assert!(definition
            .publish("operator-1", Instant::from_unix_secs(1_700_000_000))
            .is_err());
        definition
            .set_external_definition_id(Some(" process:sales-order:v2 ".to_string()))
            .unwrap();
        definition
            .publish("operator-1", Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Published);
        assert_eq!(
            definition.external_definition_id.as_deref(),
            Some("process:sales-order:v2")
        );
    }

    #[test]
    fn published_definition_is_frozen_and_can_only_retire() {
        let mut definition =
            ApprovalDefinition::new(ApprovalDefinitionId::new("definition-2"), internal_data()).unwrap();
        definition
            .publish("operator-1", Instant::from_unix_secs(1_700_000_000))
            .unwrap();
        assert!(!definition.is_mutable());
        assert!(definition.rename("new name").is_err());
        assert!(definition
            .publish("operator-1", Instant::from_unix_secs(1_700_000_001))
            .is_err());
        definition.retire().unwrap();
        assert_eq!(definition.status, ApprovalDefinitionStatus::Retired);
        assert!(definition.retire().is_err());
    }

    #[test]
    fn internal_definition_rejects_external_identity() {
        let data = ApprovalDefinitionData {
            external_definition_id: Some("external-process".to_string()),
            ..internal_data()
        };
        assert!(ApprovalDefinition::new(ApprovalDefinitionId::new("definition-2"), data).is_err());
    }
}
