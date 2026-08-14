//! `approval_step_instance`：单个冻结审批步骤的运行事实与决定审计。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{ApprovalInstanceId, ApprovalStepInstanceId};
use crate::validation::{normalize_optional_text, normalize_required_text};

use super::{ApprovalDecision, ApprovalStepStatus};

const STEP_KEY_MAX_LEN: usize = 128;
const EXTERNAL_ACTIVITY_ID_MAX_LEN: usize = 256;
const DECISION_REASON_MAX_LEN: usize = 2_000;
const BLOCKER_CODE_MAX_LEN: usize = 128;
const USER_ID_MAX_LEN: usize = 128;

/// 审批步骤实例创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalStepInstanceData {
    /// 所属审批实例。
    pub approval_instance_id: ApprovalInstanceId,
    /// 从冻结步骤定义复制的步骤编码。
    pub step_key: String,
    /// 从冻结步骤定义复制的串行顺序号。
    pub sequence_no: u32,
    /// 初始状态，只允许 `WAITING` 或首步骤的 `ACTIVE`。
    pub initial_status: ApprovalStepStatus,
    /// BPM 外部活动身份。
    pub external_activity_id: Option<String>,
}

/// 审批步骤实例实体。
///
/// `BaseModel.version` 是步骤持久化乐观锁，API 固定映射为 `step_version`。
/// 正式决定只能从 `ACTIVE` 形成；`WAITING` 步骤不得提前创建 `work_item`。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalStepInstance {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 所属审批实例。
    pub approval_instance_id: ApprovalInstanceId,
    /// 冻结步骤编码。
    pub step_key: String,
    /// 冻结串行顺序号。
    pub sequence_no: u32,
    /// 步骤运行状态。
    pub status: ApprovalStepStatus,
    /// 正式决定；仅决定形成后写入。
    pub decision: Option<ApprovalDecision>,
    /// 正式决定原因。
    pub decision_reason: Option<String>,
    /// 决定执行人。
    pub decided_by: Option<String>,
    /// 决定时间。
    pub decided_at: Option<Instant>,
    /// BPM 外部活动身份。
    pub external_activity_id: Option<String>,
    /// 当前结构化阻塞原因。
    pub blocker_code: Option<String>,
    /// 进入当前阻塞状态的时间。
    pub blocked_at: Option<Instant>,
}

