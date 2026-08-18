//! 启动第 1 轮运行实例并进入入口节点。

use crate::ids::{ApprovalInstanceAssigneeId, ApprovalNodeExecutionId, ApprovalProcessInstanceId};
use crate::model::types::ApprovalExecutionAssignmentSource;
use crate::model::{
    ApprovalInstanceAssignee, ApprovalProcessInstance, ParticipantId, ProcessKind, SubjectRef, Timestamp,
};

use super::enter_node::plan_enter_node;
use super::event::{BpmEvent, BpmEventKind};
use super::transition_plan::TransitionPlan;
use super::{DefinitionGraph, Eligibility, EngineError, EngineResult};

/// 启动命令：调用方提供全部 ID、时间与入口资格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartCommand {
    /// 实例主键。
    pub instance_id: ApprovalProcessInstanceId,
    /// 流程种类。
    pub process_kind: ProcessKind,
    /// 业务对象引用。
    pub subject: SubjectRef,
    /// 冻结提交版本。
    pub subject_version: u32,
    /// 启动人。
    pub started_by: ParticipantId,
    /// 入口执行主键。
    pub entry_execution_id: ApprovalNodeExecutionId,
    /// 调用方时间。
    pub now: Timestamp,
}

/// 启动时为定义节点冻结的实例审批人。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct StartAssigneeBinding {
    /// 绑定主键。
    pub id: ApprovalInstanceAssigneeId,
    /// 节点键。
    pub node_key: String,
    /// 定义审批人。
    pub participant: ParticipantId,
    /// 入口节点资格；非入口节点仅校验绑定完整。
    pub eligibility: Eligibility,
}

/// 创建 `RUNNING` 第 1 轮实例、全部节点审批人快照，并进入入口。
///
/// 任一人资格无效时不得创建实例或执行。入口进入必须是有效资格。
///
/// # 参数
/// * `command` - 启动命令
/// * `graph` - 定义图
/// * `bindings` - 与定义节点一一对应的审批人绑定
///
/// # 错误
/// 入口缺失、绑定不完整、任一人失效或模型不变式失败时返回错误。
pub fn start(
    command: StartCommand,
    graph: &DefinitionGraph,
    bindings: &[StartAssigneeBinding],
) -> EngineResult<TransitionPlan> {
    ensure_all_assignees_eligible(bindings)?;
    let instance = ApprovalProcessInstance::start_running(
        command.instance_id.clone(),
        graph.definition.base.id.clone().into_definition(),
        graph.definition.definition_version,
        command.process_kind,
        command.subject,
        command.subject_version,
        command.started_by.clone(),
        command.now,
    )?;
    let assignees = freeze_assignees(&instance, graph, bindings, command.now)?;
    let entry = graph.entry_node()?;
    let entry_binding = binding_for(bindings, &entry.node_key)?;
    let mut plan = plan_enter_node(
        instance,
        graph,
        &entry.node_key,
        1,
        entry_binding.participant.clone(),
        entry_binding.eligibility.clone(),
        command.entry_execution_id,
        1,
        ApprovalExecutionAssignmentSource::Definition,
        None,
        command.now,
    )?;
    plan.created_assignees = assignees;
    plan.events.insert(
        0,
        BpmEvent::new(BpmEventKind::InstanceStarted, command.instance_id, 1).with_actor(command.started_by),
    );
    Ok(plan)
}

/// 启动前全部节点审批人必须有效，否则不得形成运行链。
fn ensure_all_assignees_eligible(bindings: &[StartAssigneeBinding]) -> EngineResult<()> {
    if bindings
        .iter()
        .any(|item| item.eligibility.blocked_code().is_some())
    {
        return Err(EngineError::InvalidCommand(
            "启动时全部审批人必须有效，不得创建受阻实例",
        ));
    }
    Ok(())
}

/// 为定义中每个节点冻结实例审批人，缺节点或重复时失败关闭。
fn freeze_assignees(
    instance: &ApprovalProcessInstance,
    graph: &DefinitionGraph,
    bindings: &[StartAssigneeBinding],
    now: Timestamp,
) -> EngineResult<Vec<ApprovalInstanceAssignee>> {
    if bindings.len() != graph.nodes.len() {
        return Err(EngineError::InvalidCommand(
            "实例审批人绑定必须与定义节点一一对应",
        ));
    }
    let mut assignees = Vec::with_capacity(graph.nodes.len());
    for node in &graph.nodes {
        let binding = binding_for(bindings, &node.node_key)?;
        assignees.push(ApprovalInstanceAssignee::from_definition(
            binding.id.clone(),
            ApprovalProcessInstanceId::new(instance.base.id.clone()),
            node.node_key.clone(),
            binding.participant.clone(),
            now,
        )?);
    }
    Ok(assignees)
}

/// 按节点键查找启动绑定。
fn binding_for<'a>(
    bindings: &'a [StartAssigneeBinding],
    node_key: &str,
) -> EngineResult<&'a StartAssigneeBinding> {
    let matches: Vec<_> = bindings.iter().filter(|item| item.node_key == node_key).collect();
    match matches.as_slice() {
        [only] => Ok(*only),
        _ => Err(EngineError::InvalidCommand("节点审批人绑定缺失或重复")),
    }
}

trait IntoDefinitionId {
    fn into_definition(self) -> crate::ids::ApprovalProcessDefinitionId;
}

impl IntoDefinitionId for String {
    fn into_definition(self) -> crate::ids::ApprovalProcessDefinitionId {
        crate::ids::ApprovalProcessDefinitionId::new(self)
    }
}
