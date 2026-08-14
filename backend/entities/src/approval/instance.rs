//! `approval_instance`：冻结定义版本与业务提交版本的审批运行状态源。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{ApprovalInstanceId, ApprovalStepInstanceId};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{ApprovalInstanceStatus, ApprovalRuntimeKind};

const DEFINITION_KEY_MAX_LEN: usize = 128;
const OBJECT_TYPE_MAX_LEN: usize = 64;
const OBJECT_ID_MAX_LEN: usize = 128;
const ORGANIZATION_ID_MAX_LEN: usize = 128;
const SUBJECT_VERSION_MAX_LEN: usize = 128;
const IDEMPOTENCY_KEY_MAX_LEN: usize = 256;
const EXTERNAL_INSTANCE_ID_MAX_LEN: usize = 256;
const BLOCKER_CODE_MAX_LEN: usize = 128;
const USER_ID_MAX_LEN: usize = 128;

/// 审批实例创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalInstanceData {
    /// 启动时冻结的稳定定义编码。
    pub definition_key: String,
    /// 启动时冻结的业务定义版本。
    pub definition_version: u32,
    /// 从定义复制的运行时类型。
    pub runtime_kind: ApprovalRuntimeKind,
    /// 被审批业务对象类型。
    pub business_object_type: String,
    /// 被审批业务对象 ID。
    pub business_object_id: String,
    /// 启动时冻结的责任与授权组织上下文。
    ///
    /// 当首步骤处理人解析失败且尚无 `work_item` 时，本字段仍用于阻塞队列
    /// 数据库侧授权过滤和恢复解析，禁止回退为全量读取后隐藏。
    pub owner_organization_id: String,
    /// 被审批的不可变提交或业务版本。
    pub subject_version: String,
    /// 启动审批请求的业务幂等键。
    ///
    /// 与 `definition_key` 组成永久唯一身份；重复请求即使原实例已终态也必须回读
    /// 原实例，同一键携带不同冻结对象或版本必须返回冲突。
    pub start_idempotency_key: String,
    /// 首个当前步骤实例 ID。
    pub current_step_instance_id: ApprovalStepInstanceId,
    /// BPM 外部流程实例身份；内部运行时必须为空。
    pub external_instance_id: Option<String>,
    /// 启动人。
    pub started_by: String,
    /// 启动时间。
    pub started_at: Instant,
}

/// 审批运行实例实体。
///
/// `BaseModel.version` 是实例持久化乐观锁，API 固定映射为 `instance_version`；
/// `definition_version` 是启动时冻结的业务定义版本，二者不得混用。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalInstance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 冻结定义编码。
    pub definition_key: String,
    /// 冻结业务定义版本。
    pub definition_version: u32,
    /// 冻结运行时类型。
    pub runtime_kind: ApprovalRuntimeKind,
    /// 被审批业务对象类型。
    pub business_object_type: String,
    /// 被审批业务对象 ID。
    pub business_object_id: String,
    /// 冻结责任与授权组织上下文。
    pub owner_organization_id: String,
    /// 被审批的不可变提交或业务版本。
    pub subject_version: String,
    /// 启动请求业务幂等键。
    pub start_idempotency_key: String,
    /// 审批实例状态。
    pub status: ApprovalInstanceStatus,
    /// 当前活动或阻塞步骤；终态必须为空。
    pub current_step_instance_id: Option<ApprovalStepInstanceId>,
    /// BPM 外部流程实例身份。
    pub external_instance_id: Option<String>,
    /// 当前结构化阻塞原因。
    pub blocker_code: Option<String>,
    /// 进入当前阻塞状态的时间。
    pub blocked_at: Option<Instant>,
    /// 启动人。
    pub started_by: String,
    /// 启动时间。
    pub started_at: Instant,
    /// 审批结束时间。
    pub ended_at: Option<Instant>,
}

