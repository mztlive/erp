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
    AccessFactsPort, AccessMapping, AccessTarget, AuthorizationService, CompiledAccess,
    Error as IdentityError, Permission, ResourceTargets, Result as IdentityResult,
};
use erp_permissions::{bulk_job, internal};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::repository::prelude::*;
use crate::{BackgroundJob, BulkJobExt, Result};

#[derive(Clone)]
pub(crate) struct BackgroundAuthorization {
    pub(crate) service: Arc<AuthorizationService>,
}

impl BackgroundAuthorization {
    // 装配固定任务目录，不从请求接受政策。
    // 未执行 I/O 的任务授权服务。
    // db、identity 为应用共享身份设施。
    // 固定登记损坏时启动失败。
    pub(crate) fn new(service: Arc<AuthorizationService>) -> Self {
        Self { service }
    }

    // 在调用方事务编译任务中心完整授权条件。
    // identity 编译的任务查询凭证。
    // actor 为当前账号，executor 为列表读取事务。
    // 账号、权限、登记或事实取证失败时拒绝。
    pub(crate) async fn query(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CompiledAccess> {
        let compiled = self.service.query(actor, &bulk_job::BACKGROUND_JOB_LIST.clone(), executor).await?;
        Ok(compiled)
    }

    // 检查真实后台任务的当前动作资格。
    // 当前账号具有入口资格且拥有任务或启用管理员角色时成功。
    // permission 为固定动作，id 为任务主键，actor、executor 为本阶段账号与事务。
    // 对象缺失、无权或身份设施故障时拒绝。
    pub(crate) async fn require(
        &self,
        permission: &Permission,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self.service.require(actor, permission, AccessTarget::record(id)?, executor).await?)
    }

    // 批量取消前在每个写事务重验启用管理员角色和入口权限。
    // identity 证明管理员批量取消资格时成功。
    // actor、executor 为当前账号与取消事务。
    // 角色停用、权限不足或设施读取失败时拒绝。
    pub(crate) async fn require_cancel_all(
        &self,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(actor, &bulk_job::BACKGROUND_JOB_CANCEL_ALL.clone(), AccessTarget::Operation, executor)
            .await?)
    }
}

/// 构造后台任务真实发起人 Facts。
/// 统一读取器。
/// db 为后台任务数据库。
/// 构造不执行 I/O。
pub fn facts(db: Database) -> Arc<dyn AccessFactsPort> {
    Arc::new(JobFacts(db))
}
/// 构造后台任务静态查询映射。
/// 固定后台任务模型。
/// 构造不执行 I/O。
pub fn mapping() -> Arc<dyn AccessMapping> {
    Arc::new(JobMapping(model()))
}

// 后台任务所有者只映射到不可变发起人字段。
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
    fn key(&self, _: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<Bson> {
        Ok("$id".into())
    }
    fn records(&self, context: &AccessContext, _: Option<&MappingFacts>) -> IdentityResult<QueryFragment> {
        let mut filter = doc! {"deleted_at": 0};
        if let Some(domain) = job_domain(&context.permission)? {
            filter.insert("domain_job_type", domain);
        }
        Ok(QueryFragment::Match(filter))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: Option<&MappingFacts>,
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
        _: Option<&MappingFacts>,
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
        let domain = job_domain(&context.permission)?;
        let ids = ids.ids().iter().map(|id| BackgroundJobId::new(id.clone())).collect::<Vec<_>>();
        Ok(self
            .0
            .background_jobs()
            .find_by_ids(&ids, executor)
            .await?
            .iter()
            .filter(|job| domain.is_none_or(|domain| job.domain_job_type.as_deref() == Some(domain)))
            .map(|job| job_facts(job, &context.resource))
            .collect())
    }
}
// 从实际任务及版本生成根对象事实，不能由请求提供所有者结论。
fn job_facts(job: &BackgroundJob, resource: &str) -> ObjectFacts {
    ObjectFacts {
        resource: resource.into(),
        id: job.base.id.clone(),
        version: job.base.version.to_string(),
        owner_id: FactValue::Known(Some(job.requested_by.clone())),
        ..Default::default()
    }
}

fn job_domain(permission: &Permission) -> IdentityResult<Option<&'static str>> {
    for (entry, domain) in [
        (internal::PRODUCT_IMPORT_JOB_READ, "PRODUCT_IMPORT"),
        (internal::SUPPLIER_IMPORT_JOB_READ, "SUPPLIER_IMPORT"),
        (internal::BRAND_IMPORT_JOB_READ, "BRAND_IMPORT"),
        (internal::DATA_TRANSFER_JOB_READ, "DATA_TRANSFER"),
    ] {
        if permission == &entry {
            return Ok(Some(domain));
        }
    }

    if [
        bulk_job::BACKGROUND_JOB_LIST,
        bulk_job::BACKGROUND_JOB_DETAIL,
        bulk_job::BACKGROUND_JOB_CANCEL,
        bulk_job::BACKGROUND_JOB_CANCEL_ALL,
        bulk_job::BACKGROUND_JOB_ITEM_LIST,
    ]
    .contains(permission)
    {
        return Ok(None);
    }
    Err(IdentityError::ValidationError("后台任务事实未接入该权限".into()))
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
            permission: bulk_job::BACKGROUND_JOB_LIST.clone(),
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

        let predicate = JobMapping(model()).predicate(&AccessFact::Owned, &context("owner"), None).unwrap();
        let QueryFragment::Match(document) = predicate else {
            panic!("本人范围应为真实发起人条件")
        };
        assert_eq!(document, doc! {"requested_by": "owner"});
        let other = JobMapping(model()).predicate(&AccessFact::Owned, &context("other"), None).unwrap();
        let QueryFragment::Match(other_document) = other else {
            panic!("本人范围应为真实发起人条件")
        };
        assert_eq!(other_document, doc! {"requested_by": "other"});
    }

    #[test]
    fn import_query_partitions_use_the_real_job_domain() {
        for (permission, domain) in [
            (internal::PRODUCT_IMPORT_JOB_READ, "PRODUCT_IMPORT"),
            (internal::SUPPLIER_IMPORT_JOB_READ, "SUPPLIER_IMPORT"),
            (internal::BRAND_IMPORT_JOB_READ, "BRAND_IMPORT"),
            (internal::DATA_TRANSFER_JOB_READ, "DATA_TRANSFER"),
        ] {
            let mut context = context("reader");
            context.permission = permission;
            let QueryFragment::Match(records) = JobMapping(model()).records(&context, None).unwrap() else {
                panic!("固定资料过滤必须为字段条件")
            };
            assert_eq!(records.get_str("domain_job_type").unwrap(), domain);
            assert_eq!(job_domain(&context.permission).unwrap(), Some(domain));
        }

        let QueryFragment::Match(records) = JobMapping(model()).records(&context("reader"), None).unwrap()
        else {
            panic!("固定资料过滤必须为字段条件")
        };
        assert!(!records.contains_key("domain_job_type"));
    }

    #[test]
    fn mapping_rejects_unsupported_fact_dimensions() {
        let mut unknown = context("owner");
        unknown.permission = Permission::parse("background_job:unknown").unwrap();
        assert!(JobMapping(model()).records(&unknown, None).is_err());
        assert!(JobMapping(model()).predicate(&AccessFact::AssignedTo, &context("owner"), None).is_err());
        assert_eq!(model().facts, [FactKind::Owner].into());
    }
}
