//! 仓库身份作为库存和独立目录的真实授权模型。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessTarget, AuthorizationService, CompiledAccess,
    Error as IdentityError, ResourceTargets, Result as IdentityResult,
};
use erp_permissions::Permission;
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use crate::entity::warehouse::Warehouse;
use crate::repository::WarehouseExt;
use crate::repository::directory::{authorized_ids_in, authorized_rows_exist};

/// 只把仓库业务字段映射为 identity 事实，不组合权限规则。
pub struct WarehouseMapping(QueryModel);
impl WarehouseMapping {
    /// 为组合层构造消费入口到真实仓库集合的授权映射，固定使用 `id` 标识仓库对象并支持仓库和协作事实；生成的模型标识不合法时返回错误。
    pub fn shared(resource: &str) -> erp_identity::Result<Arc<dyn AccessMapping>> {
        Ok(Arc::new(Self(QueryModel {
            key: QueryModelKey::parse(format!("{resource}_warehouse_identity"))?,
            resource: resource.into(),
            collection: <Database as WarehouseExt>::WAREHOUSES.into(),
            id_field: "id".into(),
            output: QueryOutput::Object,
            facts: [FactKind::Warehouse, FactKind::Collaborators].into(),
        })))
    }
}

pub struct WarehouseFacts(Database);
impl WarehouseFacts {
    /// 将本域仓库数据库封装为共享授权事实端口，供跨域身份检查复用；构造不访问数据库。
    pub fn shared(db: Database) -> Arc<dyn AccessFactsPort> {
        Arc::new(Self(db))
    }