impl ApprovalStepInstance {
    /// 创建等待或活动中的步骤实例。
    ///
    /// 启动审批必须一次创建全部步骤实例：首步骤使用 `ACTIVE`，其余步骤使用
    /// `WAITING`。首步骤解析失败时可在创建后调用 [`Self::block`]。
    ///
    /// # 参数
    /// * `id` - 步骤实例主键
    /// * `data` - 步骤实例创建数据
    ///
    /// # 返回
    /// 返回未形成决定且未阻塞的步骤实例。
    ///
    /// # 错误
    /// 顺序号为零、文本字段非法，或初始状态不是 `WAITING`/`ACTIVE` 时返回错误。
    pub fn new(id: ApprovalStepInstanceId, data: ApprovalStepInstanceData) -> Result<Self> {
        if data.sequence_no == 0 {
            return Err(Error::from("审批步骤顺序号必须从 1 开始"));
        }
        if !matches!(
            data.initial_status,
            ApprovalStepStatus::Waiting | ApprovalStepStatus::Active
        ) {
            return Err(Error::from("审批步骤初始状态只能是 WAITING 或 ACTIVE"));
        }

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            approval_instance_id: data.approval_instance_id,
            step_key: normalize_required_text(
                data.step_key,
                "审批步骤编码不能为空",
                STEP_KEY_MAX_LEN,
                "审批步骤编码过长",
            )?,
            sequence_no: data.sequence_no,
            status: data.initial_status,
            decision: None,
            decision_reason: None,
            decided_by: None,
            decided_at: None,
            external_activity_id: normalize_optional_text(
                data.external_activity_id,
                "外部审批活动身份",
                EXTERNAL_ACTIVITY_ID_MAX_LEN,
            )?,
            blocker_code: None,
            blocked_at: None,
        })
    }

    /// 返回 API 使用的步骤实例乐观锁版本。
    ///
    /// # 返回
    /// 返回 `BaseModel.version`，API 必须命名为 `step_version`。
    pub fn step_version(&self) -> u64 {
        self.base.version
    }

    /// 激活唯一下一步骤。
    ///
    /// 激活只允许 `WAITING → ACTIVE`；开放待办必须由 Service 在同一事务创建。
    ///
    /// # 错误
    /// 步骤不是 `WAITING` 时返回非法状态错误。
    pub fn activate(&mut self) -> Result<()> {
        if self.status != ApprovalStepStatus::Waiting {
            return Err(self.transition_error(ApprovalStepStatus::Active));
        }
        self.status = ApprovalStepStatus::Active;
        Ok(())
    }

    /// 绑定 BPM 外部活动身份。
    ///
    /// 相同身份重复调用按幂等成功处理，已绑定后禁止改写；运行时种类与外部相关性
    /// 必须由所属审批实例和 BPM Runtime 一并校验。
    ///
    /// # 参数
    /// * `external_activity_id` - 唯一外部活动身份
    ///
    /// # 错误
    /// 步骤已终结、身份非法，或试图替换既有身份时返回错误。
    pub fn set_external_activity_id(&mut self, external_activity_id: impl Into<String>) -> Result<()> {
        if self.is_terminal() {
            return Err(Error::from("终态审批步骤不得绑定外部活动身份"));
        }
        let external_activity_id = normalize_required_text(
            external_activity_id.into(),
            "外部审批活动身份不能为空",
            EXTERNAL_ACTIVITY_ID_MAX_LEN,
            "外部审批活动身份过长",
        )?;
        match self.external_activity_id.as_deref() {
            Some(existing) if existing == external_activity_id => Ok(()),
            Some(_) => Err(Error::from("审批步骤的外部活动身份已冻结，不得改写")),
            None => {
                self.external_activity_id = Some(external_activity_id);
                Ok(())
            }
        }
    }

    /// 从活动步骤形成正式决定。
    ///
    /// 本方法写入步骤决定和决定审计。决定是否在冻结步骤的 `allowed_decisions` 中，
    /// 以及待办完成、领域决定、下一步骤或实例终结，均由 Service 在同一事务处理。
    ///
    /// # 参数
    /// * `decision` - 固定审批决定
    /// * `decision_reason` - 决定原因；驳回申请人时必填
    /// * `decided_by` - 当前待办责任人
    /// * `at` - 决定时间
    ///
    /// # 错误
    /// 步骤不是 `ACTIVE`、决定审计非法，或驳回未提供原因时返回错误。
    pub fn decide(
        &mut self,
        decision: ApprovalDecision,
        decision_reason: Option<String>,
        decided_by: impl Into<String>,
        at: Instant,
    ) -> Result<()> {
        if self.status != ApprovalStepStatus::Active {
            return Err(self.transition_error(Self::decision_status(decision)));
        }
        let decision_reason =
            normalize_optional_text(decision_reason, "审批决定原因", DECISION_REASON_MAX_LEN)?;
        if decision == ApprovalDecision::RejectToApplicant && decision_reason.is_none() {
            return Err(Error::from("驳回申请人必须填写原因"));
        }
        let decided_by = normalize_required_text(
            decided_by.into(),
            "审批决定人不能为空",
            USER_ID_MAX_LEN,
            "审批决定人过长",
        )?;

        self.status = Self::decision_status(decision);
        self.decision = Some(decision);
        self.decision_reason = decision_reason;
        self.decided_by = Some(decided_by);
        self.decided_at = Some(at);
        self.blocker_code = None;
        self.blocked_at = None;
        Ok(())
    }

    /// 将当前或待激活步骤标记为结构化阻塞。
    ///
    /// `ACTIVE` 可在推进中阻塞；`WAITING` 可在解析下一步骤责任人失败时直接阻塞。
    /// 两种状态恢复后都回到原步骤的 `ACTIVE`，不得跳步骤或代替审批人形成决定。
    ///
    /// # 参数
    /// * `blocker_code` - 服务端注册的结构化阻塞原因
    /// * `at` - 进入阻塞状态的时间
    ///
    /// # 错误
    /// 步骤不是 `WAITING`/`ACTIVE`，或阻塞原因非法时返回错误。
    pub fn block(&mut self, blocker_code: impl Into<String>, at: Instant) -> Result<()> {
        if !matches!(
            self.status,
            ApprovalStepStatus::Waiting | ApprovalStepStatus::Active
        ) {
            return Err(self.transition_error(ApprovalStepStatus::Blocked));
        }
        let blocker_code = normalize_required_text(
            blocker_code.into(),
            "审批步骤阻塞原因不能为空",
            BLOCKER_CODE_MAX_LEN,
            "审批步骤阻塞原因过长",
        )?;
        self.status = ApprovalStepStatus::Blocked;
        self.blocker_code = Some(blocker_code);
        self.blocked_at = Some(at);
        Ok(())
    }

    /// 恢复原阻塞步骤为活动步骤。
    ///
    /// 本方法只执行 `RETRY_CURRENT_STEP` 的步骤状态变化并清除阻塞字段。
    ///
    /// # 错误
    /// 步骤不是 `BLOCKED` 时返回非法状态错误。
    pub fn recover(&mut self) -> Result<()> {
        if self.status != ApprovalStepStatus::Blocked {
            return Err(self.transition_error(ApprovalStepStatus::Active));
        }
        self.status = ApprovalStepStatus::Active;
        self.blocker_code = None;
        self.blocked_at = None;
        Ok(())
    }

    /// 取消尚未终结的步骤。
    ///
    /// 撤回审批时，当前步骤与所有未执行步骤均可取消；已形成正式决定的步骤保持不可变。
    ///
    /// # 错误
    /// 步骤已形成决定或已取消时返回非法状态错误。
    pub fn cancel(&mut self) -> Result<()> {
        if !matches!(
            self.status,
            ApprovalStepStatus::Waiting | ApprovalStepStatus::Active | ApprovalStepStatus::Blocked
        ) {
            return Err(self.transition_error(ApprovalStepStatus::Cancelled));
        }
        self.status = ApprovalStepStatus::Cancelled;
        self.blocker_code = None;
        self.blocked_at = None;
        Ok(())
    }

    /// 判断步骤是否为实例当前运行位置。
    ///
    /// # 返回
    /// 状态为 `ACTIVE` 或 `BLOCKED` 时返回 `true`。
    pub fn is_current(&self) -> bool {
        self.status.is_current()
    }

    /// 判断步骤是否已处于不可逆终态。
    ///
    /// # 返回
    /// 状态为通过、驳回、终止或取消时返回 `true`。
    pub fn is_terminal(&self) -> bool {
        self.status.is_terminal()
    }

    fn decision_status(decision: ApprovalDecision) -> ApprovalStepStatus {
        match decision {
            ApprovalDecision::Approve => ApprovalStepStatus::Approved,
            ApprovalDecision::RejectToApplicant => ApprovalStepStatus::Rejected,
            ApprovalDecision::TerminateApproval => ApprovalStepStatus::Terminated,
        }
    }

    fn transition_error(&self, target: ApprovalStepStatus) -> Error {
        Error::InvalidStateTransition {
            from: format!("{:?}", self.status),
            to: format!("{target:?}"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ApprovalStepInstance, ApprovalStepInstanceData};
    use crate::approval::{ApprovalDecision, ApprovalStepStatus};
    use crate::common::time::Instant;
    use crate::ids::{ApprovalInstanceId, ApprovalStepInstanceId};

    fn data(initial_status: ApprovalStepStatus) -> ApprovalStepInstanceData {
        ApprovalStepInstanceData {
            approval_instance_id: ApprovalInstanceId::new("instance-1"),
            step_key: " SALES_MANAGER ".to_string(),
            sequence_no: 1,
            initial_status,
            external_activity_id: None,
        }
    }

    #[test]
    fn new_step_only_accepts_waiting_or_active() {
        let active = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Active),
        )
        .unwrap();
        assert_eq!(active.step_key, "SALES_MANAGER");
        assert_eq!(active.step_version(), active.base.version);
        assert!(active.is_current());

        assert!(ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-2"),
            data(ApprovalStepStatus::Blocked),
        )
        .is_err());
    }

    #[test]
    fn waiting_step_activates_before_decision() {
        let mut step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Waiting),
        )
        .unwrap();
        assert!(step
            .decide(
                ApprovalDecision::Approve,
                None,
                "manager-1",
                Instant::from_unix_secs(1_700_000_100),
            )
            .is_err());
        step.activate().unwrap();
        step.decide(
            ApprovalDecision::Approve,
            Some(" 同意 ".to_string()),
            " manager-1 ",
            Instant::from_unix_secs(1_700_000_100),
        )
        .unwrap();
        assert_eq!(step.status, ApprovalStepStatus::Approved);
        assert_eq!(step.decision, Some(ApprovalDecision::Approve));
        assert_eq!(step.decision_reason.as_deref(), Some("同意"));
        assert_eq!(step.decided_by.as_deref(), Some("manager-1"));
    }

    #[test]
    fn rejection_requires_reason_and_preserves_active_state_on_failure() {
        let mut step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Active),
        )
        .unwrap();
        assert!(step
            .decide(
                ApprovalDecision::RejectToApplicant,
                Some("   ".to_string()),
                "manager-1",
                Instant::from_unix_secs(1_700_000_100),
            )
            .is_err());
        assert_eq!(step.status, ApprovalStepStatus::Active);
        assert!(step.decision.is_none());
    }

    #[test]
    fn block_and_recover_clear_only_block_fields() {
        let mut step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Waiting),
        )
        .unwrap();
        step.block(" ASSIGNEE_NOT_FOUND ", Instant::from_unix_secs(1_700_000_100))
            .unwrap();
        assert_eq!(step.status, ApprovalStepStatus::Blocked);
        assert_eq!(step.blocker_code.as_deref(), Some("ASSIGNEE_NOT_FOUND"));
        step.recover().unwrap();
        assert_eq!(step.status, ApprovalStepStatus::Active);
        assert!(step.blocker_code.is_none());
        assert!(step.blocked_at.is_none());
    }

    #[test]
    fn cancelled_or_decided_step_cannot_reopen() {
        let mut cancelled = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Waiting),
        )
        .unwrap();
        cancelled.cancel().unwrap();
        assert!(cancelled.activate().is_err());

        let mut decided = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-2"),
            data(ApprovalStepStatus::Active),
        )
        .unwrap();
        decided
            .decide(
                ApprovalDecision::TerminateApproval,
                None,
                "manager-1",
                Instant::from_unix_secs(1_700_000_100),
            )
            .unwrap();
        assert!(decided.cancel().is_err());
        assert!(decided.is_terminal());
    }

    #[test]
    fn external_activity_identity_is_idempotent_but_immutable() {
        let mut step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Waiting),
        )
        .unwrap();
        step.set_external_activity_id("activity-1").unwrap();
        step.set_external_activity_id("activity-1").unwrap();
        assert!(step.set_external_activity_id("activity-2").is_err());
    }

    #[test]
    fn entity_roundtrips_through_bson() {
        let step = ApprovalStepInstance::new(
            ApprovalStepInstanceId::new("step-instance-1"),
            data(ApprovalStepStatus::Active),
        )
        .unwrap();
        let roundtrip: ApprovalStepInstance = bson::from_document(bson::to_document(&step).unwrap()).unwrap();
        assert_eq!(roundtrip, step);
    }
}