impl ApprovalInstance {
    /// 创建运行中的审批实例。
    ///
    /// 定义身份、业务对象身份和 `subject_version` 在此冻结，后续状态方法均不修改。
    /// 启动事务若遇到首步骤解析失败，应继续调用 [`Self::block`] 落下可恢复阻塞事实。
    ///
    /// # 参数
    /// * `id` - 审批实例主键
    /// * `data` - 实例创建数据
    ///
    /// # 返回
    /// 返回状态为 `RUNNING` 的审批实例。
    ///
    /// # 错误
    /// 冻结字段非法、业务定义版本为零，或内部运行时携带外部实例身份时返回错误。
    pub fn new(id: ApprovalInstanceId, data: ApprovalInstanceData) -> Result<Self> {
        if data.definition_version == 0 {
            return Err(Error::from("审批定义版本必须从 1 开始"));
        }
        let external_instance_id = normalize_optional_text(
            data.external_instance_id,
            "外部审批实例身份",
            EXTERNAL_INSTANCE_ID_MAX_LEN,
        )?;
        if data.runtime_kind == ApprovalRuntimeKind::Internal && external_instance_id.is_some() {
            return Err(Error::from("INTERNAL 审批实例不得设置外部实例身份"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            definition_key: normalize_required_text(
                data.definition_key,
                "审批定义编码不能为空",
                DEFINITION_KEY_MAX_LEN,
                "审批定义编码过长",
            )?,
            definition_version: data.definition_version,
            runtime_kind: data.runtime_kind,
            business_object_type: normalize_required_text(
                data.business_object_type,
                "审批业务对象类型不能为空",
                OBJECT_TYPE_MAX_LEN,
                "审批业务对象类型过长",
            )?,
            business_object_id: normalize_required_text(
                data.business_object_id,
                "审批业务对象ID不能为空",
                OBJECT_ID_MAX_LEN,
                "审批业务对象ID过长",
            )?,
            owner_organization_id: normalize_required_text(
                data.owner_organization_id,
                "审批责任组织不能为空",
                ORGANIZATION_ID_MAX_LEN,
                "审批责任组织过长",
            )?,
            subject_version: normalize_required_text(
                data.subject_version,
                "审批对象版本不能为空",
                SUBJECT_VERSION_MAX_LEN,
                "审批对象版本过长",
            )?,
            start_idempotency_key: normalize_required_text(
                data.start_idempotency_key,
                "审批启动幂等键不能为空",
                IDEMPOTENCY_KEY_MAX_LEN,
                "审批启动幂等键过长",
            )?,
            status: ApprovalInstanceStatus::Running,
            current_step_instance_id: Some(data.current_step_instance_id),
            external_instance_id,
            blocker_code: None,
            blocked_at: None,
            started_by: normalize_required_text(
                data.started_by,
                "审批启动人不能为空",
                USER_ID_MAX_LEN,
                "审批启动人过长",
            )?,
            started_at: data.started_at,
            ended_at: None,
        })
    }

    /// 返回 API 使用的审批实例乐观锁版本。
    ///
    /// # 返回
    /// 返回 `BaseModel.version`，API 必须命名为 `instance_version`。
    pub fn instance_version(&self) -> u64 {
        self.base.version
    }

    /// 绑定 BPM 外部实例身份。
    ///
    /// 仅 BPM 非终态实例可绑定。相同身份重复调用按幂等成功处理，已绑定后禁止改写。
    ///
    /// # 参数
    /// * `external_instance_id` - 唯一外部流程实例身份
    ///
    /// # 错误
    /// 实例不是 BPM、已经终结、身份非法，或试图替换既有身份时返回错误。
    pub fn set_external_instance_id(&mut self, external_instance_id: impl Into<String>) -> Result<()> {
        if self.runtime_kind != ApprovalRuntimeKind::Bpm {
            return Err(Error::from("INTERNAL 审批实例不得绑定外部实例身份"));
        }
        if self.is_terminal() {
            return Err(Error::from("终态审批实例不得绑定外部实例身份"));
        }
        let external_instance_id = normalize_required_text(
            external_instance_id.into(),
            "外部审批实例身份不能为空",
            EXTERNAL_INSTANCE_ID_MAX_LEN,
            "外部审批实例身份过长",
        )?;
        match self.external_instance_id.as_deref() {
            Some(existing) if existing == external_instance_id => Ok(()),
            Some(_) => Err(Error::from("审批实例的外部身份已冻结，不得改写")),
            None => {
                self.external_instance_id = Some(external_instance_id);
                Ok(())
            }
        }
    }

    /// 将运行实例推进到唯一下一步骤。
    ///
    /// 本方法只更新当前步骤指针；前一步决定、待办完成和下一步骤激活必须由 Service
    /// 在同一事务中完成。
    ///
    /// # 参数
    /// * `next_step_instance_id` - 已冻结串行定义中的唯一下一步骤实例
    ///
    /// # 错误
    /// 实例不是 `RUNNING` 时返回非法状态错误。
    pub fn advance_to(&mut self, next_step_instance_id: ApprovalStepInstanceId) -> Result<()> {
        self.ensure_status(ApprovalInstanceStatus::Running, "推进")?;
        self.current_step_instance_id = Some(next_step_instance_id);
        Ok(())
    }

    /// 将运行实例标记为结构化阻塞。
    ///
    /// 当前步骤指针保持不变，供 `RETRY_CURRENT_STEP` 恢复原步骤。
    ///
    /// # 参数
    /// * `blocker_code` - 服务端注册的结构化阻塞原因
    /// * `at` - 进入阻塞状态的时间
    ///
    /// # 错误
    /// 实例不是 `RUNNING`，或阻塞原因非法时返回错误。
    pub fn block(&mut self, blocker_code: impl Into<String>, at: Instant) -> Result<()> {
        self.ensure_status(ApprovalInstanceStatus::Running, "阻塞")?;
        let blocker_code = normalize_required_text(
            blocker_code.into(),
            "审批阻塞原因不能为空",
            BLOCKER_CODE_MAX_LEN,
            "审批阻塞原因过长",
        )?;
        self.status = ApprovalInstanceStatus::Blocked;
        self.blocker_code = Some(blocker_code);
        self.blocked_at = Some(at);
        Ok(())
    }

    /// 恢复原阻塞步骤继续运行。
    ///
    /// 只执行合同允许的 `RETRY_CURRENT_STEP` 语义，并清除当前阻塞字段。
    ///
    /// # 错误
    /// 实例不是 `BLOCKED` 时返回非法状态错误。
    pub fn recover(&mut self) -> Result<()> {
        self.ensure_status(ApprovalInstanceStatus::Blocked, "恢复")?;
        self.status = ApprovalInstanceStatus::Running;
        self.blocker_code = None;
        self.blocked_at = None;
        Ok(())
    }

    /// 以全部步骤通过结束审批实例。
    ///
    /// # 参数
    /// * `at` - 审批结束时间
    ///
    /// # 错误
    /// 实例不是 `RUNNING` 时返回非法状态错误。
    pub fn approve(&mut self, at: Instant) -> Result<()> {
        self.finish(ApprovalInstanceStatus::Approved, at)
    }

    /// 以驳回申请人结束审批实例。
    ///
    /// # 参数
    /// * `at` - 审批结束时间
    ///
    /// # 错误
    /// 实例不是 `RUNNING` 时返回非法状态错误。
    pub fn reject(&mut self, at: Instant) -> Result<()> {
        self.finish(ApprovalInstanceStatus::Rejected, at)
    }

    /// 以终止审批结束实例。
    ///
    /// # 参数
    /// * `at` - 审批结束时间
    ///
    /// # 错误
    /// 实例不是 `RUNNING` 时返回非法状态错误。
    pub fn terminate(&mut self, at: Instant) -> Result<()> {
        self.finish(ApprovalInstanceStatus::Terminated, at)
    }

    /// 取消尚未终结的审批实例。
    ///
    /// 业务是否允许撤回及不可逆决定校验由 `cancel_approval` Service 完成。
    ///
    /// # 参数
    /// * `at` - 取消时间
    ///
    /// # 错误
    /// 实例不是 `RUNNING` 或 `BLOCKED` 时返回非法状态错误。
    pub fn cancel(&mut self, at: Instant) -> Result<()> {
        if !matches!(
            self.status,
            ApprovalInstanceStatus::Running | ApprovalInstanceStatus::Blocked
        ) {
            return Err(self.transition_error(ApprovalInstanceStatus::Cancelled));
        }
        self.status = ApprovalInstanceStatus::Cancelled;
        self.current_step_instance_id = None;
        self.blocker_code = None;
        self.blocked_at = None;
        self.ended_at = Some(at);
        Ok(())
    }

    /// 判断实例是否已处于不可逆终态。
    ///
    /// # 返回
    /// 状态为通过、驳回、终止或取消时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    fn finish(&mut self, target: ApprovalInstanceStatus, at: Instant) -> Result<()> {
        self.ensure_status(ApprovalInstanceStatus::Running, "结束")?;
        debug_assert!(target.is_terminal());
        self.status = target;
        self.current_step_instance_id = None;
        self.blocker_code = None;
        self.blocked_at = None;
        self.ended_at = Some(at);
        Ok(())
    }

