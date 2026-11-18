//! 工作项根与精确任务目标复用同一读取器，财务任务追加真实业务来源。
mod detail;
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
pub(super) use detail::TaskDetailMapping;
use erp_identity::entity::authorization::{AccessContext, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessFactsPort, Error, ResourceTargets, Result};
use erp_permissions::{
    Permission, internal, payable, purchase_order, purchase_payment_request, receivable, sales_order,
    work_item,
};
use erp_read_models::Error as ReadError;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_workflow::ports::{ObjectFact, ObjectKind, OrderTaskSource};
use erp_workflow::repository::{BpmExt, WorkItemRepositoryExt};
use erp_workflow::{WorkItem, WorkItemExt, WorkItemType};
use mongodb::Database;
use persistence_core::Executor;

pub(super) struct TaskFacts {
    pub db: Database,
    pub items: Arc<dyn AccessFactsPort>,
    pub financial: Arc<dyn AccessFactsPort>,
    pub roots: BTreeMap<String, Arc<dyn AccessFactsPort>>,
}

#[async_trait]
impl AccessFactsPort for TaskFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ObjectFacts>> {
        if context.resource == "work_item" {
            let mut facts = self.items.load(context, ids, executor).await?;
            if [internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT, work_item::DETAIL]
                .contains(&context.permission)
            {
                self.execution_sources(context, &mut facts, executor).await?;
            }
            return Ok(facts);
        }
        if [
            purchase_payment_request::CREDIT_LIST,
            purchase_payment_request::SUBMIT,
            payable::PAYMENT_RECIPIENT_REVEAL,
            receivable::INVOICE_POST,
        ]
        .contains(&context.permission)
        {
            return self.financial.load(context, ids, executor).await;
        }
        for id in ids.ids() {
            let task = self
                .db
                .work_items()
                .find_work_item(id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("精确任务不存在".into()))?;
            ensure_subject(&task, &context.resource)?;
        }
        let mut facts = self.items.load(context, ids, executor).await?;
        for object in &mut facts {
            object.resource = context.resource.clone();
        }
        Ok(facts)
    }
}

impl TaskFacts {
    // 解析工作项执行授权依赖时从持久化任务及真实外键补齐来源事实，不读取或缓存任何账号的授权结论。
    async fn execution_sources(
        &self,
        context: &AccessContext,
        facts: &mut [ObjectFacts],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        for object in facts {
            let task = self
                .db
                .work_items()
                .find_work_item(&object.id, executor)
                .await?
                .ok_or_else(|| Error::NotFound("任务真实业务来源缺失".into()))?;
            if task.work_item_type.is_document_approval()
                && self.approval_source(context, &task, object, executor).await?
            {
                continue;
            }
            let relation = task
                .work_item_type
                .brief_relation(&task.business_object_type)
                .ok_or_else(|| Error::ValidationError("任务类型未登记真实业务关系".into()))?;
            if OrderTaskSource::required_for(relation.object_kind) {
                let key = (relation.object_kind, task.business_object_id.clone());
                let loaded = WorkItemFactsReader::new(self.db.clone())
                    .load(&HashSet::from([key.clone()]), executor)
                    .await
                    .map_err(read_error)?;
                let source = order_source(relation.object_kind, loaded.get(&key))?;
                let (resource, id, permission) = match source {
                    OrderTaskSource::Sales(id) => ("sales_order", id, sales_order::DETAIL),
                    OrderTaskSource::Purchase(id) => ("purchase_order", id, purchase_order::DETAIL),
                };
                let related = self.related(context, resource, &id, permission, executor).await?;
                object.relations.insert(RelationKey::parse("order_source")?, related);
            } else if relation.object_kind == ObjectKind::SupplierPortalRequest {
                let related = self
                    .related(
                        context,
                        "supplier_portal_material",
                        &task.business_object_id,
                        internal::WORK_ITEM_EXECUTE,
                        executor,
                    )
                    .await?;
                object.relations.insert(RelationKey::parse("portal_source")?, related);
            }
        }
        Ok(())
    }

