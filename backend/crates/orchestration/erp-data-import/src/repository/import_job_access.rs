//! 业务导入入口保留原功能权限，任务归属统一由 identity 的 Owned 政策执行。

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
    AccessTarget, AuthorizationRegistry, AuthorizationService, DataAccess, Error as IdentityError,
    Permission, ResourceTargets, Result as IdentityResult, SharedRbacService,
};
use erp_support::repository::BackgroundJobRepositoryExt;
use erp_support::{BackgroundJob, BulkJobExt};
use mongodb::Database;
use mongodb::bson::{Bson, doc};
use persistence_core::Executor;

use crate::Result;

/// 同一业务任务模型的本人查询与对象授权；不授予任务中心管理员权限。
#[derive(Clone)]
pub(crate) struct ImportJobAccess {
    pub(crate) service: AuthorizationService,
    pub(crate) mapping: Arc<dyn AccessMapping>,
}

impl ImportJobAccess {
    /// 登记固定任务类型及原业务功能权限，不读取身份或任务。
    /// # 参数
    /// db、rbac 为共享设施，domain、permission 为服务端固定任务类型与原业务权限。
    /// # 返回
    /// 绑定真实任务模型的授权入口。
    /// # 错误
    /// 固定登记非法时启动失败。
    pub(crate) fn new(
        db: Database,
        rbac: SharedRbacService,
        domain: &'static str,
        permission: Option<&str>,
    ) -> Self {
        let mapping: Arc<dyn AccessMapping> = Arc::new(JobMapping { domain, model: model() });
        let facts: Arc<dyn AccessFactsPort> = Arc::new(JobFacts { db: db.clone(), domain });
        let registry = registry(mapping.clone(), facts, permission).expect("固定导入任务授权目录合法");
        Self { service: AuthorizationService::new(db, rbac, registry), mapping }
    }

    /// 在原执行器检查真实任务归属、账号有效性及配置的业务功能权限。
    /// # 参数
    /// id 为任务 ID，actor 为本阶段账号，executor 沿用当前读取或写入事务。
    /// # 返回
    /// 该类型任务属于当前有效账号且业务功能权限成立时成功。
    /// # 错误
    /// 任务缺失、类型错误、无权或身份设施失败时拒绝。
    pub(crate) async fn require(
        &self,
        id: &str,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        Ok(self
            .service
            .require(
                actor,
                &Permission::parse("background_job:detail")?,
                AccessTarget::Records { subject: AccessSubject::Direct, ids: ResourceTargets::new(&[id])? },
                executor,
            )
            .await?)
    }
}

fn registry(
    mapping: Arc<dyn AccessMapping>,
    facts: Arc<dyn AccessFactsPort>,
    permission: Option<&str>,
) -> IdentityResult<AuthorizationRegistry> {
    let data = DataAccess {
        subject: AccessSubject::Direct,
        required_facts: [FactKind::Owner].into(),
        policy: policy(permission)?,
    };
    AuthorizationRegistry::new(vec![
        AccessRegistration {
            permission: Permission::parse("background_job:list")?,
            required_permissions: vec![],
            access: AccessMode::Query { data: data.clone(), mapping },
        },
        AccessRegistration {
            permission: Permission::parse("background_job:detail")?,
            required_permissions: vec![],
            access: AccessMode::Object { data, facts },
        },
    ])
}

/// 任务材料沿用原业务入口资格；后台任务功能权限不额外加入要求。
fn policy(permission: Option<&str>) -> IdentityResult<AccessPolicy> {
    let policy = match permission {
        Some(code) => {
            AccessPolicy::All(vec![AccessPolicy::Owned, AccessPolicy::Permission(Permission::parse(code)?)])
        },
        None => AccessPolicy::Owned,
    };
    Ok(AccessPolicy::Account { policy: Box::new(policy) })
}

fn model() -> QueryModel {
    QueryModel {
        key: QueryModelKey::parse("business_import_jobs").expect("固定导入任务模型"),
        resource: "background_job".into(),
        collection: Database::BACKGROUND_JOBS.into(),
        id_field: "id".into(),
        output: QueryOutput::Object,
        facts: [FactKind::Owner].into(),
    }
}

struct JobMapping {
    domain: &'static str,
    model: QueryModel,
}

