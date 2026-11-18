//! 品牌后台导入：加密持久化输入；品牌、审计、行结果与任务进度原子提交。
use erp_permissions::catalog as catalog_permissions;
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
use erp_identity::{AccessTarget, AuthorizationService, Error as IdentityError};
use erp_party::SensitiveDataCodec;
use erp_permissions::internal;
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
    authorization: Arc<AuthorizationService>,
    job_access: ImportJobAccess,
    storage: S3Storage,
    codec: Arc<SensitiveDataCodec>,
    sources: Arc<ImportSourceCache<BrandImportJobRequest>>,
}

impl BrandImportProcess {
    /// 装配品牌导入所需服务，不执行 I/O。
    ///
    /// - `db`：任务数据库
    /// - `catalog`：已装配审计的品牌服务
    /// - `authorization`：实时身份与权限提供者
    /// - `storage`：持久化输入的对象存储
    /// - `codec`：输入加密及指纹密钥
    ///
    /// 可供提交与 worker 执行的流程。
    pub fn new(
        db: Database,
        catalog: CatalogService,
        authorization: Arc<AuthorizationService>,
        storage: S3Storage,
        codec: Arc<SensitiveDataCodec>,
    ) -> Self {
        let job_access = ImportJobAccess::new(
            db.clone(),
            authorization.clone(),
            BRAND_IMPORT_DOMAIN,
            internal::BRAND_IMPORT_JOB_READ,
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
}

// 提交、回放与执行在各自原事务重验当前账号及品牌创建权限。
async fn authorize(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<()> {
    authorization
        .require(actor, &catalog_permissions::PRODUCT_BRAND_CREATE, AccessTarget::Operation, executor)
        .await
        .map_err(|error| match error {
            IdentityError::Forbidden(_) => Error::Forbidden("当前账号无权导入品牌".into()),
            error => error.into(),
        })
}

// 从已保存且提交时验证过的内部提交人身份构造后台审计主体。
fn job_actor(job: &BackgroundJob) -> AuditActor {
    AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin)
}
