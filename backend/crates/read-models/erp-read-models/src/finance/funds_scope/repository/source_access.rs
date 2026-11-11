//! 资金入口的真实来源查询；字段映射只提供事实，组合政策交给 identity。

use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, QueryModel, QueryModelKey, QueryOutput, ReadAccess, RelationKey,
    RelationMatch,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry,
    AuthorizationService, CompiledAccess, DataAccess, Error as IdentityError, Permission,
    Result as IdentityResult, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document};
use persistence_core::Executor;

use crate::{Error, Result};

/// 本次资金来源读取的 identity 凭证及其复验服务。
#[derive(Clone)]
pub struct SourceQuery {
    /// 当前读取阶段的不可伪造查询凭证。
    pub access: Arc<CompiledAccess>,
    /// 生成该凭证的同一授权服务。
    pub service: AuthorizationService,
}

impl SourceQuery {
    /// 在原执行器中复验当前来源凭证。
    /// # 参数
    /// executor 为本次查询沿用的执行器。
    /// # 返回
    /// 身份、授权及映射依赖仍有效时成功。
    /// # 错误
    /// 授权撤销、版本变化、会话不一致或持久化失败时返回对应错误。
    pub async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        self.service.validate(&self.access, executor).await?;
        Ok(())
    }

    /// 取得编译器生成的完整来源授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// 必须在分页、计数、汇总之前执行的只读聚合阶段。
    /// # 错误
    /// 不执行 I/O；阶段使用前后仍须复验凭证。
    pub fn stages(&self) -> &[Document] {
        self.access.stages()
    }
}

struct SourceMapping {
    model: QueryModel,
    relation: RelationKey,
    source: Arc<dyn AccessMapping>,
}

impl SourceMapping {
    /// 根行与实际来源使用同一物理主键，不预筛除任何来源引用。
    fn new(
        entry: &Permission,
        source: Arc<dyn AccessMapping>,
        relation: RelationKey,
    ) -> IdentityResult<Self> {
        Ok(Self {
            model: QueryModel {
                key: QueryModelKey::parse(format!("funds_{}", source.model().resource))?,
                resource: entry.resource().into(),
                collection: source.model().collection.clone(),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: BTreeSet::new(),
            },
            relation,
            source,
        })
    }
}

#[async_trait]
impl AccessMapping for SourceMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, context: &AccessContext, facts: &MappingFacts) -> IdentityResult<Bson> {
        self.source.key(context, facts)
    }
    async fn prepare(
        &self,
        context: &AccessContext,
        executor: &mut dyn Executor,
    ) -> IdentityResult<MappingFacts> {
        self.source.prepare(context, executor).await
    }
    fn records(&self, context: &AccessContext, facts: &MappingFacts) -> IdentityResult<QueryFragment> {
        self.source.records(context, facts)
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        Err(IdentityError::ValidationError("来源根不提供责任事实".into()))
    }
    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        if relation != &self.relation {
            return Err(IdentityError::ValidationError("未知资金来源关系".into()));
        }
        Ok(RelationMapping { expected_ids: Bson::Array(vec!["$id".into()]), mapping: self.source.clone() })
    }
}

/// 按固定入口登记真实来源并编译完整查询。
/// # 参数
/// db、rbac 为身份设施，actor 为真实操作人，entry 为固定业务入口；
/// source 为真实来源 Mapping，executor 为原业务执行器。
/// # 返回
/// 绑定原事务的完整查询凭证。
/// # 错误
/// 入口未登记、事实不完整或授权失败时拒绝。
pub(in crate::finance::funds_scope) async fn source_query(
    db: &Database,
    rbac: &SharedRbacService,
    actor: &AuditActor,
    entry: &Permission,
    source: Arc<dyn AccessMapping>,
    executor: &mut dyn Executor,
) -> Result<SourceQuery> {
    require_source_entry(entry)?;
    let relation = RelationKey::parse("funds_source")?;
    let mapping: Arc<dyn AccessMapping> =
        Arc::new(SourceMapping::new(entry, source.clone(), relation.clone())?);
    let registration = source_registration(entry, source.as_ref(), mapping.clone(), relation.clone())?;
    let registry = AuthorizationRegistry::new(vec![registration])?;
    let service = AuthorizationService::new(db.clone(), rbac.clone(), registry);
    let query = service.query(actor, entry, AccessSubject::Source(relation), executor).await?;
    let access = Arc::new(query.compile(mapping.as_ref(), executor).await?);
    service.validate(&access, executor).await?;
    Ok(SourceQuery { access, service })
}

