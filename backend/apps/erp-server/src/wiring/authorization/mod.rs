//! HTTP 与 worker 共用的完整授权实现装配；本模块不声明业务政策。
mod approvals;
mod commerce;
mod directories;
mod finance;
mod inquiries;
mod master;
mod sales;
mod suppliers;
mod tasks;

use std::sync::Arc;

use erp_finance_posting::finance_posting::task_access::authorization_task_facts;
use erp_identity::{AccessFactsPort, AccessMapping, AuthorizationService, Error, Result, SharedRbacService};
use erp_integration::repository::authorization as integration;
use erp_permissions::{
    Permission, bulk_job, integration_ops, internal, purchase_order, supplier_settlement, work_item,
};
use erp_read_models::purchase_center::creation_authorization::CreationTaskMapping;
use erp_support::authorization::{background, files};
use erp_workflow::service::approval::copies;
use erp_workflow::service::work_item as work_items;
pub(crate) use master::contract_access_with_db;
use mongodb::Database;

pub(super) type FactsBindings = Vec<(String, Arc<dyn AccessFactsPort>)>;
pub(super) type MappingBindings = Vec<(Permission, Arc<dyn AccessMapping>)>;

/// 连接全部真实身份设施、业务事实和查询映射。
/// # 参数
/// db、rbac 为进程共享数据库与身份设施。
/// # 返回
/// 通过完整规则与实现能力校验的共享服务；应用工厂只注入该实例。
/// # 错误
/// 缺失、重复、未知或能力错配均拒绝，不构造部分生产服务。
pub fn build(db: Database, rbac: SharedRbacService) -> Result<Arc<AuthorizationService>> {
    let (facts, mappings) = bindings(db.clone())?;
    let mut builder = AuthorizationService::builder(db, rbac);
    for (resource, reader) in facts {
        builder = builder.facts(&resource, reader)?;
    }
    for (permission, mapping) in mappings {
        builder = builder.mapping(&permission, mapping)?;
    }
    Ok(Arc::new(builder.build()?))
}

fn bindings(db: Database) -> Result<(FactsBindings, MappingBindings)> {
    let base = master::bindings(db.clone())?;
    let (mut facts, mut mappings, purchase) = commerce::bindings(
        db.clone(),
        base.customer_facts.clone(),
        base.customer_mapping.clone(),
        base.warehouse_facts.clone(),
        base.warehouse_mapping.clone(),
    )?;
    facts.extend(base.facts);
    mappings.extend(base.mappings);
    extend(&mut facts, &mut mappings, suppliers::bindings(db.clone())?);
    let (sales_facts, sales_mappings, sales) =
        sales::bindings(db.clone(), base.customer_facts, base.customer_mapping)?;
    facts.extend(sales_facts);
    mappings.extend(sales_mappings);
    let settlement = mapping(&mappings, &supplier_settlement::STATEMENT_LIST)?;
    let (financial_facts, financial_mappings, mut financial) =
        finance::bindings(db.clone(), sales, purchase, settlement)?;
    facts.extend(financial_facts);
    mappings.extend(financial_mappings);
    financial.insert(
        "purchase_order".into(),
        facts
            .iter()
            .find(|(resource, _)| resource == "purchase_order")
            .ok_or_else(|| Error::ValidationError("缺少采购事实实现".into()))?
            .1
            .clone(),
    );
    extend(&mut facts, &mut mappings, inquiries::bindings(db.clone())?);
    extend(&mut facts, &mut mappings, directories::bindings(db.clone())?);
    let approval = approvals::bindings(db.clone(), &facts, &mappings)?;
    extend(&mut facts, &mut mappings, approval);
    runtime_bindings(
        db.clone(),
        &mut facts,
        &mut mappings,
        authorization_task_facts(db.clone(), financial)?,
    )?;
    Ok((facts, mappings))
}

