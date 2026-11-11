//! 完整财务金额的统一资格：identity 决定完整来源视图成员资格。

use std::collections::BTreeSet;
use std::collections::hash_map::DefaultHasher;
use std::hash::{Hash, Hasher};
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, MappingFacts, QueryModel, QueryModelKey, QueryOutput, ReadAccess, RelationKey,
    RelationMatch,
};
use erp_identity::entity::policy_permission::FINANCE_LEDGER_READ;
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget,
    AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError, Permission,
    Result as IdentityResult, SharedRbacService,
};
use erp_procurement::authorization::PurchaseMapping;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use super::settlement_mapping::SettlementMapping;
use super::source_access::SourceQuery;
use crate::sales_center::repository::authorization::SalesMapping;
use crate::{Error, Result};

const SOURCE_FIELDS: [&str; 3] = ["__funds_sales_ids", "__funds_purchase_ids", "__funds_settlement_ids"];
const RELATIONS: [&str; 3] = ["whole_sales", "whole_purchase", "whole_settlement"];

/// 当前读取阶段的完整来源查询与无来源财务整账资格。
#[derive(Clone, Default)]
pub(crate) struct WholeAccess {
    queries: Vec<SourceQuery>,
    zero: bool,
    collection: String,
}

impl WholeAccess {
    /// 在原事务中证明完整金额资格并编译全部非空来源模型。
    /// # 参数
    /// db、rbac 为身份设施，actor 为当前身份，entry 为固定资金读取入口；
    /// collection 为服务端登记的资金根集合，executor 为原业务执行器。
    /// # 返回
    /// 同角色具备入口与整账权限时返回当前阶段凭证，否则返回空完整金额资格。
    /// # 错误
    /// 未登记入口、非法映射、身份设施或授权取证失败时返回对应错误。
    pub(crate) async fn query(
        db: &Database,
        rbac: &SharedRbacService,
        actor: &AuditActor,
        entry: &Permission,
        collection: &str,
        executor: &mut dyn Executor,
    ) -> Result<Self> {
        require_entry(entry)?;
        let ledger = Permission::parse(FINANCE_LEDGER_READ)?;
        let mut registrations = vec![AccessRegistration {
            permission: ledger.clone(),
            required_permissions: vec![entry.clone()],
            access: AccessMode::Operation,
        }];
        let mappings = whole_mappings(db, entry, collection)?;
        for mapping in &mappings {
            registrations.push(registration(entry, &ledger, mapping.clone())?);
        }
        let service =
            AuthorizationService::new(db.clone(), rbac.clone(), AuthorizationRegistry::new(registrations)?);
        let permissions = [entry.clone(), ledger.clone()];
        let qualified = service.permissions(actor, &permissions, executor).await?;
        if !qualified.is_some_and(|qualified| qualified.same_role(&permissions)) {
            return Ok(Self::default());
        }
        service.require(actor, &ledger, AccessTarget::Operation, executor).await?;
        let queries = compile_views(&service, actor, entry, mappings, executor).await?;
        Ok(Self { queries, zero: true, collection: collection.into() })
    }

    /// 将当前投影行与 identity 授权视图按完整根身份关联。
    /// # 参数
    /// 无；调用方必须先投影三个完整来源 ID 数组与真实来源存在性。
    /// # 返回
    /// 设置 scope_whole 的聚合阶段；无完整资格时恒为 false。
    /// # 错误
    /// 不执行 I/O；使用阶段前后必须调用 validate。
    pub(crate) fn stages(&self) -> Vec<Document> {
        let mut stages = Vec::new();
        let mut aliases = Vec::new();
        if self.zero {
            let alias = "__funds_whole_zero".to_owned();
            stages.push(root_lookup(&self.collection, &alias, zero_stages()));
            aliases.push(alias);
        }
        for (index, query) in self.queries.iter().enumerate() {
            let alias = format!("__funds_whole_view_{index}");
            stages.push(root_lookup(&self.collection, &alias, query.stages().to_vec()));
            aliases.push(alias);
        }
        stages.extend(membership_stages(aliases));
        stages
    }

    /// 复验本次完整来源视图的身份、政策及映射依赖。
    /// # 参数
    /// executor 为原读取执行器。
    /// # 返回
    /// 所有视图凭证仍有效时成功。
    /// # 错误
    /// 撤权、会话或版本变化及持久化失败时返回对应错误。
    pub(crate) async fn validate(&self, executor: &mut dyn Executor) -> Result<()> {
        for query in &self.queries {
            query.validate(executor).await?;
        }
        Ok(())
    }

    /// 返回 identity 未提供任何完整金额资格的状态。
    /// # 参数
    /// 无。
    /// # 返回
    /// 整账操作未授权且没有完整来源凭证时为 true。
    /// # 错误
    /// 不执行 I/O。
    pub(crate) fn is_empty(&self) -> bool {
        !self.zero && self.queries.is_empty()
    }

