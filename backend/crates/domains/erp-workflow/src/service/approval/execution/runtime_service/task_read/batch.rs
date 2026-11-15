//! 同一执行器内批量装载已逐任务证明读取资格的审批运行事实。

use std::collections::{HashMap, HashSet};

use application_core::AuditActor;
use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use erp_identity::{AuthorizationService, SharedRbacService};
use erp_permissions::approval_instance;
use mongodb::Database;
use persistence_core::Executor;

use super::super::{ensure_exact_runtime_snapshot, runtime_document_type, runtime_snapshot_mismatch};
use super::{RuntimeReadSubject, manager_can_read, task_chain_matches_subject};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};
use crate::error::{Error, Result};
use crate::ports::WorkflowAuthorizationPort;
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;

/// 只在当前批次和调用方执行器中复用的运行事实。
struct RuntimeBatchFacts {
    executions: HashMap<String, ApprovalNodeExecution>,
    instances: HashMap<String, ApprovalProcessInstance>,
    snapshots: HashMap<String, ApprovalSubjectSnapshot>,
}

/// 同一批次只取得一次读者资格，逐任务读取责任事实，返回前复验身份版本。
///
/// # 参数
/// * `identity_service` - 同一应用注入的共享身份与权限服务。
/// * `db` / `auth` - 工作流仓储与当前权限端口
/// * `actor` - 同一已认证读者
/// * `items` - 服务端读取的当前批任务，调用方负责保持输入顺序
/// * `executor` - 当前查询或命令的同一执行器
///
/// # 返回
/// 返回与输入等长、同顺序的可读标志；空输入不读取任何事实。
///
/// # 错误
/// 保留逐任务权限、冻结主体、责任链及管理来源的原错误映射。
///
/// # 关键业务约束
/// 账号及同角色读写资格仅在本批次复用；返回前核对完整身份版本，不为拒绝项加载运行链。
/// 原单任务入口复用此入口，运行事实不得跨批次或独立授权快照缓存。
///
/// # Panics
/// 准入任务与主体结果必须等长且同序；`subjects.next()` 为 `None` 时 `expect`，表示本函数内部拼接被破坏。
pub async fn approval_tasks_readable_with_executor(
    authorization: &AuthorizationService,
    _identity_service: &SharedRbacService,
    db: &Database,
    _auth: &impl WorkflowAuthorizationPort,
    actor: &AuditActor,
    items: &[&WorkItem],
    executor: &mut dyn Executor,
) -> Result<Vec<bool>> {
    if !items.iter().any(|item| {
        item.work_item_type == WorkItemType::DocumentApproval
            && item.assignment_source == AssignmentSource::ApprovalRuntime
    }) {
        return Ok(vec![false; items.len()]);
    }
    let required = [approval_instance::COPIES_HISTORY.clone(), approval_instance::SUBMIT_DECISION.clone()];
    let Some(qualified) = authorization.permissions(actor, &required, executor).await? else {
        return Ok(vec![false; items.len()]);
    };
    if !qualified.permissions().covers_one(&required[0]) {
        return Ok(vec![false; items.len()]);
    }
    let result =
        read_tasks(authorization, db, actor, items, qualified.same_role(&required), executor).await?;
    let current = authorization.permissions(actor, &required, executor).await?;
    if current.as_ref().is_none_or(|current| current.version() != qualified.version()) {
        return Err(Error::ConflictError("DATA_SCOPE_CHANGED：审批读取资格已改变".into()));
    }
    Ok(result)
}