/// 只接受组合根已接入的资金采购来源入口。
fn require_source_entry(entry: &Permission) -> Result<()> {
    if matches!(
        entry.resource(),
        "payable_account"
            | "supplier_payment"
            | "purchase_payment_request"
            | "invoice"
            | "purchase_invoice_allocation"
    ) {
        Ok(())
    } else {
        Err(Error::ValidationError("未登记的资金采购来源入口".into()))
    }
}

/// 保留目标入口资格，数据政策绑定真实来源及其完整关系。
fn source_registration(
    entry: &Permission,
    source: &dyn AccessMapping,
    mapping: Arc<dyn AccessMapping>,
    relation: RelationKey,
) -> IdentityResult<AccessRegistration> {
    let source_resource = source.model().resource.as_str();
    let action = if source_resource == "supplier_settlement_statement" { "list" } else { "detail" };
    let permission = Permission::parse(format!("{source_resource}:{action}"))?;
    let read = ReadAccess { history: false, records: source_resource == "purchase_order" };
    Ok(AccessRegistration {
        permission: entry.clone(),
        required_permissions: vec![],
        access: AccessMode::Query {
            data: DataAccess {
                subject: AccessSubject::Source(relation.clone()),
                required_facts: BTreeSet::new(),
                policy: AccessPolicy::Related {
                    relation,
                    resource: source.model().resource.clone(),
                    mode: RelationMatch::AllNonEmpty,
                    required_facts: source.model().facts.clone(),
                    policy: Box::new(AccessPolicy::Scope { permission, read }),
                },
            },
            mapping,
        },
    })
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use mongodb::bson::doc;

    use super::*;
    use crate::finance::funds_scope::repository::settlement_mapping::SettlementMapping;

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "经办人".into(), AccountKind::Admin),
            permission: Permission::parse("payable_account:list").unwrap(),
            resource: "payable_account".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn source_mapping_preserves_actual_record_identity_and_rejects_unknown_facts_and_relations() {
        let entry = Permission::parse("payable_account:list").unwrap();
        let source: Arc<dyn AccessMapping> = Arc::new(SettlementMapping::new().unwrap());
        let key = RelationKey::parse("funds_source").unwrap();
        let mapping = SourceMapping::new(&entry, source.clone(), key.clone()).unwrap();
        let facts = MappingFacts::default();
        assert_eq!(mapping.key(&context(), &facts).unwrap(), Bson::String("$id".into()));
        assert_eq!(mapping.model().collection, source.model().collection);
        assert_eq!(mapping.model().resource, "payable_account");
        assert!(mapping.model().facts.is_empty());
        let QueryFragment::Match(records) = mapping.records(&context(), &facts).unwrap() else {
            panic!("直接来源记录应使用字段条件");
        };
        assert_eq!(records, doc! { "deleted_at": 0 });
        let related = mapping.relation(&key, &context(), &facts).unwrap();
        assert_eq!(related.expected_ids, Bson::Array(vec!["$id".into()]));
        assert!(Arc::ptr_eq(&related.mapping, &source));
        assert!(mapping.relation(&RelationKey::parse("other").unwrap(), &context(), &facts).is_err());
        assert!(mapping.predicate(&AccessFact::Owned, &context(), &facts).is_err());
    }

    #[test]
    fn registered_entry_keeps_target_qualification_and_all_nonempty_source_policy() {
        let entry = Permission::parse("payable_account:list").unwrap();
        let source: Arc<dyn AccessMapping> = Arc::new(SettlementMapping::new().unwrap());
        let relation = RelationKey::parse("funds_source").unwrap();
        let mapping: Arc<dyn AccessMapping> =
            Arc::new(SourceMapping::new(&entry, source.clone(), relation.clone()).unwrap());
        let registration = source_registration(&entry, source.as_ref(), mapping, relation.clone()).unwrap();
        assert_eq!(registration.permission, entry);
        let AccessMode::Query { data, .. } = &registration.access else {
            panic!("必须登记查询");
        };
        assert_eq!(data.subject, AccessSubject::Source(relation));
        let AccessPolicy::Related { resource, mode, policy, .. } = &data.policy else {
            panic!("必须绑定真实来源");
        };
        assert_eq!(resource, "supplier_settlement_statement");
        assert_eq!(*mode, RelationMatch::AllNonEmpty);
        let AccessPolicy::Scope { permission, read } = policy.as_ref() else {
            panic!("必须使用identity范围");
        };
        assert_eq!(permission, &Permission::parse("supplier_settlement_statement:list").unwrap());
        assert!(!read.history);
        assert!(!read.records);
        assert!(AuthorizationRegistry::new(vec![registration]).is_ok());
        assert!(require_source_entry(&entry).is_ok());
        assert!(require_source_entry(&Permission::parse("customer:list").unwrap()).is_err());
    }
}
