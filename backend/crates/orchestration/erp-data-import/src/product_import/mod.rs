//! 产品报价表异步导入：解析模板、登记后台任务并逐行创建商品。

mod add_sku;
mod backfill;
mod direct_upload;
mod execute;
mod identity;
mod images;
mod joint;
mod joint_commit;
mod legacy;
mod media;
mod parse;
mod query;
mod resolve;
mod row;
mod row_manifest;
mod submit;
mod supply_input;
mod views;

use std::sync::Arc;

use erp_catalog::{CatalogAccess, CatalogService};
use erp_identity::AuthorizationService;
use erp_permissions::internal;
use erp_supply::OfferingAccess;
use erp_supply::service::supplier_offering::SupplierOfferingService;
use erp_support::PRODUCT_IMPORT_DOMAIN_JOB_TYPE;
use mongodb::Database;
use storage::S3Storage;

use self::execute::JobRowSource;
use crate::background::import_source::ImportSourceCache;
use crate::repository::import_job_access::ImportJobAccess;

/// 导入消费的领域服务与附件转换合同，由应用注入实际 Port 提供者。
#[derive(Clone)]
pub struct ProductImportServices {
    pub catalog: CatalogService,
    pub scoped_catalog: CatalogService,
    pub catalog_access: CatalogAccess,
    pub offerings: Arc<SupplierOfferingService>,
    pub offering_access: OfferingAccess,
}

/// 商品报价表导入流程。
#[derive(Clone)]
pub struct ProductImportProcess {
    db: Database,
    services: ProductImportServices,
    job_access: ImportJobAccess,
    storage: S3Storage,
    secret: Vec<u8>,
    sources: Arc<ImportSourceCache<JobRowSource>>,
}

impl ProductImportProcess {
    /// 创建导入流程。
    ///
    /// - `db`：MongoDB 数据库。
    /// - `services`：应用装配的目录、供给与数据范围服务。
    /// - `authorization`：实时身份与权限服务。
    /// - `secret`：文件内容指纹密钥。
    ///
    /// 返回流程实例。
    pub fn new(
        db: Database,
        services: ProductImportServices,
        authorization: Arc<AuthorizationService>,
        storage: S3Storage,
        secret: impl Into<Vec<u8>>,
    ) -> Self {
        Self {
            job_access: ImportJobAccess::new(
                db.clone(),
                authorization,
                PRODUCT_IMPORT_DOMAIN_JOB_TYPE,
                internal::PRODUCT_IMPORT_JOB_READ,
            ),
            db,
            services,
            storage,
            secret: secret.into(),
            // 单份旧工作簿允许 700 MiB；缓存最多一轮任务，合计输入预算 1 GiB。
            sources: Arc::new(ImportSourceCache::new(1024 * 1024 * 1024)),
        }
    }
}