/// 在已证明的账号资格内批量读取责任链；逐任务错误按输入顺序返回。
async fn read_tasks(
    authorization: &AuthorizationService,
    db: &Database,
    actor: &AuditActor,
    items: &[&WorkItem],
    can_decide: bool,
    executor: &mut dyn Executor,
) -> Result<Vec<bool>> {
    let checks = items
        .iter()
        .map(|item| {
            item.work_item_type == WorkItemType::DocumentApproval
                && item.assignment_source == AssignmentSource::ApprovalRuntime
                && item.approval_node_execution_id.is_some()
        })
        .collect::<Vec<_>>();
    let candidates = eligible_items(items, &checks);
    let facts = load_runtime_facts(db, &candidates, executor).await?;
    let subjects = candidates.iter().map(|item| subject_for_task(item, &facts)).collect::<Vec<_>>();
    let open_tasks = load_open_tasks(db, &candidates, &subjects, executor).await?;
    let mut subjects = subjects.into_iter();
    let mut result = Vec::with_capacity(items.len());
    for (item, check) in items.iter().zip(checks) {
        if !check {
            result.push(false);
            continue;
        }
        let subject = subjects.next().expect("每个准入任务均有同顺序的运行主体结果")?;
        let Some(subject) = subject else {
            result.push(false);
            continue;
        };
        let tasks = item
            .approval_node_execution_id
            .as_ref()
            .and_then(|id| open_tasks.get(id.as_ref()))
            .map_or(&[][..], Vec::as_slice);
        let readable = if !task_chain_matches_subject(item, &subject, tasks)? {
            false
        } else if item.owner_user_id.as_deref() == Some(actor.id())
            && (item.status != WorkItemStatus::Open || can_decide)
        {
            true
        } else {
            manager_can_read(authorization, actor, &subject, executor).await?
        };
        result.push(readable);
    }
    Ok(result)
}

/// 只保留原权限阶段已经成功准入的任务引用。
///
/// # 参数
/// * `items` - 原任务顺序
/// * `checks` - 同顺序任务类型及执行引用准入结果
///
/// # 返回
/// 返回准入任务的有序子集，不克隆任务事实。
///
/// # 错误
/// 无；冻结主体和责任链错误由最终按原任务顺序传播。
fn eligible_items<'a>(items: &[&'a WorkItem], checks: &[bool]) -> Vec<&'a WorkItem> {
    items.iter().zip(checks).filter_map(|(item, check)| check.then_some(*item)).collect()
}

/// 按准入任务执行引用批量读取执行、存在实例和合法类型的冻结快照。
///
/// # 参数
/// * `db` - 工作流数据库
/// * `items` - 已证明静态读取资格的任务
/// * `executor` - 当前快照执行器
///
/// # 返回
/// 返回当前批次的运行事实，缺失执行或实例不补占位值。
///
/// # 错误
/// 仓储查询与反序列化失败时返回原仓储错误。
async fn load_runtime_facts(
    db: &Database,
    items: &[&WorkItem],
    executor: &mut dyn Executor,
) -> Result<RuntimeBatchFacts> {
    let execution_ids = execution_ids(items);
    let executions = db.bpm_workflow().list_executions_by_ids(&execution_ids, executor).await?;
    let mut seen = HashSet::new();
    let instance_ids = executions
        .iter()
        .filter(|execution| seen.insert(&execution.process_instance_id))
        .map(|execution| execution.process_instance_id.clone())
        .collect::<Vec<_>>();
    let instances = db.bpm_workflow().list_instances_by_ids(&instance_ids, executor).await?;
    let mut facts = RuntimeBatchFacts {
        executions: executions.into_iter().map(|execution| (execution.base.id.clone(), execution)).collect(),
        instances: instances.into_iter().map(|instance| (instance.base.id.clone(), instance)).collect(),
        snapshots: HashMap::new(),
    };
    let snapshot_ids = snapshot_instance_ids(items, &facts);
    facts.snapshots = db
        .approval_subject_snapshots()
        .find_by_process_instance_ids(&snapshot_ids, executor)
        .await?
        .into_iter()
        .map(|snapshot| (snapshot.approval_process_instance_id.to_string(), snapshot))
        .collect();
    Ok(facts)
}

/// 对任务持有的执行身份去重，保留首次出现顺序。
///
/// # 参数
/// * `items` - 已准入的任务集合
///
/// # 返回
/// 返回非空执行引用的去重集合。
///
/// # 错误
/// 无。
fn execution_ids(items: &[&WorkItem]) -> Vec<ApprovalNodeExecutionId> {
    let mut seen = HashSet::new();
    items
        .iter()
        .filter_map(|item| item.approval_node_execution_id.as_ref())
        .filter(|id| seen.insert(*id))
        .cloned()
        .collect()
}

