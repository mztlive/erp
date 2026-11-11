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
use erp_identity::{AuthorizationRegistry, AuthorizationService, Permission, SharedRbacService};
use erp_party::SensitiveDataCodec;
use erp_support::BackgroundJob;
use mongodb::Database;
use persistence_core::Executor;
use storage::S3Storage;

use crate::background::import_source::ImportSourceCache;
use crate::{Error, Result};

/// 持久化的品牌导入任务类型。
pub const BRAND_IMPORT_DOMAIN: &str = "BRAND_IMPORT";

/// 品牌导入流程，由 HTTP 和独立 worker 共用。
#[derive(Clone)]
pub struct BrandImportProcess {
    db: Database,
    catalog: CatalogService,
    rbac: SharedRbacService,
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
        Self {
            db,
            catalog,
            rbac,
            storage,
            codec,
            sources: Arc::new(ImportSourceCache::new(20 * 1024 * 1024)),
        }
    }

    /// 每次提交、执行和下载都重验当前账号及品牌创建权限，不缓存授权结论。
    async fn authorize(&self, actor: &AuditActor, executor: &mut dyn Executor) -> Result<()> {
        let authorization =
            AuthorizationService::new(self.db.clone(), self.rbac.clone(), AuthorizationRegistry::default());
        if !authorization.has_permission(actor, &Permission::parse("product_brand:create")?, executor).await?
        {
            return Err(Error::Forbidden("当前账号无权导入品牌".into()));
        }
        Ok(())
    }
}

/// 从已保存且提交时验证过的内部提交人身份构造后台审计主体。
fn job_actor(job: &BackgroundJob) -> AuditActor {
    AuditActor::new(job.requested_by.clone(), job.requested_by.clone(), AccountKind::Admin)
}