    /// 为跨页结果绑定完整来源及整账资格版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前完整金额凭证的稳定摘要；空资格使用独立固定摘要。
    /// # 错误
    /// 不执行 I/O，不能替代 validate。
    pub(crate) fn version(&self) -> String {
        let mut hash = DefaultHasher::new();
        "funds-whole-membership-v1".hash(&mut hash);
        self.zero.hash(&mut hash);
        for query in &self.queries {
            query.access.version().hash(&mut hash);
        }
        format!("{:x}", hash.finish())
    }
}

struct WholeMapping {
    model: QueryModel,
    partition: u8,
    sources: [Arc<dyn AccessMapping>; 3],
}

impl WholeMapping {
    /// 每个非空来源分区只提供业务存在事实，全部授权组合由登记政策执行。
    fn new(
        entry: &Permission,
        collection: &str,
        partition: u8,
        sources: [Arc<dyn AccessMapping>; 3],
    ) -> IdentityResult<Self> {
        let model = QueryModel {
            key: QueryModelKey::parse(format!("funds_whole_{partition}"))?,
            resource: entry.resource().into(),
            collection: collection.into(),
            id_field: "id".into(),
            output: QueryOutput::Existence { fields: ["id".into()].into() },
            facts: BTreeSet::new(),
        };
        model.validate()?;
        Ok(Self { model, partition, sources })
    }
}

#[async_trait]
impl AccessMapping for WholeMapping {
    fn model(&self) -> &QueryModel {
        &self.model
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
        Ok(QueryFragment::Match(partition_records(self.partition)))
    }
    fn predicate(
        &self,
        _: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        Err(IdentityError::ValidationError("完整金额根模型不提供责任事实".into()))
    }
    fn relation(
        &self,
        relation: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        let index = RELATIONS
            .iter()
            .position(|key| *key == relation.as_str())
            .ok_or_else(|| IdentityError::ValidationError("未知完整金额来源关系".into()))?;
        Ok(RelationMapping {
            expected_ids: format!("${}", SOURCE_FIELDS[index]).into(),
            mapping: self.sources[index].clone(),
        })
    }
}

/// 只接入拥有完整来源投影的固定财务读取入口。
fn require_entry(entry: &Permission) -> Result<()> {
    let registered = match entry.resource() {
        "cost_allocation" => entry.action() == "list",
        "customer_receipt" | "supplier_payment" | "invoice" | "cost_entry" => {
            matches!(entry.action(), "list" | "detail")
        },
        _ => false,
    };
    if registered { Ok(()) } else { Err(Error::ValidationError("未登记完整金额读取入口".into())) }
}

/// 固定来源模型不接受客户端集合、字段或政策参数。
fn whole_mappings(db: &Database, entry: &Permission, collection: &str) -> Result<Vec<Arc<WholeMapping>>> {
    let sources: [Arc<dyn AccessMapping>; 3] = [
        Arc::new(SalesMapping::new(db.clone(), "sales_order", "funds_whole_sales")?),
        Arc::new(PurchaseMapping::new("purchase_order")?),
        Arc::new(SettlementMapping::new()?),
    ];
    (1..8)
        .map(|partition| {
            WholeMapping::new(entry, collection, partition, sources.clone())
                .map(Arc::new)
                .map_err(Error::from)
        })
        .collect()
}

/// 财务入口及整账资格必须由同一有效角色提供，来源关系必须全部非空命中。
fn registration(
    entry: &Permission,
    ledger: &Permission,
    mapping: Arc<WholeMapping>,
) -> IdentityResult<AccessRegistration> {
    let mut policies = Vec::new();
    for (index, source) in mapping.sources.iter().enumerate() {
        if mapping.partition & (1 << index) == 0 {
            continue;
        }
        let resource = source.model().resource.as_str();
        let action = if index == 2 { "list" } else { "detail" };
        let policy = AccessPolicy::Scope {
            permission: Permission::parse(format!("{resource}:{action}"))?,
            read: ReadAccess { history: false, records: index != 2 },
        };
        policies.push(AccessPolicy::Related {
            relation: RelationKey::parse(RELATIONS[index])?,
            resource: resource.into(),
            mode: RelationMatch::AllNonEmpty,
            required_facts: source.model().facts.clone(),
            policy: Box::new(policy),
        });
    }
    Ok(AccessRegistration {
        permission: entry.clone(),
        required_permissions: vec![ledger.clone()],
        access: AccessMode::Query {
            data: DataAccess {
                subject: AccessSubject::View(mapping.model.key.clone()),
                required_facts: BTreeSet::new(),
                policy: AccessPolicy::All(policies),
            },
            mapping,
        },
    })
}