/// 只为存在且主体类型有效的实例读取冻结快照。
///
/// # 参数
/// * `items` - 原顺序的准入任务
/// * `facts` - 已读取的执行与实例事实
///
/// # 返回
/// 返回去重实例 ID；遇首个主体类型错误后不再为后续任务加载快照。
///
/// # 错误
/// 类型错误由最终任务顺序的主体组装入口返回。
fn snapshot_instance_ids(items: &[&WorkItem], facts: &RuntimeBatchFacts) -> Vec<String> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for item in items {
        let Some(instance) = instance_for_task(item, facts) else {
            continue;
        };
        if runtime_document_type(instance, false).is_err() {
            break;
        }
        if seen.insert(instance.base.id.as_str()) {
            result.push(instance.base.id.clone());
        }
    }
    result
}

/// 从已加载身份链定位任务的流程实例，缺链保持不可读。
///
/// # 参数
/// * `item` - 当前任务事实
/// * `facts` - 当前批运行事实
///
/// # 返回
/// 返回任务执行精确引用的实例；任一身份缺失时返回 None。
///
/// # 错误
/// 无。
fn instance_for_task<'a>(
    item: &WorkItem,
    facts: &'a RuntimeBatchFacts,
) -> Option<&'a ApprovalProcessInstance> {
    let execution_id = item.approval_node_execution_id.as_ref()?;
    let execution = facts.executions.get(execution_id.as_ref())?;
    facts.instances.get(execution.process_instance_id.as_ref())
}

/// 对单任务沿用共同的类型与不可变冻结主体三元组校验。
///
/// # 参数
/// * `item` - 已准入的任务
/// * `facts` - 当前批次加载的运行事实
///
/// # 返回
/// 返回与原单任务入口相同的精确主体，缺失执行或实例时返回 None。
///
/// # 错误
/// 快照缺失或三元组损坏时返回原有冻结主体冲突。
fn subject_for_task(item: &WorkItem, facts: &RuntimeBatchFacts) -> Result<Option<RuntimeReadSubject>> {
    let Some(instance) = instance_for_task(item, facts) else {
        return Ok(None);
    };
    let document_type = runtime_document_type(instance, false)?;
    let snapshot =
        facts.snapshots.get(instance.base.id.as_str()).ok_or_else(|| runtime_snapshot_mismatch(false))?;
    ensure_exact_runtime_snapshot(instance, snapshot, document_type, false)?;
    let execution = item.approval_node_execution_id.as_ref().and_then(|id| facts.executions.get(id.as_ref()));
    Ok(Some(RuntimeReadSubject {
        instance: instance.clone(),
        current_execution: execution.cloned(),
        snapshot: snapshot.clone(),
        document_type,
    }))
}

/// 只为已校验冻结主体的开放任务批量加载完整开放责任链。
///
/// # 参数
/// * `db` - 工作流数据库
/// * `items` / `subjects` - 同顺序的准入任务及主体校验结果
/// * `executor` - 当前快照执行器
///
/// # 返回
/// 返回执行 ID 到全部开放任务的映射，重复开放任务完整保留。
///
/// # 错误
/// 仓储读取失败时返回原错误；主体或适配器错误留给最终有序判定。
async fn load_open_tasks(
    db: &Database,
    items: &[&WorkItem],
    subjects: &[Result<Option<RuntimeReadSubject>>],
    executor: &mut dyn Executor,
) -> Result<HashMap<String, Vec<WorkItem>>> {
    let execution_ids = open_execution_ids(items, subjects);
    let tasks = db.work_items().open_approval_tasks_for_executions(&execution_ids, executor).await?;
    let mut grouped = HashMap::<String, Vec<WorkItem>>::new();
    for task in tasks {
        if let Some(id) = &task.approval_node_execution_id {
            grouped.entry(id.to_string()).or_default().push(task);
        }
    }
    Ok(grouped)
}

/// 先按原顺序排除主体错误与终态项，再收集需要开放唯一链证明的执行。
///
/// # 参数
/// * `items` / `subjects` - 同顺序的准入任务与主体结果
///
/// # 返回
/// 返回首次主体或适配器错误之前的开放执行引用，去重但不改变任务判定顺序。
///
/// # 错误
/// 原错误由最终按任务顺序传播，不在本阶段提前返回。
fn open_execution_ids(
    items: &[&WorkItem],
    subjects: &[Result<Option<RuntimeReadSubject>>],
) -> Vec<ApprovalNodeExecutionId> {
    let mut result = Vec::new();
    let mut seen = HashSet::new();
    for (item, subject) in items.iter().zip(subjects) {
        let subject = match subject {
            Ok(Some(subject)) => subject,
            Ok(None) => continue,
            Err(_) => break,
        };
        if adapter_spec_of(subject.document_type).is_err() {
            break;
        }
        if item.status == WorkItemStatus::Open
            && let Some(id) = &item.approval_node_execution_id
            && seen.insert(id)
        {
            result.push(id.clone());
        }
    }
    result
}