    // 把仓库行转换成授权对象事实；仓库身份取自身 ID，协作人明确为空，与查询端协作条件恒不匹配保持一致。
    fn object(context: &AccessContext, row: Warehouse) -> ObjectFacts {
        ObjectFacts {
            resource: context.resource.clone(),
            id: row.base.id.clone(),
            version: row.base.version.to_string(),
            warehouse_id: FactValue::Known(Some(row.base.id)),
            collaborators: FactValue::Known(Default::default()),
            ..Default::default()
        }
    }
}
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
            if let Some(row) = self.0.warehouses().find_by_id(id, executor).await? {
                objects.push(Self::object(context, row));
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
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<Bson> {
        Ok("$id".into())
    }
    fn records(&self, _: &AccessContext, _: Option<&MappingFacts>) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(doc! {"deleted_at":0}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<QueryFragment> {
        Ok(QueryFragment::Match(match fact {
            AccessFact::WarehouseIn(ids) => doc! {"id":{"$in":ids.iter().cloned().collect::<Vec<_>>() }},
            AccessFact::Collaborating => doc! {"$expr": false},
            _ => return Err(erp_identity::Error::ValidationError("仓库身份仅支持仓库事实".into())),
        }))
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: Option<&MappingFacts>,
    ) -> erp_identity::Result<RelationMapping> {
        Err(erp_identity::Error::ValidationError("仓库身份没有来源关系".into()))
    }
}

/// 当前事务中由统一权限服务产生的仓库身份查询。
#[derive(Clone)]
pub struct WarehouseAuthorization {
    db: Database,
    service: Arc<AuthorizationService>,
    permission: Permission,
}

impl WarehouseAuthorization {
    /// 将共享授权服务和固定动作权限绑定到仓库数据库，供实际仓库身份检查；构造不执行 I/O，权限与映射由共享服务在启动时校验。
    pub fn new(db: Database, service: Arc<AuthorizationService>, permission: Permission) -> Self {
        Self { db, service, permission }
    }

    /// 用原读取执行器对当前页候选执行完整仓库授权，返回候选中的已授权 ID，不加载全量范围。非空候选在返回前复验凭证，身份、权限、映射或仓储错误直接返回；空候选直接返回空集合。
    pub async fn ids_in(
        &self,
        actor: &AuditActor,
        ids: &[String],
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<Vec<String>> {
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let compiled = self.service.query(actor, &self.permission, executor).await?;
        let ids = authorized_ids_in(&self.db, &compiled, ids, executor).await?;
        self.service.validate(&compiled, executor).await?;
        Ok(ids)
    }

    /// 在原业务事务对实际仓库核验动作、同角色附加资格及对象范围；仓库缺失、无权或授权事实变化时拒绝，不能用先前读到的 ID 替代。
    pub async fn require(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<()> {
        self.service.require(actor, &self.permission, AccessTarget::record(id)?, executor).await
    }

    /// 检查完整授权条件下是否至少存在一个真实仓库，并在交付结果前复验凭证。身份、映射、仓储或授权依赖失败时返回原错误，不当作无范围。
    pub async fn has_any(&self, actor: &AuditActor, executor: &mut dyn Executor) -> IdentityResult<bool> {
        let query = self.query(actor, executor).await?;
        query.validate(executor).await?;
        Ok(!query.is_empty())
    }

    /// 在原读取事务编译完整仓库授权，并最多读一条身份判断范围是否为空，不预载全量 ID。返回绑定当前读取边界的凭证；身份、权限、映射、仓储或边界复验失败时直接返回。
    pub async fn query(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<WarehouseQuery> {
        let compiled = self.service.query(actor, &self.permission, executor).await?;
        let has_rows = authorized_rows_exist(&self.db, &compiled, executor).await?;
        compiled.ensure_current(executor)?;
        Ok(WarehouseQuery { has_rows, authorization: self.service.clone(), compiled: Arc::new(compiled) })
    }
}

/// 当前仓库查询的完整凭证；目录和列表不通过授权 ID 集合执行筛选。
#[derive(Clone)]
pub struct WarehouseQuery {
    has_rows: bool,
    authorization: Arc<AuthorizationService>,
    compiled: Arc<CompiledAccess>,
}

impl WarehouseQuery {
    // 交给仓储执行统一授权编译出的全部聚合阶段；必须放在分页和数量上限之前，以免截断授权结果。
    pub(crate) fn stages(&self) -> &[Document] {
        self.compiled.stages()
    }

    // 提供绑定本次身份与映射依赖的范围版本，供后续页判断是否继续沿用该范围。
    pub(crate) fn version(&self) -> String {
        self.compiled.version()
    }

    // 提供本次查询冻结的权限版本、组织版本和授权时点，供构造响应元信息；有效性仍由 `validate` 复验。
    pub(crate) fn metadata(&self) -> (u64, u64, Instant) {
        (self.compiled.policy_version(), self.compiled.organization_version(), self.compiled.as_of())
    }

    /// 判断完整授权条件是否未命中任何实际仓库；这是有界存在性查询的结果，不用于解释权限规则。
    pub fn is_empty(&self) -> bool {
        !self.has_rows
    }

    // 在原执行器上无 I/O 核对仓库模型、事务、读取阶段和有效期；资源、集合、行标识或输出粒度不符，或凭证失效时拒绝。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> IdentityResult<()> {
        ensure_warehouse_query(&self.compiled, executor)
    }

    // 向统一授权服务复验绑定原读取事务的凭证，供业务取数前后核对授权依赖；授权或版本失效时保留原错误。
    pub(crate) async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

// 为仓库查询共用固定模型与读取边界校验，不访问数据库；模型错配、跨事务、跨读取阶段或凭证到期均拒绝。
pub(crate) fn ensure_warehouse_query(
    access: &CompiledAccess,
    executor: &mut dyn Executor,
) -> IdentityResult<()> {
    warehouse_model(access.model())?;
    access.ensure_current(executor)
}

// 只接受 `warehouse` 或 `inventory_warehouse` 对真实仓库集合、`id` 和对象粒度的映射，防止借其它资源或受限输出读取仓库身份。
fn warehouse_model(model: &QueryModel) -> IdentityResult<()> {
    if !matches!(model.resource.as_str(), "warehouse" | "inventory_warehouse")
        || model.collection != <Database as WarehouseExt>::WAREHOUSES
        || model.id_field != "id"
        || model.output != QueryOutput::Object
    {
        return Err(IdentityError::ValidationError("仓库查询授权模型不匹配".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_identity::entity::access_control::{ResolvedScope, ScopeClause};
    use erp_identity::testing::fixed_authorization;
    use erp_permissions::{internal, inventory, warehouse};
    use mongodb::Client;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::WarehouseData;
    use crate::entity::warehouse::WarehouseId;

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
            permission: warehouse::LIST,
            resource: "warehouse".into(),
            model: None,
            as_of: Instant::from_unix_secs(1),
        }
    }

    async fn compiled(model: QueryModel, executor: &mut dyn Executor) -> WarehouseQuery {
        let authorization = fixed_authorization(
            &[warehouse::LIST],
            ResolvedScope {
                role_clauses: vec![ScopeClause { company: true, ..Default::default() }],
                user_limit: None,
            },
            vec![],
            vec![(warehouse::LIST, Arc::new(WarehouseMapping(model)))],
        )
        .unwrap();
        let compiled = authorization.query(&context().actor, &warehouse::LIST, executor).await.unwrap();
        WarehouseQuery { has_rows: true, authorization, compiled: Arc::new(compiled) }
    }

    #[test]
    fn warehouse_model_requires_fixed_resource_collection_identity_and_object_output() {
        let model = mapping().0;
        warehouse_model(&model).unwrap();
        warehouse_model(&QueryModel { resource: "inventory_warehouse".into(), ..model.clone() }).unwrap();
        for invalid in [
            QueryModel { resource: "supplier".into(), ..model.clone() },
            QueryModel { collection: "other_warehouses".into(), ..model.clone() },
            QueryModel { id_field: "warehouse_id".into(), ..model.clone() },
            QueryModel { output: QueryOutput::Share { fields: ["id".into()].into() }, ..model.clone() },
            QueryModel { output: QueryOutput::Existence { fields: ["id".into()].into() }, ..model.clone() },
        ] {
            assert!(matches!(warehouse_model(&invalid), Err(IdentityError::ValidationError(_))));
        }
    }

    #[tokio::test]
    async fn warehouse_read_entry_points_reject_wrong_models_and_stale_boundaries() {
        let db = Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("warehouse_query_guard");
        let mut executor = ReadExecutor::default();
        let correct = compiled(mapping().0, &mut executor).await;
        correct.ensure_current(&mut executor).unwrap();

        let wrong =
            compiled(QueryModel { collection: "other_warehouses".into(), ..mapping().0 }, &mut executor)
                .await;
        assert!(matches!(wrong.ensure_current(&mut executor), Err(IdentityError::ValidationError(_))));
        assert!(matches!(
            authorized_rows_exist(&db, &wrong.compiled, &mut executor).await,
            Err(IdentityError::ValidationError(_))
        ));
        assert!(matches!(
            authorized_ids_in(&db, &wrong.compiled, &["warehouse".into()], &mut executor).await,
            Err(IdentityError::ValidationError(_))
        ));

        executor.next_read();
        assert!(matches!(correct.ensure_current(&mut executor), Err(IdentityError::ValidationError(_))));
        assert!(matches!(
            authorized_rows_exist(&db, &correct.compiled, &mut executor).await,
            Err(IdentityError::ValidationError(_))
        ));
        assert!(matches!(
            authorized_ids_in(&db, &correct.compiled, &[], &mut executor).await,
            Err(IdentityError::ValidationError(_))
        ));
    }
    #[test]
    fn warehouse_fact_uses_real_identity_and_rejects_unsupported_dimensions() {
        let mapping = mapping();
        let context = context();
        let prepared = None;
        let fragment = mapping
            .predicate(&AccessFact::WarehouseIn(["wh-a".into(), "wh-b".into()].into()), &context, prepared)
            .unwrap();
        assert!(
            matches!(fragment, QueryFragment::Match(filter) if filter == doc! {"id":{"$in":["wh-a","wh-b"]}})
        );
        assert!(mapping.predicate(&AccessFact::Owned, &context, prepared).is_err());
        assert!(mapping.relation(&RelationKey::parse("source").unwrap(), &context, prepared).is_err());
    }
    fn inventory_entries() -> [(Permission, Permission); 8] {
        [
            (internal::INVENTORY_BALANCE_WAREHOUSES, inventory::STOCK_BALANCE_LIST),
            (internal::INVENTORY_BALANCE_DETAIL_WAREHOUSES, inventory::STOCK_BALANCE_DETAIL),
            (internal::INVENTORY_MOVEMENT_WAREHOUSES, inventory::STOCK_MOVEMENT_LIST),
            (internal::INVENTORY_RESERVATION_WAREHOUSES, inventory::STOCK_RESERVATION_LIST),
            (internal::INVENTORY_ADJUSTMENT_WAREHOUSES, inventory::STOCK_ADJUSTMENT_LIST),
            (internal::INVENTORY_ADJUSTMENT_DETAIL_WAREHOUSES, inventory::STOCK_ADJUSTMENT_DETAIL),
            (internal::INVENTORY_CREATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_CREATE),
            (internal::INVENTORY_UPDATE_WAREHOUSES, inventory::STOCK_ADJUSTMENT_UPDATE),
        ]
    }

    #[tokio::test]
    async fn inventory_bindings_keep_missing_collaboration_empty_and_preserve_real_warehouse_scope() {
        for (entry, source) in inventory_entries() {
            for has_warehouse_scope in [false, true] {
                let mut context = context();
                context.permission = entry.clone();
                context.resource = entry.resource().into();
                let row = Warehouse::new(WarehouseId::new("warehouse"), WarehouseData::new("WH-1"), "actor")
                    .unwrap();
                let facts = WarehouseFacts::object(&context, row);
                assert_eq!(facts.collaborators, FactValue::Known(Default::default()));

                let mut clauses = vec![ScopeClause { collaborative: true, ..Default::default() }];
                if has_warehouse_scope {
                    clauses.push(ScopeClause {
                        warehouse_ids: ["warehouse".into()].into(),
                        ..Default::default()
                    });
                }
                let mapping = WarehouseMapping::shared(entry.resource()).unwrap();
                let authorization = fixed_authorization(
                    &[source.clone(), inventory::STOCK_ADJUSTMENT_DETAIL, entry.clone()],
                    ResolvedScope { role_clauses: clauses, user_limit: None },
                    vec![facts],
                    vec![(entry.clone(), mapping)],
                )
                .unwrap();
                let mut executor = SnapshotExecutor::default();

                let result = authorization
                    .require(
                        &context.actor,
                        &entry,
                        AccessTarget::record("warehouse").unwrap(),
                        &mut executor,
                    )
                    .await;
                if has_warehouse_scope {
                    result.unwrap();
                } else {
                    assert!(matches!(result, Err(IdentityError::Forbidden(_))));
                }

                let access = authorization.query(&context.actor, &entry, &mut executor).await.unwrap();
                warehouse_model(access.model()).unwrap();
                authorization.validate(&access, &mut executor).await.unwrap();
            }
        }
    }

    #[test]
    fn warehouse_collaboration_query_matches_known_empty_object_facts() {
        let mapping = WarehouseMapping::shared("inventory_warehouse").unwrap();
        assert!(mapping.model().facts.contains(&FactKind::Collaborators));
        let fragment = mapping.predicate(&AccessFact::Collaborating, &context(), None).unwrap();
        assert!(matches!(fragment, QueryFragment::Match(filter) if filter == doc! {"$expr": false}));
    }

    #[test]
    fn model_records_exclude_deleted_warehouse_identities() {
        let mapping = mapping();
        assert_eq!(mapping.key(&context(), None).unwrap(), Bson::String("$id".into()));
        assert!(
            matches!(mapping.records(&context(), None).unwrap(), QueryFragment::Match(filter) if filter == doc! {"deleted_at":0})
        );
    }
}
