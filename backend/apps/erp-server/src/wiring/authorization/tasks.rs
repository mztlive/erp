//! 工作项根与精确任务目标复用同一读取器，财务任务追加真实业务来源。
use std::collections::{BTreeMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_identity::entity::authorization::{AccessContext, ObjectFacts, RelatedObjects, RelationKey};
use erp_identity::{AccessFactsPort, Error, ResourceTargets, Result};
use erp_permissions::{
    Permission, internal, payable, purchase_order, purchase_payment_request, receivable, sales_order,
};
use erp_read_models::Error as ReadError;
use erp_read_models::workbench::authority::WorkItemFactsReader;
use erp_workflow::ports::{ObjectFact, ObjectKind, OrderTaskSource};
use erp_workflow::repository::WorkItemRepositoryExt;
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
            if [internal::WORK_ITEM_EXECUTE, internal::WORK_ITEM_RECIPIENT].contains(&context.permission) {
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
    /// 从持久化任务和真实业务外键补齐来源，不读取任何账号授权结论。
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

    /// 用原执行器读取真实来源的完整事实，保留精确引用供 identity 校验完整性。
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

/// 来源必须来自实际业务对象，不能使用展示根节点或任务业务主键替代。
fn order_source(kind: ObjectKind, fact: Option<&ObjectFact>) -> Result<OrderTaskSource> {
    fact.and_then(|fact| fact.order_scope_source.as_ref())
        .filter(|source| source.matches_kind(kind))
        .cloned()
        .ok_or_else(|| Error::NotFound("订单任务缺少权威业务来源".into()))
}

/// 跨边界读取保留冲突、基础设施和授权错误的原始分类。
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
        "supplier_settlement_statement" => task.work_item_type == WorkItemType::SupplierSettlementReview,
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
    use erp_identity::entity::authorization::FactValue;
    use mongodb::Client;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;

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
