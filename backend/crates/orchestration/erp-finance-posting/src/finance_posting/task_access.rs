//! 财务办理在调用方快照内核对精确任务与固定动作。

use std::collections::BTreeMap;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::WorkItemId;
use erp_identity::entity::authorization::{AccessContext, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{
    AccessFactsPort, AccessTarget, Error as IdentityError, ResourceTargets, Result as IdentityResult,
};
use erp_permissions::{Permission, payable, purchase_order, purchase_payment_request, receivable};
use erp_workflow::service::work_item::authorization_facts;
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

use crate::{Error, FinancePostingPorts, Result};

/// 固定动作不接受客户端提供的权限或政策。
pub(crate) enum TaskPurpose {
    Payment,
    Reveal,
    Invoice,
}

impl TaskPurpose {
    fn permission(&self) -> Permission {
        match self {
            Self::Payment => purchase_payment_request::CREDIT_LIST,
            Self::Reveal => payable::PAYMENT_RECIPIENT_REVEAL,
            Self::Invoice => receivable::INVOICE_POST,
        }
    }

    fn target(&self, actor: &AuditActor, task: &WorkItem) -> Result<AccessTarget> {
        Ok(match self {
            Self::Reveal => {
                if task.owner_user_id.as_deref() != Some(actor.id()) {
                    return Err(Error::Forbidden("只有当前任务责任人可以揭示收款账号".into()));
                }
                AccessTarget::Operation
            },
            Self::Payment | Self::Invoice => {
                AccessTarget::Task { id: task.base.id.clone(), version: task.base.version.to_string() }
            },
        })
    }
}

/// 在当前命令事务内核验真实任务的当前版本，允许已提交命令沿原任务重放。
/// # 参数
/// db 为业务数据库，ports 为共享授权设施，actor 为当前认证操作人，
/// task_id 为原任务主键，purpose 为固定动作，executor 为原命令事务执行器。
/// # 返回
/// 当前账号仍具备同一任务办理资格时成功。
/// # 错误
/// 任务缺失时返回 `NotFound`；资格、任务版本或指派失效时拒绝。
/// 授权及仓储读取错误按原分类传播。
pub(crate) async fn require_entry(
    db: &Database,
    ports: &FinancePostingPorts,
    actor: &AuditActor,
    task_id: &WorkItemId,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    let task = db
        .work_items()
        .find_by_id(task_id, executor)
        .await?
        .ok_or_else(|| Error::NotFound("财务办理任务不存在".into()))?;
    require_task(ports, actor, &task, purpose, executor).await
}

/// 在原事务检查固定动作；付款和开票同时核对精确任务。
/// # 参数
/// ports 为共享授权能力；task 为同执行器已读任务，purpose 为本次固定动作。
/// # 返回
/// 揭示具备当前功能资格且为任务责任人时成功；付款和开票还须满足任务版本与实际指派。
/// # 错误
/// 资格、任务版本、指派或事实读取失败时拒绝。
pub(crate) async fn require_task(
    ports: &FinancePostingPorts,
    actor: &AuditActor,
    task: &WorkItem,
    purpose: TaskPurpose,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(ports
        .authorization
        .require(actor, &purpose.permission(), purpose.target(actor, task)?, executor)
        .await?)
}

/// 提供财务办理动作的真实任务事实；由组合根复用工作项读取器。
/// # 参数
/// db 为工作项所在数据库；financial_facts 复用应收子账、付款申请及采购单的完整事实读取器。
/// # 返回
/// 只接受固定财务任务权限的事实读取器；保留真实任务身份和版本。
/// # 错误
/// 缺少应收子账、付款申请或采购单读取器时立即拒绝；构造不执行 I/O。
pub fn authorization_task_facts(
    db: Database,
    financial_facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
) -> IdentityResult<Arc<dyn AccessFactsPort>> {
    for resource in ["receivable_account", "purchase_payment_request", "purchase_order"] {
        if !financial_facts.contains_key(resource) {
            return Err(IdentityError::ValidationError(format!("缺少财务任务来源 Facts：{resource}")));
        }
    }
    Ok(Arc::new(FinancialTaskFacts {
        tasks: authorization_facts(db.clone()),
        sources: Arc::new(MongoTaskSources { db, financial_facts }),
    }))
}

struct FinancialTaskFacts {
    tasks: Arc<dyn AccessFactsPort>,
    sources: Arc<dyn TaskSources>,
}

#[async_trait]
impl AccessFactsPort for FinancialTaskFacts {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        if ![
            purchase_payment_request::CREDIT_LIST,
            purchase_payment_request::SUBMIT,
            payable::PAYMENT_RECIPIENT_REVEAL,
            receivable::INVOICE_POST,
        ]
        .contains(&context.permission)
            || context.resource != context.permission.resource()
        {
            return Err(IdentityError::ValidationError(
                "当前待办不支持此项财务操作，请从对应待办办理".into(),
            ));
        }

        let task_context = AccessContext { resource: "work_item".into(), ..context.clone() };
        let mut facts = self.tasks.load(&task_context, targets, executor).await?;
        if facts.iter().any(|fact| fact.resource != "work_item") {
            return Err(IdentityError::ValidationError("当前待办与所办财务事项不符，请重新选择待办".into()));
        }

        let mut sources = self.sources.load(context, targets, executor).await?;
        for object in &mut facts {
            object.resource = context.resource.clone();
            object.relations = sources.remove(&object.id).ok_or_else(|| {
                IdentityError::ValidationError("找不到财务待办关联的业务单据，请联系管理员核对".into())
            })?;
        }
        Ok(facts)
    }
}