#[cfg(test)]
mod tests {

    use super::super::tests::fixture;
    use super::*;
    use crate::entity::document_registry::DocumentType;

    /// 将同一条真实运行夹具拆成批量 loader 消费的身份映射。
    fn facts(mut subject: RuntimeReadSubject) -> RuntimeBatchFacts {
        let execution = subject.current_execution.take().unwrap();
        RuntimeBatchFacts {
            executions: HashMap::from([(execution.base.id.clone(), execution)]),
            instances: HashMap::from([(subject.instance.base.id.clone(), subject.instance)]),
            snapshots: HashMap::from([(
                subject.snapshot.approval_process_instance_id.to_string(),
                subject.snapshot,
            )]),
        }
    }

    /// 未准入任务不进入运行读取集合，准入任务保留原次序。
    #[test]
    fn approval_task_batch_candidates_preserve_eligible_order() {
        let (_, task) = fixture();
        let mut other = task.clone();
        other.base.id = "task-2".into();
        let mut failed = task.clone();
        failed.base.id = "task-3".into();
        let items = [&task, &other, &failed];
        let checks = [false, true, false];
        let selected = eligible_items(&items, &checks);
        assert_eq!(selected.len(), 1);
        assert_eq!(selected[0].base.id, "task-2");
        assert!(execution_ids(&[]).is_empty());
        assert_eq!(execution_ids(&[&task, &other]).len(), 1);
    }

    /// 缺失执行或实例保持不可读，不把缺关联改成冻结主体冲突。
    #[test]
    fn approval_task_batch_subject_preserves_missing_execution_and_instance_denial() {
        let (subject, task) = fixture();
        let mut loaded = facts(subject);
        assert!(subject_for_task(&task, &loaded).unwrap().is_some());
        loaded.instances.clear();
        assert!(subject_for_task(&task, &loaded).unwrap().is_none());
        loaded.executions.clear();
        assert!(subject_for_task(&task, &loaded).unwrap().is_none());
    }

    /// 冻结快照缺失或类型、身份、版本错配保持原稳定冲突。
    #[test]
    fn approval_task_batch_subject_preserves_exact_frozen_triple_errors() {
        let changes: &[fn(&mut ApprovalSubjectSnapshot)] = &[
            |snapshot| snapshot.document_type = DocumentType::SalesOrder,
            |snapshot| snapshot.business_object_id = "wrong-object".into(),
            |snapshot| snapshot.subject_version += 1,
        ];
        for change in changes {
            let (subject, task) = fixture();
            let mut loaded = facts(subject);
            change(loaded.snapshots.get_mut("instance-1").unwrap());
            assert!(
                matches!(subject_for_task(&task, &loaded), Err(Error::ConflictError(message)) if message == "审批实例与冻结业务快照不一致")
            );
        }
        let (subject, task) = fixture();
        let mut loaded = facts(subject);
        loaded.snapshots.clear();
        assert!(
            matches!(subject_for_task(&task, &loaded), Err(Error::ConflictError(message)) if message == "审批实例与冻结业务快照不一致")
        );
    }

    /// 终态无需开放链读取，首个主体错误之后的任务也不加载开放链。
    #[test]
    fn approval_task_batch_open_ids_skip_terminal_and_post_error_tasks() {
        let (subject, task) = fixture();
        let mut terminal = task.clone();
        terminal.status = WorkItemStatus::Completed;
        let subjects = [Ok(Some(subject))];
        assert!(open_execution_ids(&[&terminal], &subjects).is_empty());
        let (subject, task) = fixture();
        let subjects = [Err(Error::ConflictError("bad-subject".into())), Ok(Some(subject))];
        assert!(open_execution_ids(&[&task, &task], &subjects).is_empty());
        let (subject, task) = fixture();
        let subjects = [Ok(Some(subject))];
        assert_eq!(open_execution_ids(&[&task], &subjects), [ApprovalNodeExecutionId::new("execution-1")]);
    }
}