/// 保留生成凭证的同一服务实例和映射实例，禁止更换注册目录。
async fn compile_views(
    service: &AuthorizationService,
    actor: &AuditActor,
    entry: &Permission,
    mappings: Vec<Arc<WholeMapping>>,
    executor: &mut dyn Executor,
) -> Result<Vec<SourceQuery>> {
    let mut queries = Vec::new();
    for mapping in mappings {
        let subject = AccessSubject::View(mapping.model.key.clone());
        let query = service.query(actor, entry, subject, executor).await?;
        let access = Arc::new(query.compile(mapping.as_ref(), executor).await?);
        service.validate(&access, executor).await?;
        queries.push(SourceQuery { access, service: service.clone() });
    }
    Ok(queries)
}

/// 空来源与损坏引用分别表达，已知损坏来源不能落入空来源分区。
fn partition_records(partition: u8) -> Document {
    let conditions = SOURCE_FIELDS
        .iter()
        .enumerate()
        .map(|(index, field)| {
            let operator = if partition & (1 << index) == 0 { "$eq" } else { "$gt" };
            Bson::Document(doc! { operator: [{ "$size": { "$ifNull": [format!("${field}"), []] } }, 0] })
        })
        .collect::<Vec<_>>();
    doc! { "deleted_at": 0, "__funds_sources_valid": true, "$expr": { "$and": conditions } }
}

/// 无来源根只接受已经完成财务整账 Operation 授权的查询路径。
fn zero_stages() -> Vec<Document> {
    vec![doc! { "$match": partition_records(0) }, doc! { "$project": { "_id": 0, "id": 1 } }]
}

/// 关联只读取同一个实际根，恢复的预投影变量不能改变根的业务身份。
fn root_lookup(collection: &str, alias: &str, authorized: Vec<Document>) -> Document {
    let mut bindings = doc! { "root_id": "$id", "sources_valid": "$__funds_sources_valid" };
    let mut fields = doc! { "__funds_sources_valid": "$$sources_valid" };
    for (index, field) in SOURCE_FIELDS.iter().enumerate() {
        let variable = format!("source_ids_{index}");
        bindings.insert(&variable, format!("${field}"));
        fields.insert(*field, format!("$${variable}"));
    }
    let mut pipeline =
        vec![doc! { "$match": { "$expr": { "$eq": ["$id", "$$root_id"] } } }, doc! { "$set": fields }];
    pipeline.extend(authorized);
    doc! { "$lookup": { "from": collection, "let": bindings, "pipeline": pipeline, "as": alias } }
}

