//! `master_mapping_task`：商城快照基础资料映射任务（数据模型 §6.13）。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::state::{ensure_transition, DocumentState};
use crate::common::time::Instant;
use crate::errors::Result;
use crate::ids::MallSalesOrderSnapshotId;
use crate::source_registry::{ExternalObjectType, RelationRole};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 责任角色最大长度。
const ROLE_MAX_LEN: usize = 128;
/// 责任用户 ID 最大长度。
const USER_MAX_LEN: usize = 128;
/// 处理结论最大长度。
const RESOLUTION_MAX_LEN: usize = 1024;

/// 映射类型（数据模型 §6.13：客户、合同、结算主体、卡券类目、唯一明细、金额格式）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingTaskType {
    /// 客户。
    Customer,
    /// 合同。
    Contract,
    /// 结算主体。
    SettlementEntity,
    /// 卡券类目。
    VoucherCategory,
    /// 唯一明细。
    UniqueLineItem,
    /// 金额格式。
    AmountFormat,
}

/// W17 可确认映射类型的固定注册项。
///
/// 注册项同时冻结来源身份字段、协议目标类型、ERP 规范对象类型与谱系关系；
/// 未注册的差异类型只能追加来源修复证据，不能提交 `CONFIRM_TARGET`。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct MappingTargetRegistration {
    /// HTTP 命令使用的目标类型代码。
    pub command_object_type: &'static str,
    /// 外部身份与内部目标共同使用的对象目录类型。
    pub object_type: ExternalObjectType,
    /// 规范化快照内允许承载来源身份的字段，按优先级排列。
    pub source_identity_fields: &'static [&'static str],
    /// 固定谱系关系。
    pub relation_role: RelationRole,
}

impl MappingTaskType {
    /// 返回类型的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Customer => "客户",
            Self::Contract => "合同",
            Self::SettlementEntity => "结算主体",
            Self::VoucherCategory => "卡券类目",
            Self::UniqueLineItem => "唯一明细",
            Self::AmountFormat => "金额格式",
        }
    }

    /// 返回类型的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Customer => "customer",
            Self::Contract => "contract",
            Self::SettlementEntity => "settlement_entity",
            Self::VoucherCategory => "voucher_category",
            Self::UniqueLineItem => "unique_line_item",
            Self::AmountFormat => "amount_format",
        }
    }

    /// 返回当前差异类型的固定目标注册项。
    ///
    /// `UniqueLineItem` 与 `AmountFormat` 是快照内容校验，不对应独立 ERP
    /// 规范身份，因此明确返回 `None`，调用方必须失败关闭确认动作。
    pub fn target_registration(self) -> Option<MappingTargetRegistration> {
        match self {
            Self::Customer => Some(MappingTargetRegistration {
                command_object_type: "CUSTOMER",
                object_type: ExternalObjectType::Customer,
                source_identity_fields: &["customer_external_id", "customer_id", "company_id"],
                relation_role: RelationRole::Primary,
            }),
            Self::Contract => Some(MappingTargetRegistration {
                command_object_type: "CONTRACT",
                object_type: ExternalObjectType::Contract,
                source_identity_fields: &["contract_external_id", "contract_no", "contract_id"],
                relation_role: RelationRole::Primary,
            }),
            Self::SettlementEntity => Some(MappingTargetRegistration {
                command_object_type: "SETTLEMENT_PARTY",
                object_type: ExternalObjectType::Party,
                source_identity_fields: &[
                    "settlement_party_external_id",
                    "settlement_party_id",
                    "parent_company_id",
                ],
                relation_role: RelationRole::Primary,
            }),
            Self::VoucherCategory => Some(MappingTargetRegistration {
                command_object_type: "VOUCHER_CATEGORY",
                object_type: ExternalObjectType::VoucherCategory,
                source_identity_fields: &["voucher_category_external_id", "card_type_id", "category_id"],
                relation_role: RelationRole::Primary,
            }),
            Self::UniqueLineItem | Self::AmountFormat => None,
        }
    }
}

