//! 仓库身份作为库存和独立目录的真实授权模型。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, MappingFacts, QueryModel, QueryModelKey, QueryOutput, ReadAccess,
    RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AuthorizationRegistry,
    AuthorizationService, CompiledAccess, DataAccess, Permission, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, Document, doc};
use persistence_core::Executor;

use crate::repository::WarehouseExt;
use crate::repository::directory::{authorized_ids, authorized_rows_exist};

/// 只把仓库业务字段映射为 identity 事实，不组合权限规则。
struct WarehouseMapping(QueryModel);

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
        let (registry, mapping) = registration(&permission, required)?;
        Ok(Self {
            db: db.clone(),
            service: AuthorizationService::new(db, rbac, registry),
            mapping,
            permission,
        })
    }

    /// 查询完整已授权仓库身份集合，前后由 identity 复验。
    /// # 参数
    /// actor 为真实会话账号；executor 为调用方事务。
    /// # 返回
    /// 已授权且实际存在的仓库 ID 及完整查询版本。
    /// # 错误
    /// 身份、权限、映射、数据库或版本变化错误透传。
    pub async fn ids(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> erp_identity::Result<WarehouseRead> {
        let query = self.service.query(actor, &self.permission, AccessSubject::Direct, executor).await?;
        let compiled = query.compile(self.mapping.as_ref(), executor).await?;
        self.service.validate(&compiled, executor).await?;
        let ids = authorized_ids(&self.db, &compiled, executor).await?;
        self.service.validate(&compiled, executor).await?;
        Ok(WarehouseRead { ids, authorization: self.service.clone(), compiled: Arc::new(compiled) })
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

    /// 判断完整授权查询是否没有实际仓库。
    /// # 参数
    /// 无。
    /// # 返回
    /// 完整查询的有界存在性结果，不解释权限规则。
    /// # 错误
    /// 无。
    pub(crate) fn is_empty(&self) -> bool {
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

/// 当前读取阶段的仓库身份结果及不可伪造凭证。
#[derive(Clone)]
pub struct WarehouseRead {
    ids: Vec<String>,
    authorization: AuthorizationService,
    compiled: Arc<CompiledAccess>,
}
impl WarehouseRead {
    /// 返回已经授权的真实仓库身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 当前读取阶段的完整身份集合。
    /// # 错误
    /// 无；使用结果前后仍须复验。
    pub fn ids(&self) -> &[String] {
        &self.ids
    }
    /// 返回完整授权查询版本。
    /// # 参数
    /// 无。
    /// # 返回
    /// 身份及业务映射依赖指纹。
    /// # 错误
    /// 无。
    pub fn version(&self) -> String {
        self.compiled.version()
    }
    /// 在实际业务读取前后复验原凭证。
    /// # 参数
    /// executor 必须是原读取事务。
    /// # 返回
    /// 身份、权限及有效期仍一致时成功。
    /// # 错误
    /// 变化返回 DATA_SCOPE_CHANGED，其余错误原样传播。
    pub async fn validate(&self, executor: &mut dyn Executor) -> erp_identity::Result<()> {
        self.authorization.validate(&self.compiled, executor).await
    }
}

/// 固定入口登记仅表达仓库事实和同角色附加功能要求。
fn registration(
    permission: &Permission,
    required: Vec<Permission>,
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
        access: AccessMode::Query { data, mapping: mapping.clone() },
    }])?;
    Ok((registry, mapping))
}

/// 机械保留身份错误的分类和载荷。
pub(crate) fn map_identity_error(error: erp_identity::Error) -> crate::Error {
    match error {
        erp_identity::Error::Internal(value) => crate::Error::Internal(value),
        erp_identity::Error::NotFound(value) => crate::Error::NotFound(value),
        erp_identity::Error::ValidationError(value) => crate::Error::ValidationError(value),
        erp_identity::Error::InvalidInput(value) => crate::Error::InvalidInput(value),
        erp_identity::Error::BusinessLogicError(value) => crate::Error::BusinessLogicError(value),
        erp_identity::Error::ConflictError(value) => crate::Error::ConflictError(value),
        erp_identity::Error::ReceiptDuplicate(value) => crate::Error::ReceiptDuplicate(value),
        erp_identity::Error::TransientTransaction(value) => crate::Error::TransientTransaction(value),
        erp_identity::Error::Forbidden(value) => crate::Error::Forbidden(value),
        erp_identity::Error::Unauthenticated(value) => crate::Error::Unauthenticated(value),
        erp_identity::Error::Logic(value) => crate::Error::Logic(value),
        erp_identity::Error::OutcomeUnknown(value) => crate::Error::OutcomeUnknown(value),
        erp_identity::Error::RepositoryError(value) => crate::Error::RepositoryError(value),
        erp_identity::Error::Rbac(value) => crate::Error::Internal(value),
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
            assert!(registration(&Permission::parse(code).unwrap(), required).is_ok(), "{code}");
        }
        assert!(registration(&Permission::parse("warehouse:update").unwrap(), Vec::new()).is_err());
    }
}