/// 只读任务实际主题并读取该主题的完整财务来源，不接受调用方来源集合。
#[async_trait]
trait TaskSources: Send + Sync {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<BTreeMap<String, BTreeMap<RelationKey, RelatedObjects>>>;
}
struct MongoTaskSources {
    db: Database,
    financial_facts: BTreeMap<String, Arc<dyn AccessFactsPort>>,
}
#[async_trait]
impl TaskSources for MongoTaskSources {
    async fn load(
        &self,
        context: &AccessContext,
        targets: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<BTreeMap<String, BTreeMap<RelationKey, RelatedObjects>>> {
        let mut sources = BTreeMap::new();
        for id in targets.ids() {
            let task = self
                .db
                .work_items()
                .find_by_id(id, executor)
                .await?
                .ok_or_else(|| IdentityError::NotFound("财务办理任务不存在".into()))?;

            let (resource, permission) = task_subject(&task, &context.permission)?;
            let source_context = AccessContext {
                resource: resource.into(),
                permission: permission.clone(),
                ..context.clone()
            };
            let related = source_relations(&self.financial_facts, &task, &source_context, executor).await?;
            sources.insert(id.clone(), related);
        }
        Ok(sources)
    }
}

/// 实际采购安排任务直接引用采购单；付款、审批揭示和开票任务展开真实资金根的完整来源。
async fn source_relations(
    financial_facts: &BTreeMap<String, Arc<dyn AccessFactsPort>>,
    task: &WorkItem,
    context: &AccessContext,
    executor: &mut dyn Executor,
) -> IdentityResult<BTreeMap<RelationKey, RelatedObjects>> {
    let port = financial_facts.get(&context.resource).ok_or_else(|| {
        IdentityError::ValidationError("无法核对财务待办关联的单据，请联系管理员检查待办设置".into())
    })?;
    let ids = ResourceTargets::new(&[&task.business_object_id])?;
    let mut objects = port.load(context, &ids, executor).await?;
    if objects.len() != 1
        || objects[0].resource != context.resource
        || objects[0].id != task.business_object_id
    {
        return Err(IdentityError::ValidationError("财务任务的真实主题不存在或身份错配".into()));
    }

    if context.resource == "purchase_order" {
        return Ok([(
            RelationKey::parse("purchase_source")?,
            RelatedObjects { expected_ids: ids.ids().clone(), objects },
        )]
        .into());
    }
    let source = objects.pop().ok_or_else(|| IdentityError::NotFound("财务任务来源不存在".into()))?;
    Ok(source.relations)
}

/// 每个财务办理动作只接受其正式工作项种类和真实主题资源。
fn task_subject(task: &WorkItem, permission: &Permission) -> IdentityResult<(&'static str, Permission)> {
    let (resource, source_permission, kind) = if permission == &receivable::INVOICE_POST {
        ("receivable_account", receivable::ACCOUNT_DETAIL, WorkItemType::SalesInvoiceExecution)
    } else if permission == &purchase_payment_request::SUBMIT {
        ("purchase_order", purchase_order::DETAIL, WorkItemType::PurchasePaymentArrangement)
    } else if permission == &payable::PAYMENT_RECIPIENT_REVEAL
        && task.work_item_type == WorkItemType::DocumentApproval
    {
        ("purchase_payment_request", purchase_payment_request::DETAIL, WorkItemType::DocumentApproval)
    } else {
        ("purchase_payment_request", purchase_payment_request::DETAIL, WorkItemType::SupplierPaymentExecution)
    };
    if task.work_item_type != kind || task.business_object_type != resource {
        return Err(IdentityError::ValidationError("财务任务类型或真实主题不匹配".into()));
    }
    Ok((resource, source_permission))
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use bpm::ApprovalNodeExecutionId;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_identity::entity::authorization::FactValue;
    use erp_permissions::sales_order;
    use erp_workflow::entity::work_item::{
        AssignmentSource, DocumentApprovalWorkItemData, WorkItemData, WorkItemPriority,
    };
    use persistence_core::{NoTransaction, ReadExecutor};

    use super::*;

    struct Tasks {
        objects: Vec<ObjectFacts>,
        contexts: Mutex<Vec<AccessContext>>,
    }

    #[async_trait]
    impl AccessFactsPort for Tasks {
        async fn load(
            &self,
            context: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            self.contexts.lock().unwrap().push(context.clone());
            Ok(self.objects.clone())
        }
    }

    fn context(permission: Permission) -> AccessContext {
        AccessContext {
            resource: permission.resource().into(),
            permission,
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            model: None,
            as_of: Instant::now(),
        }
    }

    struct MissingSources;
    #[async_trait]
    impl TaskSources for MissingSources {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<BTreeMap<String, BTreeMap<RelationKey, RelatedObjects>>> {
            Ok(BTreeMap::new())
        }
    }
    struct Sources;
    #[async_trait]
    impl TaskSources for Sources {
        async fn load(
            &self,
            _: &AccessContext,
            targets: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<BTreeMap<String, BTreeMap<RelationKey, RelatedObjects>>> {
            Ok(targets
                .ids()
                .iter()
                .map(|id| {
                    (
                        id.clone(),
                        BTreeMap::from([(
                            RelationKey::parse("sales_source").unwrap(),
                            RelatedObjects {
                                expected_ids: ["actual-sale".into()].into(),
                                objects: vec![ObjectFacts {
                                    resource: "sales_order".into(),
                                    id: "actual-sale".into(),
                                    version: "source-v8".into(),
                                    owner_id: FactValue::Known(Some("sales-owner".into())),
                                    ..Default::default()
                                }],
                            },
                        )]),
                    )
                })
                .collect())
        }
    }

    fn tasks() -> Arc<Tasks> {
        Arc::new(Tasks {
            objects: vec![ObjectFacts {
                resource: "work_item".into(),
                id: "actual-task".into(),
                version: "17".into(),
                owner_id: FactValue::Known(Some("assignee".into())),
                assignee_id: FactValue::Known(Some("assignee".into())),
                ..Default::default()
            }],
            contexts: Mutex::new(Vec::new()),
        })
    }

    #[tokio::test]
    async fn financial_task_facts_preserve_real_task_identity_version_and_assignment() {
        for permission in [
            purchase_payment_request::CREDIT_LIST,
            purchase_payment_request::SUBMIT,
            payable::PAYMENT_RECIPIENT_REVEAL,
            receivable::INVOICE_POST,
        ] {
            let tasks = tasks();
            let facts = FinancialTaskFacts { tasks: tasks.clone(), sources: Arc::new(Sources) };
            let context = context(permission.clone());

            let loaded = facts
                .load(&context, &ResourceTargets::new(&["actual-task"]).unwrap(), &mut NoTransaction)
                .await
                .unwrap();
            assert_eq!(loaded.len(), 1);
            assert_eq!(loaded[0].resource, permission.resource());
            assert_eq!(loaded[0].id, "actual-task");
            assert_eq!(loaded[0].version, "17");
            assert_eq!(loaded[0].assignee_id, FactValue::Known(Some("assignee".into())));
            assert_eq!(loaded[0].owner_id, loaded[0].assignee_id);

            let source = &loaded[0].relations[&RelationKey::parse("sales_source").unwrap()];
            assert_eq!(source.expected_ids, ["actual-sale".into()].into());
            assert_eq!(source.objects[0].id, "actual-sale");
            assert_eq!(source.objects[0].version, "source-v8");
            assert_eq!(source.objects[0].owner_id, FactValue::Known(Some("sales-owner".into())));
            assert_ne!(source.objects[0].owner_id, loaded[0].owner_id);

            let contexts = tasks.contexts.lock().unwrap();
            assert_eq!(contexts.len(), 1);
            assert_eq!(contexts[0].resource, "work_item");
            assert_eq!(contexts[0].permission, permission);
            assert_eq!(contexts[0].actor, context.actor);
            assert_eq!(contexts[0].as_of, context.as_of);
        }
    }

    #[tokio::test]
    async fn financial_task_facts_reject_permission_mismatch_without_fabricating_missing_targets() {
        let tasks = tasks();
        let facts = FinancialTaskFacts { tasks: tasks.clone(), sources: Arc::new(Sources) };
        let targets = ResourceTargets::new(&["actual-task"]).unwrap();
        for context in [
            context(sales_order::DETAIL),
            AccessContext { resource: "invoice".into(), ..context(purchase_payment_request::CREDIT_LIST) },
        ] {
            assert!(matches!(
                facts.load(&context, &targets, &mut NoTransaction).await,
                Err(IdentityError::ValidationError(_))
            ));
        }
        assert!(tasks.contexts.lock().unwrap().is_empty());

        let incomplete = facts
            .load(
                &context(purchase_payment_request::CREDIT_LIST),
                &ResourceTargets::new(&["actual-task", "missing"]).unwrap(),
                &mut NoTransaction,
            )
            .await
            .unwrap();
        assert_eq!(incomplete.len(), 1);
        assert_eq!(incomplete[0].id, "actual-task");
    }
    #[tokio::test]
    async fn financial_task_requires_the_actual_business_subject_sources() {
        let facts = FinancialTaskFacts { tasks: tasks(), sources: Arc::new(MissingSources) };
        let ids = ResourceTargets::new(&["actual-task"]).unwrap();
        for permission in [
            purchase_payment_request::CREDIT_LIST,
            receivable::INVOICE_POST,
            payable::PAYMENT_RECIPIENT_REVEAL,
        ] {
            assert!(matches!(
                facts.load(&context(permission), &ids, &mut NoTransaction).await,
                Err(IdentityError::ValidationError(_))
            ));
        }
    }

    fn approval_task() -> WorkItem {
        WorkItem::new_document_approval(
            WorkItemId::new("approval-task"),
            DocumentApprovalWorkItemData {
                approval_node_execution_id: ApprovalNodeExecutionId::new("actual-execution"),
                business_object_type: "purchase_payment_request".into(),
                business_object_id: "actual-request".into(),
                subject_version: "3".into(),
                owner_role: "finance".into(),
                owner_organization_id: "org".into(),
                owner_user_id: "approver".into(),
                priority: WorkItemPriority::Normal,
                due_at: None,
            },
            Instant::from_unix_secs(10),
        )
        .unwrap()
    }

    #[test]
    fn reveal_uses_function_qualification_and_other_financial_actions_keep_exact_task() {
        let mut task = approval_task();
        let actor = AuditActor::new("approver".into(), "approver".into(), AccountKind::Admin);

        assert!(matches!(TaskPurpose::Reveal.target(&actor, &task), Ok(AccessTarget::Operation)));
        for purpose in [TaskPurpose::Payment, TaskPurpose::Invoice] {
            assert!(matches!(
                purpose.target(&actor, &task),
                Ok(AccessTarget::Task { id, version })
                    if id == task.base.id && version == task.base.version.to_string()
            ));
        }

        for owner in [Some("other".into()), None] {
            task.owner_user_id = owner;
            assert!(matches!(TaskPurpose::Reveal.target(&actor, &task), Err(Error::Forbidden(_))));
        }
    }

    #[test]
    fn account_reveal_accepts_payment_request_approval_without_granting_payment_execution() {
        let mut task = approval_task();
        assert_eq!(
            task_subject(&task, &payable::PAYMENT_RECIPIENT_REVEAL).unwrap(),
            ("purchase_payment_request", purchase_payment_request::DETAIL),
        );

        for permission in [
            purchase_payment_request::CREDIT_LIST,
            purchase_payment_request::SUBMIT,
            receivable::INVOICE_POST,
        ] {
            assert!(matches!(task_subject(&task, &permission), Err(IdentityError::ValidationError(_))));
        }

        task.business_object_type = "invoice".into();
        assert!(matches!(
            task_subject(&task, &payable::PAYMENT_RECIPIENT_REVEAL),
            Err(IdentityError::ValidationError(_)),
        ));
    }

    struct SubjectFacts {
        objects: Vec<ObjectFacts>,
        calls: Mutex<Vec<(AccessContext, Vec<String>, usize)>>,
    }

    #[async_trait]
    impl AccessFactsPort for SubjectFacts {
        async fn load(
            &self,
            context: &AccessContext,
            targets: &ResourceTargets,
            executor: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            self.calls.lock().unwrap().push((
                context.clone(),
                targets.ids().iter().cloned().collect(),
                executor as *mut dyn Executor as *mut () as usize,
            ));
            Ok(self.objects.clone())
        }
    }

    #[tokio::test]
    async fn approval_reveal_reads_exact_real_request_sources_on_original_executor() {
        let task = approval_task();
        let (resource, permission) = task_subject(&task, &payable::PAYMENT_RECIPIENT_REVEAL).unwrap();
        let context = AccessContext { resource: resource.into(), ..context(permission) };

        let related = BTreeMap::from([(
            RelationKey::parse("purchase_source").unwrap(),
            RelatedObjects {
                expected_ids: ["actual-purchase".into()].into(),
                objects: vec![ObjectFacts {
                    resource: "purchase_order".into(),
                    id: "actual-purchase".into(),
                    version: "4".into(),
                    owner_id: FactValue::Known(Some("purchaser".into())),
                    ..Default::default()
                }],
            },
        )]);
        let root = ObjectFacts {
            resource: resource.into(),
            id: task.business_object_id.clone(),
            version: "9".into(),
            relations: related.clone(),
            ..Default::default()
        };

        let mut executor = ReadExecutor::default();
        let expected_executor = &mut executor as *mut ReadExecutor as usize;
        for objects in [
            vec![root.clone()],
            Vec::new(),
            vec![ObjectFacts { id: "other-request".into(), ..root.clone() }],
            vec![ObjectFacts { resource: "receivable_account".into(), ..root.clone() }],
            vec![root.clone(), root.clone()],
        ] {
            let valid = objects.len() == 1 && objects[0] == root;
            let facts = Arc::new(SubjectFacts { objects, calls: Mutex::new(Vec::new()) });
            let ports = BTreeMap::from([(resource.into(), facts.clone() as Arc<dyn AccessFactsPort>)]);

            let loaded = source_relations(&ports, &task, &context, &mut executor).await;
            if valid {
                assert_eq!(loaded.unwrap(), related);
            } else {
                assert!(matches!(loaded, Err(IdentityError::ValidationError(_))));
            }
            assert_eq!(
                *facts.calls.lock().unwrap(),
                vec![(context.clone(), vec!["actual-request".into()], expected_executor)],
            );
        }
    }

    #[test]
    fn financial_tasks_require_the_registered_type_and_real_business_subject() {
        let cases = [
            (receivable::INVOICE_POST, "receivable_account", WorkItemType::SalesInvoiceExecution),
            (purchase_payment_request::SUBMIT, "purchase_order", WorkItemType::PurchasePaymentArrangement),
            (
                purchase_payment_request::CREDIT_LIST,
                "purchase_payment_request",
                WorkItemType::SupplierPaymentExecution,
            ),
            (
                payable::PAYMENT_RECIPIENT_REVEAL,
                "purchase_payment_request",
                WorkItemType::SupplierPaymentExecution,
            ),
        ];
        for (permission, resource, kind) in cases {
            let mut task = WorkItem::new_with_responsibility_key(
                WorkItemId::new("task"),
                WorkItemData {
                    work_item_type: kind,
                    business_object_type: resource.into(),
                    business_object_id: "real-subject".into(),
                    subject_version: "1".into(),
                    owner_role: "finance".into(),
                    owner_organization_id: "org".into(),
                    owner_user_id: "actor".into(),
                    assignment_source: AssignmentSource::SystemRule,
                    priority: WorkItemPriority::Normal,
                    due_at: None,
                    reason_code: None,
                    impact_summary: None,
                },
                if kind == WorkItemType::PurchasePaymentArrangement {
                    "purchase_payment_arrangement:real-subject"
                } else {
                    "task-responsibility"
                },
            )
            .unwrap();

            assert_eq!(task_subject(&task, &permission).unwrap().0, resource);
            task.business_object_type = "unrelated".into();
            assert!(task_subject(&task, &permission).is_err());

            task.business_object_type = resource.into();
            task.work_item_type = WorkItemType::BusinessException;
            assert!(task_subject(&task, &permission).is_err());
        }
    }
}
