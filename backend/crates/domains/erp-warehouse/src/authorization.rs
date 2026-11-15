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
    AccessFactsPort, AccessMapping, AccessTarget, AuthorizationService, CompiledAccess, ResourceTargets,
};
use erp_permissions::Permission;
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

pub struct WarehouseFacts(Database);
impl WarehouseFacts {
    /// 构造本域仓库事实读取器。
    /// # 参数
    /// db 为拥有仓库集合的数据库。
    /// # 返回
    /// 供统一授权装配复用的共享事实读取器。
    /// # 错误
    /// 构造不执行 I/O。
    pub fn shared(db: Database) -> Arc<dyn AccessFactsPort> {
        Arc::new(Self(db))
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
    /// 登记一个固定仓库身份入口。
    /// # 参数
    /// db 为基础设施，service 为共享授权服务；permission 为固定入口权限。
    /// # 返回
    /// 不执行 I/O 的查询入口。
    /// # 错误
    /// 构造不执行 I/O；固定权限及映射由共享服务在启动时校验。
    pub fn new(db: Database, service: Arc<AuthorizationService>, permission: Permission) -> Self {
        Self { db, service, permission }
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
        let compiled = self.service.query(actor, &self.permission, executor).await?;
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
        self.service.require(actor, &self.permission, AccessTarget::record(id)?, executor).await
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

    /// 核验当前事务、读取阶段及有效期，不读取数据库。
    pub(crate) fn ensure_current(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.compiled.ensure_current(executor)
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

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;

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
    #[test]
    fn model_records_exclude_deleted_warehouse_identities() {
        let mapping = mapping();
        assert_eq!(mapping.key(&context(), None).unwrap(), Bson::String("$id".into()));
        assert!(
            matches!(mapping.records(&context(), None).unwrap(), QueryFragment::Match(filter) if filter == doc! {"deleted_at":0})
        );
    }
}