    // 由审批任务实际关联的执行记录定位唯一实例；展示用业务根不能代替审批身份。
    async fn approval_source(
        &self,
        context: &AccessContext,
        task: &WorkItem,
        object: &mut ObjectFacts,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let execution_id = task
            .approval_node_execution_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("审批任务缺少实际执行引用".into()))?;
        let execution = self
            .db
            .approval_node_executions()
            .find_by_id(execution_id.as_ref(), executor)
            .await?
            .ok_or_else(|| Error::NotFound("审批任务实际执行不存在".into()))?;
        self.approval_instance_facts(context, object, execution.process_instance_id.as_ref(), executor).await
    }

    // 工作项详情只需审批实例来源；解析办理人或收件人资格时继续加载关联的原业务事实。
    async fn approval_instance_facts(
        &self,
        context: &AccessContext,
        object: &mut ObjectFacts,
        instance_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let related = self
            .related(context, "approval_instance", instance_id, internal::APPROVAL_STARTED_LIST, executor)
            .await?;
        object.relations.insert(RelationKey::parse("approval_instance")?, related);
        Ok(context.permission == work_item::DETAIL)
    }

    // 在调用者执行器上读取任务的真实来源及精确引用，交给身份授权层校验来源完整性。
    async fn related(
        &self,
        context: &AccessContext,
        resource: &str,
        id: &str,
        permission: Permission,
        executor: &mut dyn Executor,
    ) -> Result<RelatedObjects> {
        let reader = self
            .roots
            .get(resource)
            .ok_or_else(|| Error::ValidationError(format!("任务真实来源缺少事实绑定：{resource}")))?;
        let source = AccessContext { resource: resource.into(), permission, model: None, ..context.clone() };
        let ids = ResourceTargets::new(&[id])?;
        let objects = reader.load(&source, &ids, executor).await?;
        Ok(RelatedObjects { expected_ids: [id.to_owned()].into(), objects })
    }
}

// 从真实业务对象解析订单来源；展示根节点和任务业务主键都不能作为订单授权事实。
fn order_source(kind: ObjectKind, fact: Option<&ObjectFact>) -> Result<OrderTaskSource> {
    fact.and_then(|fact| fact.order_scope_source.as_ref())
        .filter(|source| source.matches_kind(kind))
        .cloned()
        .ok_or_else(|| Error::NotFound("订单任务缺少权威业务来源".into()))
}

// 把任务事实读取错误传回身份边界，区分冲突、基础设施故障与明确的授权拒绝。
fn read_error(error: ReadError) -> Error {
    match error {
        ReadError::Internal(value) => Error::Internal(value),
        ReadError::NotFound(value) => Error::NotFound(value),
        ReadError::ValidationError(value) => Error::ValidationError(value),
        ReadError::InvalidInput(value) => Error::InvalidInput(value),
        ReadError::BusinessLogicError(value) => Error::BusinessLogicError(value),
        ReadError::ConflictError(value) => Error::ConflictError(value),
        ReadError::ReceiptDuplicate(value) => Error::ReceiptDuplicate(value),
        ReadError::TransientTransaction(value) => Error::TransientTransaction(value),
        ReadError::Forbidden(value) => Error::Forbidden(value),
        ReadError::Unauthenticated(value) => Error::Unauthenticated(value),
        ReadError::Logic(value) => Error::Logic(value),
        ReadError::Rbac(value) => Error::Rbac(value),
        ReadError::OutcomeUnknown(value) => Error::OutcomeUnknown(value),
        ReadError::RepositoryError(value) => Error::RepositoryError(value),
        ReadError::Coded(value) => Error::BusinessLogicError(value.to_string()),
    }
}