    fn ensure_status(&self, expected: ApprovalInstanceStatus, _action: &str) -> Result<()> {
        if self.status == expected {
            return Ok(());
        }
        Err(self.transition_error(expected))
    }

    fn transition_error(&self, target: ApprovalInstanceStatus) -> Error {
        Error::InvalidStateTransition {
            from: format!("{:?}", self.status),
            to: format!("{target:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalInstance, ApprovalInstanceData};
    use crate::approval::{ApprovalInstanceStatus, ApprovalRuntimeKind};
    use crate::common::time::Instant;
    use crate::ids::{ApprovalInstanceId, ApprovalStepInstanceId};

    fn data() -> ApprovalInstanceData {
        ApprovalInstanceData {
            definition_key: " SALES_ORDER_APPROVAL ".to_string(),
            definition_version: 2,
            runtime_kind: ApprovalRuntimeKind::Internal,
            business_object_type: " SALES_ORDER ".to_string(),
            business_object_id: " sales-order-1 ".to_string(),
            owner_organization_id: " organization-1 ".to_string(),
            subject_version: " submission-3 ".to_string(),
            start_idempotency_key: " start-request-1 ".to_string(),
            current_step_instance_id: ApprovalStepInstanceId::new("step-instance-1"),
            external_instance_id: None,
            started_by: " user-1 ".to_string(),
            started_at: Instant::from_unix_secs(1_700_000_000),
        }
    }

    #[test]
    fn new_instance_freezes_identity_and_exposes_lock_version_mapping() {
        let instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), data()).unwrap();
        assert_eq!(instance.definition_key, "SALES_ORDER_APPROVAL");
        assert_eq!(instance.definition_version, 2);
        assert_eq!(instance.business_object_type, "SALES_ORDER");
        assert_eq!(instance.owner_organization_id, "organization-1");
        assert_eq!(instance.subject_version, "submission-3");
        assert_eq!(instance.start_idempotency_key, "start-request-1");
        assert_eq!(instance.status, ApprovalInstanceStatus::Running);
        assert_eq!(instance.instance_version(), instance.base.version);
        assert_eq!(
            instance.current_step_instance_id,
            Some(ApprovalStepInstanceId::new("step-instance-1"))
        );
    }

    #[test]
    fn new_instance_requires_idempotency_key_and_authorization_organization() {
        let missing_idempotency = ApprovalInstanceData {
            start_idempotency_key: "   ".to_string(),
            ..data()
        };
        assert!(ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), missing_idempotency,).is_err());

