//! 同一执行器内批量装载已逐任务证明读取资格的审批运行事实。

use std::collections::{HashMap, HashSet};

use bpm::ids::ApprovalNodeExecutionId;
use bpm::model::{ApprovalNodeExecution, ApprovalProcessInstance};
use mongodb::Database;
use persistence_core::Executor;

use super::super::{ensure_exact_runtime_snapshot, runtime_document_type, runtime_snapshot_mismatch};
use super::{RuntimeReadSubject, task_chain_matches_subject};
use crate::entity::approval_integration::ApprovalSubjectSnapshot;
use crate::entity::work_item::{AssignmentSource, WorkItem, WorkItemStatus, WorkItemType};
use crate::error::Result;
use crate::repository::prelude::*;
use crate::repository::{ApprovalIntegrationExt, BpmExt, WorkItemExt};
use crate::service::approval::business_adapter::adapter_spec_of;

/// 只在当前批次和调用方执行器中复用的运行事实。
struct RuntimeBatchFacts {
    executions: HashMap<String, ApprovalNodeExecution>,
    instances: HashMap<String, ApprovalProcessInstance>,
    snapshots: HashMap<String, ApprovalSubjectSnapshot>,
}

/// 中央授权已通过后，只核对该审批任务的冻结运行主体和责任链。
/// # 参数
/// db 为工作流数据库，item 为同一快照已授权的任务，executor 为原读取执行器。
/// # 返回
/// 精确执行、实例、冻结快照及当前开放或终态责任全部一致时返回 true。
/// # 错误
/// 仓储、正式单据类型或冻结主体损坏时返回对应错误。
pub(crate) async fn approval_task_facts_current(
    db: &Database,
    item: &WorkItem,
    executor: &mut dyn Executor,
) -> Result<bool> {
    if item.work_item_type != WorkItemType::DocumentApproval
        || item.assignment_source != AssignmentSource::ApprovalRuntime
        || item.approval_node_execution_id.is_none()
    {
        return Ok(false);
    }

    let items = [item];
    let facts = load_runtime_facts(db, &items, executor).await?;
    let subjects = [subject_for_task(item, &facts)];
    let tasks = load_open_tasks(db, &items, &subjects, executor).await?;
    let [subject] = subjects;
    let Some(subject) = subject? else { return Ok(false) };
    let open = item
        .approval_node_execution_id
        .as_ref()
        .and_then(|id| tasks.get(id.as_ref()))
        .map_or(&[][..], Vec::as_slice);
    task_chain_matches_subject(item, &subject, open)
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
    use crate::error::Error;

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

    #[test]
    fn execution_references_are_empty_or_deduplicated() {
        let (_, task) = fixture();
        let mut other = task.clone();
        other.base.id = "task-2".into();
        assert!(execution_ids(&[]).is_empty());
        assert_eq!(execution_ids(&[&task, &other]), [ApprovalNodeExecutionId::new("execution-1")]);
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