fn ensure_subject(task: &WorkItem, resource: &str) -> Result<()> {
    let allowed = match resource {
        "purchase_order" => {
            task.work_item_type == WorkItemType::ProcurementOrderCreation
                && task.business_object_type == "sales_order"
        },
        "supplier_settlement_statement" => {
            task.work_item_type == WorkItemType::SupplierSettlementReview
                && task.business_object_type == "supplier_settlement_statement"
        },
        "customer_acceptance" => {
            task.work_item_type == WorkItemType::CustomerAcceptanceRegistration
                && task.business_object_type == "sales_order"
        },
        "delivery" | "purchase_receipt" | "electronic_delivery" | "service_fulfillment" => {
            task.work_item_type == WorkItemType::FulfillmentOperation && task.business_object_type == resource
        },
        "supplier_fulfillment_order" => {
            matches!(
                task.work_item_type,
                WorkItemType::BusinessException | WorkItemType::IntegrationResultUnknown
            ) && task.business_object_type == "SUPPLIER_FULFILLMENT_ORDER"
        },
        "supplier_offering" => {
            task.work_item_type == WorkItemType::BusinessException
                && task.business_object_type == "SUPPLIER_OFFERING"
        },
        "stock_adjustment" => {
            task.work_item_type == WorkItemType::InventoryAdjustmentReview
                && task.business_object_type == "stock_adjustment"
        },
        _ => false,
    };
    if allowed {
        Ok(())
    } else {
        Err(Error::ValidationError("任务类型或真实业务来源与本次权限不符".into()))
    }
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;
    use erp_identity::entity::authorization::FactValue;
    use erp_workflow::entity::work_item::SupplierPortalReviewTaskData;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

    #[test]
    fn settlement_task_requires_the_actual_statement_source_and_registered_kind() {
        let mut task = WorkItem::new_supplier_portal_review(
            WorkItemId::new("task"),
            SupplierPortalReviewTaskData {
                request_id: "statement".into(),
                subject_version: "1".into(),
                owner_user_id: "reviewer".into(),
                owner_organization_id: "finance".into(),
                due_at: None,
                impact_summary: None,
            },
        )
        .unwrap();
        task.work_item_type = WorkItemType::SupplierSettlementReview;
        task.business_object_type = "supplier_settlement_statement".into();
        assert!(ensure_subject(&task, "supplier_settlement_statement").is_ok());
        for source in ["purchase_order", "SUPPLIER_FULFILLMENT_ORDER", ""] {
            task.business_object_type = source.into();
            assert!(matches!(
                ensure_subject(&task, "supplier_settlement_statement"),
                Err(Error::ValidationError(_))
            ));
        }
        task.business_object_type = "supplier_settlement_statement".into();
        task.work_item_type = WorkItemType::SupplierPortalReview;
        assert!(matches!(
            ensure_subject(&task, "supplier_settlement_statement"),
            Err(Error::ValidationError(_))
        ));
    }

    #[test]
    fn order_sources_use_real_foreign_keys_instead_of_display_roots() {
        for (kind, source) in [
            (ObjectKind::SalesOrder, OrderTaskSource::Sales("sale".into())),
            (ObjectKind::SalesChangeOrder, OrderTaskSource::Sales("sale".into())),
            (ObjectKind::Delivery, OrderTaskSource::Sales("sale".into())),
            (ObjectKind::PurchaseOrder, OrderTaskSource::Purchase("purchase".into())),
            (ObjectKind::PurchaseChangeOrder, OrderTaskSource::Purchase("purchase".into())),
            (ObjectKind::PurchaseReceipt, OrderTaskSource::Purchase("purchase".into())),
            (ObjectKind::ElectronicDelivery, OrderTaskSource::Purchase("purchase".into())),
            (ObjectKind::ServiceFulfillment, OrderTaskSource::Purchase("purchase".into())),
        ] {
            let fact = ObjectFact::new("display-root", "title", "creator").with_order_source(source.clone());
            assert_eq!(order_source(kind, Some(&fact)).unwrap(), source);
        }
    }

    #[test]
    fn missing_empty_or_wrong_order_sources_fail_closed() {
        for fact in [
            None,
            Some(ObjectFact::new("display", "title", "creator")),
            Some(
                ObjectFact::new("display", "title", "creator")
                    .with_order_source(OrderTaskSource::Sales("".into())),
            ),
            Some(
                ObjectFact::new("display", "title", "creator")
                    .with_order_source(OrderTaskSource::Purchase("wrong-type".into())),
            ),
        ] {
            assert!(matches!(order_source(ObjectKind::Delivery, fact.as_ref()), Err(Error::NotFound(_))));
        }
    }

    struct SourceReader {
        fail: bool,
        calls: Mutex<Vec<(AccessContext, Vec<String>, usize)>>,
    }

    #[async_trait]
    impl AccessFactsPort for SourceReader {
        async fn load(
            &self,
            context: &AccessContext,
            ids: &ResourceTargets,
            executor: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            self.calls.lock().unwrap().push((
                context.clone(),
                ids.ids().iter().cloned().collect(),
                executor as *mut dyn Executor as *mut () as usize,
            ));
            if self.fail {
                return Err(Error::ConflictError("source-changed".into()));
            }
            Ok(vec![ObjectFacts {
                resource: context.resource.clone(),
                id: ids.ids().iter().next().unwrap().clone(),
                version: "7".into(),
                subject_type: FactValue::Known("offering_update".into()),
                ..Default::default()
            }])
        }
    }

    struct UnusedReader;

    #[async_trait]
    impl AccessFactsPort for UnusedReader {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> Result<Vec<ObjectFacts>> {
            panic!("真实来源装配不得递归读取任务或调用财务办理授权")
        }
    }

    #[tokio::test]
    async fn approval_task_sources_keep_instance_for_every_entry_and_business_sources_for_commands() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        let reader = Arc::new(SourceReader { fail: false, calls: Mutex::new(vec![]) });
        let adapter = TaskFacts {
            db: client.database("approval_task_source_unit"),
            items: Arc::new(UnusedReader),
            financial: Arc::new(UnusedReader),
            roots: [("approval_instance".into(), reader.clone() as Arc<dyn AccessFactsPort>)].into(),
        };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;
        for permission in [work_item::DETAIL, internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT] {
            let context = AccessContext {
                actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
                permission: permission.clone(),
                resource: "work_item".into(),
                model: None,
                as_of: Instant::from_unix_secs(100),
            };
            let order = RelatedObjects { expected_ids: ["order".into()].into(), objects: vec![] };
            let mut object = ObjectFacts {
                resource: "work_item".into(),
                id: "task".into(),
                subject_type: FactValue::Known("DOCUMENT_APPROVAL".into()),
                relations: [(RelationKey::parse("order_source").unwrap(), order.clone())].into(),
                ..Default::default()
            };
            let complete = adapter
                .approval_instance_facts(&context, &mut object, "actual-instance", &mut executor)
                .await
                .unwrap();
            assert_eq!(complete, permission == work_item::DETAIL);
            let instance = &object.relations[&RelationKey::parse("approval_instance").unwrap()];
            assert_eq!(instance.expected_ids, ["actual-instance".into()].into());
            assert_eq!(
                (instance.objects[0].resource.as_str(), instance.objects[0].id.as_str()),
                ("approval_instance", "actual-instance")
            );
            assert_eq!(object.relations[&RelationKey::parse("order_source").unwrap()], order);
            let (seen, ids, pointer) = reader.calls.lock().unwrap().last().cloned().unwrap();
            assert_eq!(seen.permission, internal::APPROVAL_STARTED_LIST);
            assert_eq!((seen.actor, ids, pointer), (context.actor, vec!["actual-instance".into()], expected));
        }
    }

    #[tokio::test]
    async fn related_facts_keep_exact_identity_permission_and_original_executor() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        let reader = Arc::new(SourceReader { fail: false, calls: Mutex::new(vec![]) });
        let adapter = TaskFacts {
            db: client.database("task_source_unit"),
            items: Arc::new(UnusedReader),
            financial: Arc::new(UnusedReader),
            roots: [
                ("sales_order".into(), reader.clone() as Arc<dyn AccessFactsPort>),
                ("supplier_portal_material".into(), reader.clone() as Arc<dyn AccessFactsPort>),
            ]
            .into(),
        };
        let context = AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: internal::WORK_ITEM_EXECUTE,
            resource: "work_item".into(),
            model: None,
            as_of: Instant::from_unix_secs(100),
        };
        let mut executor = SnapshotExecutor::default();
        let expected = &mut executor as *mut SnapshotExecutor as usize;
        for (resource, id, permission) in [
            ("sales_order", "actual-order", sales_order::DETAIL),
            ("supplier_portal_material", "actual-request", internal::WORK_ITEM_EXECUTE),
        ] {
            let related =
                adapter.related(&context, resource, id, permission.clone(), &mut executor).await.unwrap();
            assert_eq!(related.expected_ids, [id.to_owned()].into());
            assert_eq!(related.objects[0].id, id);
            assert_eq!(related.objects[0].resource, resource);
            assert_eq!(related.objects[0].subject_type.get().unwrap(), "offering_update");
            let (seen, ids, pointer) = reader.calls.lock().unwrap().last().cloned().unwrap();
            assert_eq!((seen.resource.as_str(), &seen.permission), (resource, &permission));
            assert_eq!(
                (seen.actor, seen.as_of, ids, pointer),
                (context.actor.clone(), context.as_of, vec![id.to_owned()], expected)
            );
        }
        assert!(matches!(
            adapter.related(&context, "missing", "id", sales_order::DETAIL, &mut executor).await,
            Err(Error::ValidationError(_))
        ));
    }

    #[tokio::test]
    async fn related_fact_failure_preserves_conflict_instead_of_empty_sources() {
        let client = Client::with_uri_str("mongodb://127.0.0.1:1").await.unwrap();
        let reader = Arc::new(SourceReader { fail: true, calls: Mutex::new(vec![]) });
        let adapter = TaskFacts {
            db: client.database("task_source_failure"),
            items: Arc::new(UnusedReader),
            financial: Arc::new(UnusedReader),
            roots: [("sales_order".into(), reader as Arc<dyn AccessFactsPort>)].into(),
        };
        let context = AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: internal::WORK_ITEM_EXECUTE,
            resource: "work_item".into(),
            model: None,
            as_of: Instant::from_unix_secs(100),
        };
        assert!(matches!(
            adapter.related(&context, "sales_order", "id", sales_order::DETAIL, &mut SnapshotExecutor::default()).await,
            Err(Error::ConflictError(message)) if message == "source-changed"
        ));
    }
}
