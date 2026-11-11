//! 后台任务以真实发起人为所有者；管理员分支由 identity 检查启用角色。
use std::sync::Arc;

use application_core::AuditActor;
use async_trait::async_trait;
use erp_core::ids::BackgroundJobId;
use erp_identity::entity::authorization::{
    AccessContext, AccessFact, FactKind, FactValue, MappingFacts, ObjectFacts, QueryModel, QueryModelKey,
    QueryOutput, RelationKey,
};
use erp_identity::ports::{QueryFragment, RelationMapping};
use erp_identity::{
    AccessFactsPort, AccessMapping, AccessMode, AccessPolicy, AccessRegistration, AccessSubject,
    AccessTarget, AuthorizationRegistry, AuthorizationService, CompiledAccess, DataAccess,
    Error as IdentityError, Permission, ResourceTargets, Result as IdentityResult, RoleId, SharedRbacService,
};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{BackgroundJob, BulkJobExt, Result};

#[derive(Clone)]
pub(crate) struct BackgroundAuthorization {
    service: AuthorizationService,
    mapping: Arc<dyn AccessMapping>,
}

impl BackgroundAuthorization {
    /// 装配固定任务目录，不从请求接受政策。
    /// # 参数
    /// db、identity 为应用共享身份设施。
    /// # 返回
    /// 未执行 I/O 的任务授权服务。
    /// # 错误
    /// 固定登记损坏时启动失败。
    pub(crate) fn new(db: Database, identity: SharedRbacService) -> Self {
        let mapping: Arc<dyn AccessMapping> = Arc::new(JobMapping(model()));
        let facts: Arc<dyn AccessFactsPort> = Arc::new(JobFacts(db.clone()));
        let registry = registry(mapping.clone(), facts).expect("固定后台任务授权目录");
        Self { service: AuthorizationService::new(db, identity, registry), mapping }
    }

    /// 在调用方事务编译任务中心完整授权条件。
    /// # 参数
    /// actor 为当前账号，executor 为列表读取事务。
    /// # 返回
    /// identity 编译的任务查询凭证。
    /// # 错误
    /// 账号、权限、登记或事实取证失败时拒绝。
    pub(crate) async fn query(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        let query = self
            .service
            .query(actor, &Permission::parse("background_job:list")?, AccessSubject::Direct, executor)
            .await?;
        Ok(query.compile(self.mapping.as_ref(), executor).await?)
    }

    /// 复验读取阶段的完整身份与映射凭证。
    /// # 参数
    /// access 为本服务产生的凭证，executor 沿用原事务。
    /// # 返回
    /// 授权依赖保持有效时成功。
    /// # 错误
    /// 撤权、身份或事实版本变化时拒绝。
    pub(crate) async fn validate(&self, access: &CompiledAccess, executor: &mut dyn Executor) -> Result<()> {
        Ok(self.service.validate(access, executor).await?)
    }

    /// 检查真实后台任务的当前动作资格。
    /// # 参数
    /// permission 为固定动作，id 为任务主键，actor、executor 为本阶段账号与事务。
    /// # 返回
    /// 当前账号具有入口资格且拥有任务或启用管理员角色时成功。
    /// # 错误
    /// 对象缺失、无权或身份设施故障时拒绝。
    pub(crate) async fn require(
        &self,
        permission: &str,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(
                actor,
                &Permission::parse(permission)?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }

    /// 批量取消前在每个写事务重验启用管理员角色和入口权限。
    /// # 参数
    /// actor、executor 为当前账号与取消事务。
    /// # 返回
    /// identity 证明管理员批量取消资格时成功。
    /// # 错误
    /// 角色停用、权限不足或设施读取失败时拒绝。
    pub(crate) async fn require_cancel_all(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(
                actor,
                &Permission::parse("background_job:cancel_all")?,
                AccessTarget::Operation,
                executor,
            )
            .await?)
    }
}

/// 原管理员范围固定在服务端政策，启用与有效性由 identity 解释。
fn administrators() -> AccessPolicy {
    AccessPolicy::Roles(vec![
        RoleId::parse("role-root").expect("固定根角色"),
        RoleId::parse("role-sysadmin").expect("固定系统管理员角色"),
    ])
}