#[async_trait]
impl AccessMapping for JobMapping {
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
        Ok(QueryFragment::Match(doc! {"deleted_at": 0, "domain_job_type": self.domain}))
    }
    fn predicate(
        &self,
        fact: &AccessFact,
        context: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<QueryFragment> {
        match fact {
            AccessFact::Owned => Ok(QueryFragment::Match(doc! {"requested_by": context.actor.id()})),
            _ => Err(IdentityError::ValidationError("导入任务仅提供发起人事实".into())),
        }
    }
    fn relation(
        &self,
        _: &RelationKey,
        _: &AccessContext,
        _: &MappingFacts,
    ) -> IdentityResult<RelationMapping> {
        Err(IdentityError::ValidationError("导入任务没有授权来源关系".into()))
    }
}

struct JobFacts {
    db: Database,
    domain: &'static str,
}

#[async_trait]
impl AccessFactsPort for JobFacts {
    async fn load(
        &self,
        _: &AccessContext,
        ids: &ResourceTargets,
        executor: &mut dyn Executor,
    ) -> IdentityResult<Vec<ObjectFacts>> {
        let ids = ids.ids().iter().map(|id| BackgroundJobId::new(id.clone())).collect::<Vec<_>>();
        Ok(self
            .db
            .background_jobs()
            .find_by_ids(&ids, executor)
            .await?
            .iter()
            .filter_map(|job| job_facts(job, self.domain))
            .collect())
    }
}

fn job_facts(job: &BackgroundJob, domain: &str) -> Option<ObjectFacts> {
    (job.domain_job_type.as_deref() == Some(domain)).then(|| ObjectFacts {
        resource: "background_job".into(),
        id: job.base.id.clone(),
        version: job.base.version.to_string(),
        owner_id: FactValue::Known(Some(job.requested_by.clone())),
        ..Default::default()
    })
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_core::common::time::Instant;
    use erp_support::{BackgroundJobData, JobType};

    use super::*;

    struct UnusedFacts;
    #[async_trait]
    impl AccessFactsPort for UnusedFacts {
        async fn load(
            &self,
            _: &AccessContext,
            _: &ResourceTargets,
            _: &mut dyn Executor,
        ) -> IdentityResult<Vec<ObjectFacts>> {
            panic!("目录构造不读取事实")
        }
    }

    #[test]
    fn original_business_feature_and_owner_are_declared_without_task_center_feature() {
        for permission in [None, Some("product:list"), Some("supplier:create"), Some("product_brand:create")]
        {
            let mapping = Arc::new(JobMapping { domain: "PRODUCT_IMPORT", model: model() });
            assert!(registry(mapping, Arc::new(UnusedFacts), permission).is_ok());
        }
        let AccessPolicy::Account { policy: rules } = policy(Some("product:list")).unwrap() else {
            panic!("业务材料使用账号入口")
        };
        let AccessPolicy::All(rules) = *rules else { panic!("必须同时证明业务权限与本人") };
        assert!(matches!(rules[0], AccessPolicy::Owned));
        assert!(
            matches!(&rules[1], AccessPolicy::Permission(permission) if permission == &Permission::parse("product:list").unwrap())
        );
        assert!(
            matches!(policy(None).unwrap(), AccessPolicy::Account { policy } if matches!(*policy, AccessPolicy::Owned))
        );
    }

    #[test]
    fn object_and_query_map_real_job_owner_and_reject_other_task_types() {
        let job = BackgroundJob::new(
            BackgroundJobId::new("job"),
            BackgroundJobData {
                job_no: "IMPORT-1".into(),
                job_type: JobType::Import,
                domain_job_type: Some("PRODUCT_IMPORT".into()),
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
        let facts = job_facts(&job, "PRODUCT_IMPORT").unwrap();
        assert_eq!(facts.resource, "background_job");
        assert_eq!(facts.id, "job");
        assert_eq!(facts.owner_id, FactValue::Known(Some("owner".into())));
        assert!(job_facts(&job, "SUPPLIER_IMPORT").is_none());
        let context = AccessContext {
            actor: AuditActor::new("owner".into(), "owner".into(), AccountKind::Admin),
            permission: Permission::parse("background_job:list").unwrap(),
            resource: "background_job".into(),
            model: Some(model().key),
            as_of: Instant::now(),
        };
        let mapping = JobMapping { domain: "PRODUCT_IMPORT", model: model() };
        let QueryFragment::Match(predicate) =
            mapping.predicate(&AccessFact::Owned, &context, &MappingFacts::default()).unwrap()
        else {
            panic!("本人事实应映射为查询条件")
        };
        assert_eq!(predicate, doc! {"requested_by": "owner"});
        assert!(mapping.predicate(&AccessFact::AssignedTo, &context, &MappingFacts::default()).is_err());
    }
}
