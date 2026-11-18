//! 品牌导入单行步骤；事务由后台任务流程持有，品牌、审计、行结果和进度共同提交。
use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_identity::AccessTarget;
use erp_permissions::catalog;
use id_generator::{DocumentNumberGenerator, DocumentNumberKind};
use persistence_core::Executor;

use super::CatalogService;
use crate::dto::{BrandImportResult, BrandImportRow, BrandImportStatus};
use crate::entity::catalog::ProductBrand;
use crate::entity::catalog::brand_import::{BrandImportKey, PreparedBrandImport};
use crate::error::{Error, Result};
use crate::repository::CatalogExt;
use crate::repository::prelude::ProductBrandRepositoryImportLookupExt;

impl CatalogService {
    /// 在调用方事务内创建一个品牌，行内错误作为确定失败返回。
    /// # 参数
    /// * `key` - 已验证的稳定导入身份
    /// * `row` - 原始行内容
    /// * `actor` - 已在当前事务重验权限的提交人
    /// * `executor` - 同时持久化任务进度的事务执行器
    /// # 返回
    /// 新建、跳过或行内校验失败；不自行提交事务。
    /// # 错误
    /// 数据库、取号或审计失败时返回错误，调用方须回滚整行事务。
    pub async fn product_brand_import_row(
        &self,
        key: &BrandImportKey,
        row: BrandImportRow,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<BrandImportResult> {
        self.data_scope
            .authorization
            .require(actor, &catalog::PRODUCT_BRAND_CREATE, AccessTarget::Operation, executor)
            .await?;

        let number = row.row_number;
        if let Some(error) = row.error.filter(|value| !value.trim().is_empty()) {
            return Ok(BrandImportResult::failed(number, error));
        }
        let prepared = PreparedBrandImport::new(key.brand_id(number), row.name, row.brand_code, actor.id());
        let prepared = match prepared {
            Ok(value) => value,
            Err(error) => return Ok(BrandImportResult::failed(number, error.to_string())),
        };

        let code = (!prepared.automatic_code).then_some(prepared.brand.brand_code.as_str());
        if let Some(existing) = self
            .db
            .product_brands()
            .find_import_match(&prepared.brand.base.id, code, &prepared.brand.name, executor)
            .await?
        {
            return Ok(match prepared.ensure_matches(&existing) {
                Ok(()) => BrandImportResult::from_brand(number, BrandImportStatus::Skipped, existing),
                Err(error) => BrandImportResult::failed(number, error.to_string()),
            });
        }

        let brand = self.create_import_brand(prepared, actor, executor).await?;
        Ok(BrandImportResult::from_brand(number, BrandImportStatus::Created, brand))
    }

    /// 品牌和审计沿同一外部执行器写入，数据库冲突不吞成行内校验失败。
    async fn create_import_brand(
        &self,
        mut prepared: PreparedBrandImport,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<ProductBrand> {
        if prepared.automatic_code {
            prepared.brand.brand_code = DocumentNumberGenerator::new(self.db.clone())
                .next_number(
                    DocumentNumberKind::ProductBrand,
                    BusinessDate::today().as_naive_date(),
                    executor,
                )
                .await
                .map_err(|error| Error::Internal(error.to_string()))?;
        }

        let audit = self.audit.resource_log(
            actor.clone(),
            "product_brand.create",
            "product_brand",
            prepared.brand.base.id.clone(),
        )?;
        self.db.product_brands().create(&prepared.brand, executor).await?;
        self.audit.persist(&audit, executor).await?;
        Ok(prepared.brand)
    }
}