/// 将入口资格、本人和启用管理员政策绑定到真实任务模型。
fn registry(
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
) -> IdentityResult<AuthorizationRegistry> {
    let mut entries = Vec::new();
    for code in
        ["background_job:list", "background_job:detail", "background_job:cancel", "background_job_item:list"]
    {
        let data = DataAccess {
            subject: AccessSubject::Direct,
            required_facts: [FactKind::Owner].into(),
            policy: AccessPolicy::Any(vec![AccessPolicy::Owned, administrators()]),
        };
        let access = if code == "background_job:list" {
            AccessMode::Query { data, mapping: mapping.clone() }
        } else {
            AccessMode::Object { data, facts: facts.clone() }
        };
        entries.push(AccessRegistration {
            permission: Permission::parse(code)?,
            required_permissions: vec![],
            access,
        });
    }
    entries.push(AccessRegistration {
        permission: Permission::parse("background_job:cancel_all")?,
        required_permissions: vec![],
        access: AccessMode::QualifiedOperation { policy: administrators() },
    });
    AuthorizationRegistry::new(entries)
}

/// 后台任务所有者只映射到不可变发起人字段。
fn model() -> QueryModel {
    QueryModel {
        key: QueryModelKey::parse("background_jobs").expect("固定后台任务模型"),
        resource: "background_job".into(),
        collection: Database::BACKGROUND_JOBS.into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [FactKind::Owner].into(),
    }
}

struct JobMapping(QueryModel);
#[async_trait]
impl AccessMapping for JobMapping {
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
        Ok(QueryFragment::Match(doc! {"deleted_at": 0}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"requested_by": context.actor.id()})),
            _ => Err(IdentityError::ValidationError("后台任务仅提供发起人事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("后台任务没有授权来源关系".into()))
    }
}

struct JobFacts(Database);
#[async_trait]
impl AccessFactsPort for JobFacts {
    async fn load(
        &self,
        context: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let ids = ids.ids().iter().map(|id| BackgroundJobId::new(id.clone())).collect::<Vec<_>>();
        Ok(self
            .0
            .background_jobs()
            .find_by_ids(&ids, executor)
            .await?
            .iter()
            .map(|job| job_facts(job, &context.resource))
            .collect())
    }
}
/// 从实际任务及版本生成根对象事实，不能由请求提供所有者结论。
fn job_facts(job: &BackgroundJob, resource: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id: job.base.id.clone(),
        version: job.base.version.to_string(),
        owner_id: FactValue::Known(Some(job.requested_by.clone())),
        ..Default::default()
    }
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;

    use super::*;
    use crate::{BackgroundJobData, JobType};

    fn context(actor: &str) -> AccessContext {
        AccessContext {
            actor: AuditActor::new(actor.into(), actor.into(), AccountKind::Admin),
            permission: Permission::parse("background_job:list").unwrap(),
            resource: "background_job".into(),
            model: Some(model().key),
            as_of: Instant::now(),
        }
    }

    #[test]
    fn mapping_and_object_share_actual_requester_ownership() {
        let job = BackgroundJob::new(
            BackgroundJobId::new("job"),
            BackgroundJobData {
                job_no: "JOB-1".into(),
                job_type: JobType::Import,
                domain_job_type: None,
                domain_job_id: None,
                selection_snapshot_id: None,
                requested_by: "owner".into(),
                request_id: "request".into(),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1,
            },
        )
        .unwrap();
        let facts = job_facts(&job, "background_job");
        assert_eq!(facts.owner_id, FactValue::Known(Some("owner".into())));
        assert_eq!(facts.id, job.base.id);
        assert_eq!(facts.version, job.base.version.to_string());
        assert_eq!(facts.resource, "background_job");
        let predicate = JobMapping(model())
            .predicate(&AccessFact::Owned, &context("owner"), &MappingFacts::default())
            .unwrap();
        let QueryFragment::Match(document) = predicate else {
            panic!("本人范围应为真实发起人条件")
        };
        assert_eq!(document, doc! {"requested_by": "owner"});
        let other = JobMapping(model())
            .predicate(&AccessFact::Owned, &context("other"), &MappingFacts::default())
            .unwrap();
        let QueryFragment::Match(other_document) = other else {
            panic!("本人范围应为真实发起人条件")
        };
        assert_eq!(other_document, doc! {"requested_by": "other"});
    }

    #[test]
    fn mapping_rejects_unsupported_fact_dimensions() {
        assert!(
            JobMapping(model())
                .predicate(&AccessFact::AssignedTo, &context("owner"), &MappingFacts::default())
                .is_err()
        );
        assert_eq!(model().facts, [FactKind::Owner].into());
    }
}
