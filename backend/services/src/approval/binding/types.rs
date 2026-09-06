use bpm::ids::ApprovalCommandReceiptId;
use entities::document_registry::{DocumentType, WorkflowActionId};
use erp_core::common::time::Instant;
use serde::{Deserialize, Serialize};

use super::super::business_adapter::BindingRevalidationContext;
use super::super::execution::PreparedCommandIdentity;
use super::super::policy::ProcessRequiredApprovalPolicy;
use super::super::upgrade_subject::ApprovalUpgradeSubjectFacts;

/// 创建时绑定命令。客户端不得提交定义 ID。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BindPublishedDefinitionCommand {
    /// 单据类型。
    pub document_type: DocumentType,
    /// 业务对象主键。
    pub business_object_id: String,
    /// 业务对象乐观锁版本。
    pub business_object_version: u64,
    /// 当前单据组织与创建人。
    pub context: BindingRevalidationContext,
}

/// 未提交单据升级命令。目标固定为当前发布版本。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct UpgradeUnsubmittedDefinitionCommand {
    /// 路由与强实体必须共同证明的精确单据类型。
    pub document_type: DocumentType,
    /// 强业务对象与注册行共用的精确 ID。
    pub document_id: String,
    /// 客户端签署的强业务对象版本。
    pub expected_business_object_version: u64,
    /// 期望的绑定 CAS 版本。
    pub expected_binding_version: u64,
    /// 升级原因；命令身份与不可变动作均使用其 trim 后值。
    pub reason: String,
    /// 运行层在任何仓储访问前预构造的精确 V3 命令身份。
    pub identity: PreparedCommandIdentity,
    /// Fresh 分支预生成的不可变动作 ID。
    pub action_id: WorkflowActionId,
    /// Fresh 分支预生成的命令收据 ID。
    pub receipt_id: ApprovalCommandReceiptId,
}

/// 绑定升级结果类别。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum UpgradeBindingOutcome {
    /// 本次事务应用了新绑定。
    Applied,
    /// 同载荷收据经当前授权后回读原动作。
    Replay,
}

/// 绑定升级返回的新定义绑定。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeBindingView {
    /// 审批定义 ID。
    pub approval_process_definition_id: String,
    /// 定义业务版本。
    pub approval_definition_version: u32,
    /// 绑定 CAS 版本；字符串形态避免 JavaScript 精度丢失。
    pub approval_binding_version: String,
    /// 绑定发生时间。
    pub approval_definition_bound_at: Instant,
}

/// 从不可变 `WorkflowAction` 投影的绑定升级结果。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct UpgradeBindingResultView {
    /// 精确单据类型。
    pub document_type: DocumentType,
    /// 精确强业务对象 ID。
    pub document_id: String,
    /// 原命令签署的强业务对象版本。
    pub original_business_object_version: String,
    /// 升级后绑定；不从当前可变注册行伪造。
    pub new_binding: UpgradeBindingView,
    /// 收据 `result_ref` 指向的不可变动作 ID。
    pub action_id: String,
    /// 本次是应用还是授权回读。
    pub outcome: UpgradeBindingOutcome,
}

/// 绑定政策决定。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BindingDecision {
    /// 无审批：不查询定义、不写绑定。
    SkipNoApproval,
    /// 必须审批：查询唯一 `PUBLISHED` 定义。
    RequirePublished,
}

/// 单个实际授权角色的范围事实。
#[derive(Debug)]
pub(super) struct RoleScopeFacts(pub(super) Vec<entities::access_control::DataScope>);

/// 同一事务快照内已重验的升级上下文。
pub(super) struct AuthorizedUpgradeContext {
    pub(super) facts: ApprovalUpgradeSubjectFacts,
    pub(super) policy: ProcessRequiredApprovalPolicy,
    pub(super) actor_role: String,
}
