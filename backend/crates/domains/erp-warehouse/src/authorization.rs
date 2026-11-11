//! 仓库身份作为库存和独立目录的真实授权模型。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, ReadAccess, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject,
    AccessTarget, AuthorizationRegistry, AuthorizationService, CompiledAccess, DataAccess, Permission,
    ResourceTargets, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use crate::repository::WarehouseExt;
use crate::repository::directory::{authorized_ids_in, authorized_rows_exist};

/// 只把仓库业务字段映射为 identity 事实，不组合权限规则。
pub struct WarehouseMapping(QueryModel);
impl WarehouseMapping {
    /// 提供仓库真实身份映射供跨域来源登记复用。
    /// # 参数
    /// resource 为消费入口的固定资源。
    /// # 返回
    /// 固定仓库集合及仓库事实的映射。
    /// # 错误
    /// 固定模型标识无效时失败。
    pub fn shared(resource: &str) -> erp_identity::Result<Arc<dyn AccessMapping>> {
        Ok(Arc::new(Self(QueryModel {
            key: QueryModelKey::parse(format!("{resource}_warehouse_identity"))?,
            resource: resource.into(),
            collection: <Database as WarehouseExt>::WAREHOUSES.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Warehouse].into(),
        })))
    }
}

struct WarehouseFacts(Database);
#[async_trait]
impl AccessFactsPort for WarehouseFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<ObjectFacts>> {
        let mut objects = Vec::new();
        for id in ids.ids() {
            if let Some(row) = self.0.warehouse().warehouse(id, executor).await? {
                objects.push(ObjectFacts {
                    resource: context.resource.clone(),
                    id: row.base.id.clone(),
                    version: row.base.version.to_string(),
                    warehouse_id: FactValue::Known(Some(row.base.id)),
                    ..Default::default()
                });
            }
        }
        Ok(objects)
    }
}

#[async_trait]
impl AccessMapping for WarehouseMapping {
    fn model(&self) -> &QueryModel {
        &self.0
    }
    fn key(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    async fn prepare(&self, _: &AccessContext, _: &mut dyn Executor) -> erp_identity::Result<MappingFacts> {
        Ok(MappingFacts::default())
    }
    fn records(&self, _: &AccessContext, _: &MappingFacts) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(match fact {
            AccessFact::WarehouseIn(ids) => doc! {"id":{"$in":ids.iter().cloned().collect::<Vec<_>>() }},
            _ => return Err(erp_identity::Error::ValidationError("仓库身份仅支持仓库事实".into())),
        }))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("仓库身份没有来源关系".into()))
    }
}

/// 当前事务中由统一权限服务产生的仓库身份查询。
#[derive(Clone)]
pub struct WarehouseAuthorization {
    db: Database,
    service: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
    permission: Permission,
}

impl WarehouseAuthorization {
    /// 登记一个固定仓库身份入口。
    /// # 参数
    /// db、rbac 为基础设施；permission 和 required 为服务端固定权限。
    /// # 返回
    /// 不执行 I/O 的查询入口。
    /// # 错误
    /// 非法权限或不满足 identity 登记合同返回错误。
    pub fn new(
        db: Database,
        rbac: SharedRbacService,
        permission: Permission,
        required: Vec<Permission>,
    ) -> erp_identity::Result<Self> {
        let (registry, mapping) = registration(&permission, required, Arc::new(WarehouseFacts(db.clone())))?;
        Ok(Self {
            db: db.clone(),
            service: AuthorizationService::new(db, rbac, registry),
            mapping,
            permission,
        })
    }