/// 映射任务状态（数据模型 §6.13：待处理、已解决、无法处理、关闭）。
///
/// 固定状态机：待处理单向推进到已解决、无法处理或关闭；
/// 关闭表示被新任务取代或快照失效（同一快照、映射类型只允许一个进行中任务）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum MappingTaskStatus {
    /// 待处理。
    Pending,
    /// 已解决。
    Resolved,
    /// 无法处理。
    Unresolvable,
    /// 关闭。
    Closed,
}

impl MappingTaskStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Pending => "待处理",
            Self::Resolved => "已解决",
            Self::Unresolvable => "无法处理",
            Self::Closed => "关闭",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Pending => "pending",
            Self::Resolved => "resolved",
            Self::Unresolvable => "unresolvable",
            Self::Closed => "closed",
        }
    }
}

impl DocumentState for MappingTaskStatus {
    fn allowed_next(self) -> &'static [Self] {
        match self {
            Self::Pending => &[Self::Resolved, Self::Unresolvable, Self::Closed],
            Self::Resolved | Self::Unresolvable | Self::Closed => &[],
        }
    }
}

/// 映射任务创建数据（数据模型 §6.13）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MasterMappingTaskData {
    /// 待处理快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 业务责任角色；未形成唯一责任路由时为空。
    pub owner_role: Option<String>,
    /// 业务责任用户 ID（可按角色领办，可为空）。
    pub owner_user_id: Option<String>,
}

/// 商城快照基础资料映射任务实体（数据模型 §6.13）。
///
/// 差异解决后使用原快照和原幂等身份重新归集，不手工补建另一张销售单
/// （§6.13）；系统管理员可补拉和重试，但不能替代销售、运营、财务确认
/// 业务映射。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Entity)]
pub struct MasterMappingTask {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 待处理快照。
    pub source_snapshot_id: MallSalesOrderSnapshotId,
    /// 映射类型。
    pub mapping_type: MappingTaskType,
    /// 任务状态。
    pub status: MappingTaskStatus,
    /// 业务责任角色；未形成唯一责任路由时为空。
    pub owner_role: Option<String>,
    /// 业务责任用户 ID。
    pub owner_user_id: Option<String>,
    /// 处理结论。
    pub resolution: Option<String>,
    /// 完成时间。
    pub resolved_at: Option<Instant>,
}

