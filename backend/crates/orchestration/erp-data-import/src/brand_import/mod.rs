//! 品牌后台导入：加密持久化输入；品牌、审计、行结果与任务进度原子提交。
mod execute;
mod failures;
mod rows;
mod source;
mod submit;

use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_catalog::dto::BrandImportJobRequest;
use erp_core::AccountKind;
use erp_identity::{
    AccessMode, AccessRegistration, AccessTarget, AuthorizationRegistry, AuthorizationService,
    Error as IdentityError, Permission, SharedRbacService,
};
use erp_party::SensitiveDataCodec;
use erp_support::BackgroundJob;
use mongodb::Database;
use persistence_core::Executor;
use storage::S3Storage;

use crate::background::import_source::ImportSourceCache;
use crate::repository::import_job_access::ImportJobAccess;
use crate::{Error, Result};

/// 持久化的品牌导入任务类型。
pub const BRAND_IMPORT_DOMAIN: &str = "BRAND_IMPORT";

/// 品牌导入流程，由 HTTP 和独立 worker 共用。
#[derive(Clone)]
pub struct BrandImportProcess {
    db: Database,
    catalog: CatalogService,
    authorization: AuthorizationService,
    job_access: ImportJobAccess,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
    sources: Arc<ImportSourceCache<BrandImportJobRequest>>,
}

impl BrandImportProcess {
    /// 装配品牌导入所需服务，不执行 I/O。
    /// # 参数
    /// * `db` - 任务数据库
    /// * `catalog` - 已装配审计的品牌服务
    /// * `rbac` - 实时身份与权限提供者
    /// * `storage` - 持久化输入的对象存储
    /// * `codec` - 输入加密及指纹密钥
    /// # 返回
    /// 可供提交与 worker 执行的流程。
    /// # 错误
    /// 无。
    pub fn new(
        db: Database,
        catalog: CatalogService,
        rbac: SharedRbacService,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
    ) -> Self {
        let job_access =
            ImportJobAccess::new(db.clone(), rbac.clone(), BRAND_IMPORT_DOMAIN, Some("product_brand:create"));
        let registration = AccessRegistration {
            permission: Permission::parse("product_brand:create").expect("固定品牌创建权限"),
            required_permissions: vec![],
            access: AccessMode::Operation,
        };
        let authorization = AuthorizationService::new(
            db.clone(),
            rbac,
            AuthorizationRegistry::new(vec![registration]).expect("品牌创建是固定无对象操作"),
        );
        Self {
            db,
            catalog,
            authorization,
            job_access,
            storage,
            codec,
            sources: Arc::new(ImportSourceCache::new(20 * 1024 * 1024)),
        }
    }

    /// 每次提交、执行和下载都重验当前账号及品牌创建权限，不缓存授权结论。
    async fn authorize(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        self.authorization
            .require(actor, &Permission::parse("product_brand:create")?, AccessTarget::Operation, executor)
            .await
            .map_err(|error| match error {
                IdentityError::Forbidden(_) => Error::Forbidden("当前账号无权导入品牌".into()),
                error => error.into(),
            })
    }
}

/// 从已保存且提交时验证过的内部提交人身份构造后台审计主体。
fn job_actor(job: &BackgroundJob) -> AuditActor {
    AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin)
}