        let missing_organization = ApprovalInstanceData {
            owner_organization_id: "   ".to_string(),
            ..data()
        };
        assert!(ApprovalInstance::new(ApprovalInstanceId::new("instance-2"), missing_organization,).is_err());
    }

    #[test]
    fn block_and_recover_preserve_current_step() {
        let mut instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), data()).unwrap();
        let current = instance.current_step_instance_id.clone();
        instance
            .block(" ASSIGNEE_NOT_FOUND ", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(instance.status, ApprovalInstanceStatus::Blocked);
        assert_eq!(instance.blocker_code.as_deref(), Some("ASSIGNEE_NOT_FOUND"));
        assert_eq!(instance.current_step_instance_id, current);
        assert!(instance
            .advance_to(ApprovalStepInstanceId::new("step-instance-2"))
            .is_err());

        instance.recover().unwrap();
        assert_eq!(instance.status, ApprovalInstanceStatus::Running);
        assert!(instance.blocker_code.is_none());
        assert!(instance.blocked_at.is_none());
        assert_eq!(instance.current_step_instance_id, current);
    }

    #[test]
    fn terminal_transition_clears_current_pointer_and_sets_end_audit() {
        let mut instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), data()).unwrap();
        let ended_at = Instant::from_unix_secs(1_700_000_200);
        instance.approve(ended_at).unwrap();
        assert_eq!(instance.status, ApprovalInstanceStatus::Approved);
        assert!(instance.current_step_instance_id.is_none());
        assert_eq!(instance.ended_at, Some(ended_at));
        assert!(instance.reject(Instant::from_unix_secs(1_700_000_300)).is_err());
    }

    #[test]
    fn blocked_instance_can_be_cancelled_but_not_directly_approved() {
        let mut instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), data()).unwrap();
        instance
            .block("MISSING_REGISTRY", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert!(instance.approve(Instant::from_unix_secs(1_700_000_200)).is_err());
        instance.cancel(Instant::from_unix_secs(1_700_000_300)).unwrap();
        assert_eq!(instance.status, ApprovalInstanceStatus::Cancelled);
        assert!(instance.current_step_instance_id.is_none());
        assert!(instance.blocker_code.is_none());
    }

    #[test]
    fn bpm_external_identity_is_stable_and_internal_rejects_it() {
        let internal = ApprovalInstanceData {
            external_instance_id: Some("external-1".to_string()),
            ..data()
        };
        assert!(ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), internal).is_err());

        let bpm = ApprovalInstanceData {
            runtime_kind: ApprovalRuntimeKind::Bpm,
            ..data()
        };
        let mut instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-2"), bpm).unwrap();
        instance.set_external_instance_id("external-2").unwrap();
        instance.set_external_instance_id("external-2").unwrap();
        assert!(instance.set_external_instance_id("external-3").is_err());
    }

    #[test]
    fn entity_roundtrips_through_bson_with_distinct_versions() {
        let instance = ApprovalInstance::new(ApprovalInstanceId::new("instance-1"), data()).unwrap();
        let document = bson::to_document(&instance).unwrap();
        assert_eq!(document.get_i64("version").unwrap(), 1);
        assert_eq!(document.get_i64("definition_version").unwrap(), 2);
        let roundtrip: ApprovalInstance = bson::from_document(document).unwrap();
        assert_eq!(roundtrip, instance);
    }
}
