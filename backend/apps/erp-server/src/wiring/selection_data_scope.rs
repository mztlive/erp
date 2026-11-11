//! 选品统一授权登记；业务映射和事实由拥有领域提供。
use std::collections::BTreeSet;
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_identity::entity::authorization::{FactKind, ReadAccess};
use erp_identity::ports::{AccessFactsPort, AccessMapping};
use erp_identity::{
    AccessMode, AccessPolicy, AccessRegistration, AccessSubject, AccessTarget, AuthorizationRegistry,
    AuthorizationService, DataAccess, Permission, ResourceTargets, SharedRbacService,
};
use erp_sales::ports::{SelectionDataScopePort, SelectionResolvedScope};
use erp_sales::repository::sales_selection::authorization::{SelectionFacts, SelectionMapping};
use erp_sales::service::sales_selection::SelectionAccess;
use mongodb::Database;
use persistence_core::Executor;

use super::identity_error::map_identity_error;
use super::scope_support;
map_identity_error!(erp_sales);

type SelectionAuthorization = (AuthorizationService, Arc<dyn AccessMapping>, Arc<dyn AccessMapping>);

/// 组合层固定选品目录及事实端口。
#[derive(Clone)]
pub struct MongoSelectionDataScope {
    db: Database,
    rbac: SharedRbacService,
}
impl MongoSelectionDataScope {
    /// 绑定身份设施。
    /// # 参数
    /// db 为数据库，rbac 为身份策略设施。
    /// # 返回
    /// 不执行 I/O 的适配器。
    /// # 错误
    /// 无。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }
    /// 构造选品域共享端口。
    /// # 参数
    /// db、rbac 为应用共用身份设施。
    /// # 返回
    /// 已接线选品端口。
    /// # 错误
    /// 无。
    pub fn shared(db: Database, rbac: SharedRbacService) -> Arc<dyn SelectionDataScopePort> {
        Arc::new(Self::new(db, rbac))
    }
    fn service(&self, facts: Arc<dyn AccessFactsPort>) -> erp_identity::Result<SelectionAuthorization> {
        let booklet: Arc<dyn AccessMapping> = Arc::new(SelectionMapping::new("sales_selection_booklet")?);
        let proposal: Arc<dyn AccessMapping> = Arc::new(SelectionMapping::new("sales_selection_proposal")?);
        let mut entries = Vec::new();
        for (resource, actions, mapping) in [
            (
                "sales_selection_booklet",
                &[
                    "list",
                    "get",
                    "create",
                    "maintain",
                    "prepare",
                    "publish",
                    "copy_link",
                    "rotate_link",
                    "close",
                    "revoke",
                    "void",
                ][..],
                &booklet,
            ),
            ("sales_selection_proposal", &["list", "get"][..], &proposal),
        ] {
            for action in actions {
                entries.push(registration(resource, action, mapping.clone(), facts.clone())?);
            }
        }
        Ok((
            AuthorizationService::new(
                self.db.clone(),
                self.rbac.clone(),
                AuthorizationRegistry::new(entries)?,
            ),
            booklet,
            proposal,
        ))
    }
}
#[async_trait]
impl SelectionDataScopePort for MongoSelectionDataScope {
    async fn resolve(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<SelectionResolvedScope> {
        let (authorization, booklet, proposal) =
            self.service(Arc::new(SelectionFacts::new(self.db.clone()))).map_err(map_identity_error)?;
        let mapping = match resource {
            "sales_selection_booklet" => booklet,
            "sales_selection_proposal" => proposal,
            _ => return Err(erp_sales::Error::ValidationError("非选品资源".into())),
        };
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        let query = authorization
            .query(actor, &permission, AccessSubject::Direct, executor)
            .await
            .map_err(map_identity_error)?;
        let compiled = query.compile(mapping.as_ref(), executor).await.map_err(map_identity_error)?;
        authorization.validate(&compiled, executor).await.map_err(map_identity_error)?;
        Ok(SelectionResolvedScope {
            user_id: actor.id().into(),
            resource: resource.into(),
            action: action.into(),
            scope_version: compiled.version(),
            as_of: Instant::now(),
            access: Arc::new(compiled),
            authorization,
        })
    }
    async fn require(
        &self,
        actor: &AuditActor,
        resource: &str,
        action: &str,
        id: &str,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<()> {
        let (authorization, _, _) =
            self.service(Arc::new(SelectionFacts::new(self.db.clone()))).map_err(map_identity_error)?;
        let permission = Permission::parse(format!("{resource}:{action}"))?;
        let ids = ResourceTargets::new(&[id]).map_err(map_identity_error)?;
        authorization
            .require(
                actor,
                &permission,
                AccessTarget::Records { subject: AccessSubject::Direct, ids },
                executor,
            )
            .await
            .map_err(map_identity_error)
    }
    async fn require_new(
        &self,
        actor: &AuditActor,
        owner: &str,
        org: &str,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<()> {
        let (authorization, _, _) = self
            .service(Arc::new(SelectionFacts::proposed(self.db.clone(), owner, org)))
            .map_err(map_identity_error)?;
        let permission = Permission::parse("sales_selection_booklet:create")?;
        let ids = ResourceTargets::new(&["selection_new"]).map_err(map_identity_error)?;
        authorization
            .require(
                actor,
                &permission,
                AccessTarget::Records { subject: AccessSubject::Direct, ids },
                executor,
            )
            .await
            .map_err(map_identity_error)
    }
    async fn expand_org_units(
        &self,
        org_unit_ids: &[String],
        include_descendants: bool,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<BTreeSet<String>> {
        scope_support::expand_org_units(
            &self.db,
            org_unit_ids,
            include_descendants,
            executor,
            Into::into,
            map_identity_error,
        )
        .await
    }

    async fn org_member_ids(
        &self,
        org_unit_ids: &BTreeSet<String>,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<Vec<String>> {
        scope_support::org_member_ids(&self.db, org_unit_ids, at, executor, Into::into).await
    }

    async fn own_org(
        &self,
        user_id: &str,
        at: Instant,
        executor: &mut dyn Executor,
    ) -> erp_sales::Result<Option<String>> {
        scope_support::own_org(&self.db, user_id, at, executor, Into::into, map_identity_error).await
    }
}
/// 创建已接线选品授权服务。
/// # 参数
/// db、rbac 为应用共享设施。
/// # 返回
/// 选品用例授权入口。
/// # 错误
/// 构造不执行 I/O。
pub fn selection_access(db: Database, rbac: SharedRbacService) -> SelectionAccess {
    SelectionAccess::new(db.clone(), MongoSelectionDataScope::shared(db, rbac))
}

/// 固定目录将方案责任解释为当前所属册责任；不增加来源菜单资格。
fn registration(
    resource: &str,
    action: &str,
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
) -> erp_identity::Result<AccessRegistration> {
    Ok(AccessRegistration {
        permission: Permission::parse(format!("{resource}:{action}"))?,
        required_permissions: vec![],
        access: AccessMode::QueryAndObject {
            data: DataAccess {
                subject: AccessSubject::Direct,
                required_facts: [FactKind::Owner, FactKind::Organization].into(),
                policy: AccessPolicy::Scope {
                    permission: Permission::parse(format!("{resource}:{action}"))?,
                    read: ReadAccess::default(),
                },
            },
            mapping,
            facts,
        },
    })
}