    /// 查询本次展示候选内有权的真实仓库。
    /// # 参数
    /// actor 为真实账号，ids 为当前页候选，executor 为原读取执行器。
    /// # 返回
    /// 已授权候选子集，不装载全量授权身份。
    /// # 错误
    /// 身份、权限、映射或数据库失败透传。
    pub async fn ids_in(
        &self,
        actor: &AuditActor,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let query = self.service.query(actor, &self.permission, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        self.service.validate(&compiled, executor).await?;
        let ids = authorized_ids_in(&self.db, &compiled, ids, executor).await?;
        self.service.validate(&compiled, executor).await?;
        Ok(ids)
    }

    /// 在原业务执行器检查实际仓库的完整动作资格。
    /// # 参数
    /// actor 为实际账号，id 为实际仓库，executor 为原事务。
    /// # 返回
    /// 功能、同角色附加资格和仓库范围均成立时成功。
    /// # 错误
    /// 仓库缺失、无权或事实变化拒绝。
    pub async fn require(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        self.service
            .require(
                actor,
                &self.permission,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await
    }

    /// 编译当前仓库查询，不预载全部授权身份。
    /// # 参数
    /// actor 为真实账号，executor 为调用方读取事务。
    /// # 返回
    /// 完整授权凭证和最多一行存在性查询得到的空范围标记。
    /// # 错误
    /// 身份、权限、映射、数据库或授权复验失败时返回原错误。
    pub async fn query(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<WarehouseQuery> {
        let query = self.service.query(actor, &self.permission, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        self.service.validate(&compiled, executor).await?;
        let has_rows = authorized_rows_exist(&self.db, &compiled, executor).await?;
        self.service.validate(&compiled, executor).await?;
        Ok(WarehouseQuery { has_rows, authorization: self.service.clone(), compiled: Arc::new(compiled) })
    }
}

/// 当前仓库查询的完整凭证；目录和列表不通过授权 ID 集合执行筛选。
#[derive(Clone)]
pub struct WarehouseQuery {
    has_rows: bool,
    authorization: AuthorizationService,
    compiled: Arc<CompiledAccess>,
}

impl WarehouseQuery {
    /// 读取完整授权阶段。
    /// # 参数
    /// 无。
    /// # 返回
    /// identity 编译的全部阶段，须在分页和数量上限前执行。
    /// # 错误
    /// 无。
    pub(crate) fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }

    /// 读取本次授权查询版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整身份及映射依赖版本。
    /// # 错误
    /// 无。
    pub(crate) fn version(&self) -> String {
        self.compiled.version()
    }

    /// 返回本次冻结的授权元数据。
    /// # 参数
    /// 无。
    /// # 返回
    /// 权限版本、组织版本和授权时点。
    /// # 错误
    /// 无；实际有效性仍由 validate 核验。
    pub(crate) fn metadata(&self) -> (u64, u64, Instant) {
        (self.compiled.policy_version(), self.compiled.organization_version(), self.compiled.as_of())
    }

    /// 判断完整授权查询是否没有实际仓库。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整查询的有界存在性结果，不解释权限规则。
    /// # 错误
    /// 无。
    pub fn is_empty(&self) -> bool {
        !self.has_rows
    }

    /// 在实际业务读取前后复验凭证。
    /// # 参数
    /// executor 为产生本凭证的原读取事务。
    /// # 返回
    /// 当前凭证仍然有效时成功。
    /// # 错误
    /// 授权或版本失效时返回原始错误。
    pub(crate) async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

/// 固定入口登记仅表达仓库事实和同角色附加功能要求。
fn registration(
    permission: &Permission,
    required: Vec<Permission>,
    facts: Arc<dyn AccessFactsPort>,
) -> erp_identity::Result<(AuthorizationRegistry, Arc<dyn AccessMapping>)> {
    let mapping: Arc<dyn AccessMapping> = Arc::new(WarehouseMapping(QueryModel {
        key: QueryModelKey::parse(format!("{}_warehouses", permission.resource()))?,
        resource: permission.resource().into(),
        collection: <Database as WarehouseExt>::WAREHOUSES.into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [FactKind::Warehouse].into(),
    }));
    let data = DataAccess {
        subject: AccessSubject::Direct,
        required_facts: [FactKind::Warehouse].into(),
        policy: AccessPolicy::Scope { permission: permission.clone(), read: ReadAccess::default() },
    };
    let registry = AuthorizationRegistry::new(vec![AccessRegistration {
        permission: permission.clone(),
        required_permissions: required,
        access: AccessMode::QueryAndObject { data, mapping: mapping.clone(), facts },
    }])?;
    Ok((registry, mapping))
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

    struct UnusedFacts;
    #[async_trait]
    impl AccessFactsPort for UnusedFacts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> erp_identity::Result<Vec<ObjectFacts>> {
            panic!("目录登记不读对象")
        }
    }
    fn mapping() -> WarehouseMapping {
        WarehouseMapping(QueryModel {
            key: QueryModelKey::parse("warehouse_rows").unwrap(),
            resource: "warehouse".into(),
            collection: "warehouses".into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Warehouse].into(),
        })
    }
    fn context() -> AccessContext {
        AccessContext {
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
            permission: Permission::parse("warehouse:list").unwrap(),
            resource: "warehouse".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }
    #[test]
    fn warehouse_fact_uses_real_identity_and_rejects_unsupported_dimensions() {
        let mapping = mapping();
        let context = context();
        let prepared = MappingFacts::default();
        let fragment = mapping
            .predicate(&AccessFact::WarehouseIn(["wh-a".into(), "wh-b".into()].into()), &context, &prepared)
            .unwrap();
        assert!(
            matches!(fragment, QueryFragment::Match(filter) if filter == doc! {"id":{"$in":["wh-a","wh-b"]}})
        );
        assert!(mapping.predicate(&AccessFact::Owned, &context, &prepared).is_err());
        assert!(mapping.relation(&RelationKey::parse("source").unwrap(), &context, &prepared).is_err());
    }
    #[test]
    fn model_records_exclude_deleted_warehouse_identities() {
        let mapping = mapping();
        assert_eq!(mapping.key(&context(), &MappingFacts::default()).unwrap(), Bson::String("$id".into()));
        assert!(
            matches!(mapping.records(&context(), &MappingFacts::default()).unwrap(), QueryFragment::Match(filter) if filter == doc! {"deleted_at":0})
        );
    }
    #[test]
    fn known_warehouse_entries_register_and_unknown_actions_fail_closed() {
        for (code, detail) in [
            ("warehouse:list", false),
            ("stock_balance:list", false),
            ("stock_balance:detail", false),
            ("stock_movement:list", false),
            ("stock_reservation:list", false),
            ("stock_adjustment:list", true),
            ("stock_adjustment:detail", false),
            ("stock_adjustment:create", true),
            ("stock_adjustment:update", true),
        ] {
            let required =
                detail.then(|| Permission::parse("stock_adjustment:detail").unwrap()).into_iter().collect();
            assert!(
                registration(&Permission::parse(code).unwrap(), required, Arc::new(UnusedFacts)).is_ok(),
                "{code}"
            );
        }
        assert!(
            registration(&Permission::parse("warehouse:update").unwrap(), Vec::new(), Arc::new(UnusedFacts))
                .is_err()
        );
    }
}