impl MasterMappingTask {
    /// 创建映射任务。
    ///
    /// 完成责任角色与责任用户的校验与规范化；未形成唯一责任路由时角色与
    /// 用户必须同时为空。任务创建即待处理。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::MasterMappingTaskId`）
    /// * `data` - 创建数据
    ///
    /// # 返回
    /// 返回新建的映射任务实体。
    ///
    /// # 错误
    /// 责任角色/用户超长，或无责任角色却预填责任用户时返回错误。
    pub fn new(id: crate::ids::MasterMappingTaskId, data: MasterMappingTaskData) -> Result<Self> {
        let owner_role = normalize_optional_text(data.owner_role, "责任角色", ROLE_MAX_LEN)?;
        let owner_user_id = normalize_optional_text(data.owner_user_id, "责任用户", USER_MAX_LEN)?;
        if owner_role.is_none() && owner_user_id.is_some() {
            return Err("未配置责任角色时不得预填责任用户".into());
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            source_snapshot_id: data.source_snapshot_id,
            mapping_type: data.mapping_type,
            status: MappingTaskStatus::Pending,
            owner_role,
            owner_user_id,
            resolution: None,
            resolved_at: None,
        })
    }

    /// 登记映射解决。
    ///
    /// # 参数
    /// * `resolution` - 处理结论（映射结果说明）
    /// * `resolved_at` - 完成时间
    ///
    /// # 返回
    /// 解决成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务已离开待处理状态，或处理结论为空/超长时返回错误。
    pub fn resolve(&mut self, resolution: String, resolved_at: Instant) -> Result<()> {
        ensure_transition(self.status, MappingTaskStatus::Resolved)?;
        self.status = MappingTaskStatus::Resolved;
        self.resolution = Some(normalize_required_text(
            resolution,
            "处理结论不能为空",
            RESOLUTION_MAX_LEN,
            "处理结论过长",
        )?);
        self.resolved_at = Some(resolved_at);
        Ok(())
    }

    /// 登记无法处理。
    ///
    /// # 参数
    /// * `resolution` - 处理结论（无法处理的原因）
    /// * `resolved_at` - 完成时间
    ///
    /// # 返回
    /// 登记成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务已离开待处理状态，或处理结论为空/超长时返回错误。
    pub fn mark_unresolvable(&mut self, resolution: String, resolved_at: Instant) -> Result<()> {
        ensure_transition(self.status, MappingTaskStatus::Unresolvable)?;
        self.status = MappingTaskStatus::Unresolvable;
        self.resolution = Some(normalize_required_text(
            resolution,
            "处理结论不能为空",
            RESOLUTION_MAX_LEN,
            "处理结论过长",
        )?);
        self.resolved_at = Some(resolved_at);
        Ok(())
    }

    /// 关闭任务。
    ///
    /// 同一快照、映射类型只允许一个进行中任务（§6.13）：新任务取代或快照
    /// 失效时由系统关闭本任务，不携带处理结论。
    ///
    /// # 参数
    /// * `resolved_at` - 关闭时间
    ///
    /// # 返回
    /// 关闭成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 任务已离开待处理状态时返回错误。
    pub fn close(&mut self, resolved_at: Instant) -> Result<()> {
        ensure_transition(self.status, MappingTaskStatus::Closed)?;
        self.status = MappingTaskStatus::Closed;
        self.resolved_at = Some(resolved_at);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::common::state::ensure_transition;
    use crate::ids::MasterMappingTaskId;

    fn task_data() -> MasterMappingTaskData {
        MasterMappingTaskData {
            source_snapshot_id: MallSalesOrderSnapshotId::new("snap-1"),
            mapping_type: MappingTaskType::Customer,
            owner_role: Some(" 销售 ".to_string()),
            owner_user_id: Some(" user-1 ".to_string()),
        }
    }

    #[test]
    fn new_trims_and_starts_pending() {
        let task = MasterMappingTask::new(MasterMappingTaskId::new("t-1"), task_data()).unwrap();

        assert_eq!(task.source_snapshot_id, MallSalesOrderSnapshotId::new("snap-1"));
        assert_eq!(task.mapping_type, MappingTaskType::Customer);
        assert_eq!(task.owner_role.as_deref(), Some("销售"));
        assert_eq!(task.owner_user_id.as_deref(), Some("user-1"));
        assert_eq!(task.status, MappingTaskStatus::Pending);
        assert!(task.resolution.is_none());
    }

    #[test]
    fn new_rejects_empty_and_overlong_fields() {
        let unassigned_route = MasterMappingTaskData {
            owner_role: None,
            owner_user_id: None,
            ..task_data()
        };
        let task = MasterMappingTask::new(MasterMappingTaskId::new("t-2"), unassigned_route).unwrap();
        assert_eq!(task.owner_role, None);

        let user_without_role = MasterMappingTaskData {
            owner_role: None,
            ..task_data()
        };
        assert!(MasterMappingTask::new(MasterMappingTaskId::new("t-3"), user_without_role).is_err());

        let overlong_user = MasterMappingTaskData {
            owner_user_id: Some("u".repeat(USER_MAX_LEN + 1)),
            ..task_data()
        };
        assert!(MasterMappingTask::new(MasterMappingTaskId::new("t-4"), overlong_user).is_err());
    }

    #[test]
    fn resolve_requires_resolution_text() {
        let mut task = MasterMappingTask::new(MasterMappingTaskId::new("t-4"), task_data()).unwrap();
        assert!(task
            .resolve(String::new(), Instant::from_unix_secs(1_700_000_000))
            .is_err());

        task.resolve(
            " 映射到客户 C-1 ".to_string(),
            Instant::from_unix_secs(1_700_000_000),
        )
        .unwrap();
        assert_eq!(task.status, MappingTaskStatus::Resolved);
        assert_eq!(task.resolution.as_deref(), Some("映射到客户 C-1"));
        assert_eq!(task.resolved_at, Some(Instant::from_unix_secs(1_700_000_000)));

        assert!(
            task.close(Instant::from_unix_secs(1_700_000_100)).is_err(),
            "终态不可再操作"
        );
    }

    #[test]
    fn unresolvable_and_close_are_terminal() {
        let mut unresolvable = MasterMappingTask::new(MasterMappingTaskId::new("t-5"), task_data()).unwrap();
        unresolvable
            .mark_unresolvable(
                " 来源数据无法修复 ".to_string(),
                Instant::from_unix_secs(1_700_000_000),
            )
            .unwrap();
        assert_eq!(unresolvable.status, MappingTaskStatus::Unresolvable);
        assert!(unresolvable
            .resolve("x".to_string(), Instant::from_unix_secs(1_700_000_100))
            .is_err());

        let mut closed = MasterMappingTask::new(MasterMappingTaskId::new("t-6"), task_data()).unwrap();
        closed.close(Instant::from_unix_secs(1_700_000_000)).unwrap();
        assert_eq!(closed.status, MappingTaskStatus::Closed);
        assert!(closed.resolution.is_none(), "系统关闭不携带处理结论");
    }

    #[test]
    fn status_machine_is_directed() {
        assert!(ensure_transition(MappingTaskStatus::Pending, MappingTaskStatus::Resolved).is_ok());
        assert!(ensure_transition(MappingTaskStatus::Pending, MappingTaskStatus::Unresolvable).is_ok());
        assert!(ensure_transition(MappingTaskStatus::Pending, MappingTaskStatus::Closed).is_ok());
        assert!(ensure_transition(MappingTaskStatus::Closed, MappingTaskStatus::Pending).is_err());
        assert!(ensure_transition(MappingTaskStatus::Resolved, MappingTaskStatus::Unresolvable).is_err());
    }

    #[test]
    fn type_and_status_serde_use_stable_codes() {
        assert_eq!(
            serde_json::to_string(&MappingTaskType::SettlementEntity).unwrap(),
            "\"settlement_entity\""
        );
        assert_eq!(
            serde_json::to_string(&MappingTaskType::AmountFormat).unwrap(),
            "\"amount_format\""
        );
        assert_eq!(
            serde_json::to_string(&MappingTaskStatus::Unresolvable).unwrap(),
            "\"unresolvable\""
        );
        assert_eq!(MappingTaskType::VoucherCategory.label(), "卡券类目");
        assert_eq!(MappingTaskStatus::Closed.label(), "关闭");
        let contract = MappingTaskType::Contract.target_registration().unwrap();
        assert_eq!(contract.command_object_type, "CONTRACT");
        assert_eq!(contract.object_type, ExternalObjectType::Contract);
        assert_eq!(contract.relation_role, RelationRole::Primary);
        assert!(MappingTaskType::UniqueLineItem.target_registration().is_none());
        assert!(MappingTaskType::AmountFormat.target_registration().is_none());
    }

    #[test]
    fn bson_roundtrip_preserves_entity() {
        let task = MasterMappingTask::new(MasterMappingTaskId::new("t-7"), task_data()).unwrap();
        let roundtrip: MasterMappingTask = bson::from_document(bson::to_document(&task).unwrap()).unwrap();
        assert_eq!(roundtrip, task);
    }
}