fn runtime_bindings(
    db: Database,
    facts: &mut FactsBindings,
    mappings: &mut MappingBindings,
    financial: Arc<dyn AccessFactsPort>,
) -> Result<()> {
    let items = work_items::authorization_facts(db.clone());
    let roots = facts.iter().cloned().collect();
    facts.push(("work_item".into(), Arc::new(tasks::TaskFacts { db: db.clone(), items, financial, roots })));
    let task_mapping = work_items::authorization_mapping();
    let detail_mapping = tasks::TaskDetailMapping::shared(task_mapping.clone(), mappings)?;
    for permission in [work_item::LIST, work_item::MANAGE] {
        mappings.push((permission, task_mapping.clone()));
    }
    mappings.push((work_item::DETAIL, detail_mapping));
    mappings.push((purchase_order::CREATE, Arc::new(CreationTaskMapping::new()?)));
    facts.push(("approval_copy".into(), copies::authorization_facts(db.clone())));
    let copy_mapping = copies::authorization_mapping()?;
    mappings.push((internal::APPROVAL_COPY_LIST, copy_mapping.clone()));
    mappings.push((internal::APPROVAL_COPY_DETAIL, copy_mapping));
    let jobs = background::facts(db.clone());
    facts.extend([("background_job".into(), jobs.clone()), ("background_job_item".into(), jobs)]);
    let job_mapping = background::mapping();
    mappings.extend([
        (bulk_job::BACKGROUND_JOB_LIST, job_mapping.clone()),
        (internal::PRODUCT_IMPORT_JOB_READ, job_mapping),
    ]);
    facts.push(("file_asset".into(), files::facts(db.clone())));
    let integrations = integration::facts(db);
    for (resource, permissions) in [
        (
            "integration_error_task",
            vec![integration_ops::ERROR_TASK_LIST, integration_ops::ERROR_TASK_DETAIL],
        ),
        (
            "reconciliation_difference",
            vec![integration_ops::DIFFERENCE_LIST, integration_ops::DIFFERENCE_DETAIL],
        ),
    ] {
        facts.push((resource.into(), integrations.clone()));
        let mapping = integration::mapping(resource)?;
        mappings.extend(permissions.into_iter().map(|permission| (permission, mapping.clone())));
    }
    Ok(())
}

fn mapping(mappings: &MappingBindings, permission: &Permission) -> Result<Arc<dyn AccessMapping>> {
    mappings
        .iter()
        .find(|(key, _)| key == permission)
        .map(|(_, mapping)| mapping.clone())
        .ok_or_else(|| Error::ValidationError(format!("缺少共享来源实现：{permission}")))
}

fn extend(facts: &mut FactsBindings, mappings: &mut MappingBindings, pair: (FactsBindings, MappingBindings)) {
    facts.extend(pair.0);
    mappings.extend(pair.1);
}

#[cfg(test)]
mod tests {
    use application_core::AuditActor;
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::RbacService;
    use erp_identity::entity::authorization::{AccessContext, QueryOutput, RelationKey};
    use erp_identity::ports::{IdentityAuditPort, PreparedResourceAudit};
    use erp_procurement::repository::PurchaseOrderExt;
    use erp_workflow::WorkItemExt;
    use mongodb::Client;
    use mongodb::options::{ClientOptions, ServerAddress};
    use persistence_core::Executor;

    use super::*;

    struct NoAudit;
    #[async_trait]
    impl IdentityAuditPort for NoAudit {
        fn resource_log(&self, _: AuditActor, _: &str, _: &str, _: String) -> Result<PreparedResourceAudit> {
            panic!("静态Builder不得调用审计")
        }
        async fn persist(&self, _: &PreparedResourceAudit, _: &mut dyn Executor) -> Result<()> {
            panic!("静态Builder不得持久化审计")
        }
    }

    #[tokio::test]
    async fn complete_bindings_construct_shared_service_without_database_reads() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let db = Client::with_options(options).unwrap().database("authorization_binding_unit");
        let rbac = Arc::new(RbacService::new(db.clone(), Arc::new(NoAudit)));
        let result = build(db, rbac);
        assert!(result.is_ok(), "完整真实业务绑定必须通过Builder: {:?}", result.err());
    }

    #[tokio::test]
    async fn approval_management_sources_keep_purchase_orders_after_creation_task_binding() {
        let options = ClientOptions::builder()
            .hosts(vec![ServerAddress::Tcp { host: "127.0.0.1".into(), port: Some(1) }])
            .build();
        let db = Client::with_options(options).unwrap().database("approval_source_binding_unit");
        let (facts, mut mappings) = bindings(db.clone()).unwrap();
        let creation = mapping(&mappings, &purchase_order::CREATE).unwrap();
        assert_eq!(creation.model().resource, "purchase_order");
        assert_eq!(creation.model().collection, <Database as WorkItemExt>::WORK_ITEMS);
        let purchase = mapping(&mappings, &purchase_order::DETAIL).unwrap();
        let management = mapping(&mappings, &internal::APPROVAL_MANAGEMENT_READ).unwrap();
        let context = AccessContext {
            actor: AuditActor::new("manager".into(), "manager".into(), AccountKind::Admin),
            permission: internal::APPROVAL_MANAGEMENT_READ,
            resource: "approval_instance".into(),
            model: Some(management.model().key.clone()),
            as_of: Instant::from_unix_secs(1),
        };
        for kind in ["purchase_order", "purchase_change_order"] {
            let key = RelationKey::parse(format!("management_{kind}_purchase_order")).unwrap();
            let source = management.relation(&key, &context, None).unwrap();
            assert!(Arc::ptr_eq(&source.mapping, &purchase));
            assert_eq!(source.mapping.model().collection, <Database as PurchaseOrderExt>::PURCHASE_ORDERS);
            assert_eq!(source.mapping.model().output, QueryOutput::Object);
        }
        mappings.retain(|(permission, _)| permission != &purchase_order::DETAIL);
        assert!(matches!(approvals::bindings(db, &facts, &mappings), Err(Error::ValidationError(_))));
    }
}