/// 合并已经通过 identity 的只读视图会员结果，不重新解释来源或人员范围。
fn membership_stages(aliases: Vec<String>) -> Vec<Document> {
    let conditions = aliases
        .iter()
        .map(|alias| Bson::Document(doc! { "$gt": [{ "$size": format!("${alias}") }, 0] }))
        .collect::<Vec<_>>();
    let mut stages = vec![
        doc! { "$set": { "scope_whole": if conditions.is_empty() { Bson::Boolean(false) } else { Bson::Document(doc! { "$or": conditions }) } } },
    ];
    if !aliases.is_empty() {
        stages.push(doc! { "$unset": aliases });
    }
    stages
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

    struct FieldSource(QueryModel);

    impl FieldSource {
        fn new(resource: &str) -> Self {
            Self(QueryModel {
                key: QueryModelKey::parse(format!("test_{resource}")).unwrap(),
                resource: resource.into(),
                collection: format!("{resource}s"),
                id_field: "id".into(),
                output: QueryOutput::Object,
                facts: BTreeSet::new(),
            })
        }
    }

    #[async_trait]
    impl AccessMapping for FieldSource {
        fn model(&self) -> &QueryModel {
            &self.0
        }
        fn key(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<Bson> {
            Ok("$id".into())
        }
        async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> IdentityResult<MappingFacts> {
            Ok(MappingFacts::default())
        }
        fn records(&self, _: &AccessContext, _: &MappingFacts) -> IdentityResult<QueryFragment> {
            Ok(QueryFragment::Match(doc! { "deleted_at": 0 }))
        }
        fn predicate(
            &self,
            _: &AccessFact,
            _: &AccessContext,
            _: &MappingFacts,
        ) -> IdentityResult<QueryFragment> {
            Err(IdentityError::ValidationError("测试来源未提供责任维度".into()))
        }
        fn relation(
            &self,
            _: &RelationKey,
            _: &AccessContext,
            _: &MappingFacts,
        ) -> IdentityResult<RelationMapping> {
            Err(IdentityError::ValidationError("测试来源没有下级关系".into()))
        }
    }

    fn sources() -> [Arc<dyn AccessMapping>; 3] {
        [
            Arc::new(FieldSource::new("sales_order")),
            Arc::new(FieldSource::new("purchase_order")),
            Arc::new(FieldSource::new("supplier_settlement_statement")),
        ]
    }

    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("finance".into(), "财务".into(), AccountKind::Admin),
            permission: Permission::parse("invoice:list").unwrap(),
            resource: "invoice".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    #[test]
    fn whole_views_only_expose_identity_and_keep_complete_actual_source_arrays() {
        let entry = Permission::parse("invoice:list").unwrap();
        let mapping = WholeMapping::new(&entry, "invoices", 7, sources()).unwrap();
        assert_eq!(mapping.model().output, QueryOutput::Existence { fields: ["id".into()].into() });
        for (name, expected, resource) in [
            ("whole_sales", "$__funds_sales_ids", "sales_order"),
            ("whole_purchase", "$__funds_purchase_ids", "purchase_order"),
            ("whole_settlement", "$__funds_settlement_ids", "supplier_settlement_statement"),
        ] {
            let related = mapping
                .relation(&RelationKey::parse(name).unwrap(), &context(), &MappingFacts::default())
                .unwrap();
            assert_eq!(related.expected_ids, Bson::String(expected.into()));
            assert_eq!(related.mapping.model().resource, resource);
        }
        assert!(
            mapping
                .relation(&RelationKey::parse("missing").unwrap(), &context(), &MappingFacts::default())
                .is_err()
        );
        assert!(mapping.predicate(&AccessFact::Owned, &context(), &MappingFacts::default()).is_err());
    }

    #[test]
    fn every_nonempty_view_requires_ledger_and_all_actual_sources() {
        let entry = Permission::parse("invoice:list").unwrap();
        let ledger = Permission::parse(FINANCE_LEDGER_READ).unwrap();
        let mut entries = Vec::new();
        for partition in 1..8 {
            let mapping = Arc::new(WholeMapping::new(&entry, "invoices", partition, sources()).unwrap());
            let registration = registration(&entry, &ledger, mapping).unwrap();
            assert_eq!(registration.required_permissions, vec![ledger.clone()]);
            let AccessMode::Query { data, .. } = &registration.access else {
                panic!("必须使用受限查询视图");
            };
            let AccessPolicy::All(policies) = &data.policy else {
                panic!("必须要求全部来源");
            };
            assert!(!policies.is_empty());
            for policy in policies {
                let AccessPolicy::Related { mode, policy, .. } = policy else {
                    panic!("必须沿真实来源登记");
                };
                assert_eq!(*mode, RelationMatch::AllNonEmpty);
                let AccessPolicy::Scope { read, .. } = policy.as_ref() else {
                    panic!("必须使用identity范围政策");
                };
                assert!(!read.history);
            }
            entries.push(registration);
        }
        assert!(AuthorizationRegistry::new(entries).is_ok());
    }

    #[test]
    fn damaged_sources_cannot_enter_the_zero_partition_and_lookup_keeps_the_root_identity() {
        let zero = partition_records(0);
        assert!(zero.get_bool("__funds_sources_valid").unwrap());
        let conditions = zero.get_document("$expr").unwrap().get_array("$and").unwrap();
        assert_eq!(conditions.len(), 3);
        for condition in conditions {
            assert!(condition.as_document().unwrap().contains_key("$eq"));
        }
        let lookup = root_lookup("invoices", "membership", zero_stages());
        let lookup = lookup.get_document("$lookup").unwrap();
        assert_eq!(lookup.get_str("from").unwrap(), "invoices");
        let pipeline = lookup.get_array("pipeline").unwrap();
        assert_eq!(
            pipeline[0],
            Bson::Document(doc! { "$match": { "$expr": { "$eq": ["$id", "$$root_id"] } } })
        );
        let restored = pipeline[1].as_document().unwrap().get_document("$set").unwrap();
        assert_eq!(restored.get_str("__funds_sources_valid").unwrap(), "$$sources_valid");
        assert_eq!(restored.get_str("__funds_sales_ids").unwrap(), "$$source_ids_0");
        assert_eq!(pipeline.last().unwrap(), &Bson::Document(doc! { "$project": { "_id": 0, "id": 1 } }));
    }

    #[test]
    fn empty_qualification_never_builds_a_source_lookup_or_whole_amount_membership() {
        let denied = WholeAccess::default();
        assert!(denied.is_empty());
        assert_eq!(denied.stages(), vec![doc! { "$set": { "scope_whole": false } }]);
        assert_eq!(denied.version(), WholeAccess::default().version());
        assert!(require_entry(&Permission::parse("cost_allocation:list").unwrap()).is_ok());
        assert!(require_entry(&Permission::parse("cost_allocation:detail").unwrap()).is_err());
        assert!(require_entry(&Permission::parse("invoice:update").unwrap()).is_err());
    }
}
