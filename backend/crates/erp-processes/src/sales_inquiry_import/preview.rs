//! 原文件指纹、询价版本与当前账号绑定的导入预览。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_identity::Permission;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_sales::entity::sales_inquiry::SalesInquiry;
use erp_sales::entity::sales_inquiry_import::{
    INQUIRY_IMPORT_MAX_BYTES, InquiryImportDelivery, InquiryImportPreview, InquiryImportProduct,
    InquiryImportValidation,
};
use erp_sales::repository::extensions::SalesInquiryImportExt;
use erp_sales::service::sales_inquiry::SalesInquiryService;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::facts::sku_facts;
use super::parse::{self, Workbook};
use super::{InquiryImportPreviewView, SalesInquiryImportProcess};
use crate::sales_inquiry::access::SalesInquiryAccess;
use crate::{Error, Result};

impl SalesInquiryImportProcess {
    /// 解析并持久化当前账号、询价和版本专用的校验预览。
    /// # 参数
    /// `actor`、`id` 为当前访问上下文，`expected_version` 为编辑版本，
    /// `file_name` 须为 xlsx，`bytes` 为最多 5 MiB 的原文件。
    /// # 返回
    /// 返回逐工作表行错误、合并结果和客户安全需求快照；不修改正式需求。
    /// # 错误
    /// 无权、版本过期、文件结构非法或存储失败时拒绝。
    pub async fn preview(
        &self,
        actor: &AuditActor,
        id: &str,
        expected_version: u64,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<InquiryImportPreviewView> {
        self.require_import(actor, id, &mut NoTransaction).await?.ensure_version(expected_version)?;
        if bytes.is_empty() || bytes.len() > INQUIRY_IMPORT_MAX_BYTES {
            return Err(Error::ValidationError("工作簿不能为空且不得超过 5 MiB".into()));
        }
        if !file_name.to_ascii_lowercase().ends_with(".xlsx") {
            return Err(Error::ValidationError("请选择 .xlsx 客户选品工作簿".into()));
        }
        let fingerprint = hex::encode(Sha256::digest(&bytes));
        let workbook = tokio::task::spawn_blocking(move || parse::parse(&bytes))
            .await
            .map_err(|_| Error::Internal("工作簿解析任务失败".into()))??;
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.save_preview(&actor, &id, expected_version, fingerprint, workbook, executor).await
                })
            })
            .await
    }

    pub(super) fn access(&self) -> SalesInquiryAccess {
        SalesInquiryAccess::new(self.db.clone(), self.rbac.clone())
    }

    pub(super) async fn require_import(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<SalesInquiry> {
        let catalog = Permission::parse("sellable_sku:list")?;
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "sales_inquiry", "import", &[catalog], executor)
            .await?;
        let inquiry = SalesInquiryService::new(self.db.clone()).require(id, executor).await?;
        if !SalesInquiryAccess::allows_sales(&access, &inquiry) {
            return Err(Error::NotFound("询价不存在或无权操作".into()));
        }
        Ok(inquiry)
    }

    async fn save_preview(
        &self,
        actor: &AuditActor,
        id: &str,
        expected_version: u64,
        fingerprint: String,
        workbook: Workbook,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportPreviewView> {
        let inquiry = self.require_import(actor, id, executor).await?;
        inquiry.ensure_version(expected_version)?;
        let products = product_rows(&workbook);
        let deliveries = delivery_rows(&workbook);
        let codes = products.iter().map(|row| row.sku_code.clone()).collect::<Vec<_>>();
        let facts = sku_facts(&self.db, &codes, executor).await?;
        let validated =
            InquiryImportValidation::build(&products, &deliveries, &facts, &inquiry.draft, next_id);
        let mut errors = workbook.errors;
        errors.extend(validated.errors);
        let expires_at = Instant::now().unix_secs() + 3600;
        let preview = InquiryImportPreview {
            base: BaseModel::new(next_id()),
            inquiry_id: id.into(),
            actor_id: actor.id().into(),
            expected_version,
            input_fingerprint: fingerprint,
            errors,
            merges: validated.merges,
            data: validated.data,
            expires_at,
            result: None,
        };
        self.db.inquiry_import_previews().create(&preview, executor).await?;
        Ok(preview.into())
    }
}

fn product_rows(workbook: &Workbook) -> Vec<InquiryImportProduct> {
    workbook
        .products
        .iter()
        .map(|row| InquiryImportProduct {
            row_number: row.number,
            sku_code: row.cells[0].value.clone(),
            sku_is_text: row.cells[0].text,
            unit: row.cells[3].value.clone(),
            quantity: row.cells[4].value.clone(),
            sales_reference_price: row.cells[5].value.clone(),
            note: row.cells[6].value.clone(),
        })
        .collect()
}

fn delivery_rows(workbook: &Workbook) -> Vec<InquiryImportDelivery> {
    workbook
        .deliveries
        .iter()
        .map(|row| InquiryImportDelivery {
            row_number: row.number,
            sku_code: row.cells[0].value.clone(),
            sku_is_text: row.cells[0].text,
            recipient: row.cells[1].value.clone(),
            phone: row.cells[2].value.clone(),
            phone_is_text: row.cells[2].text,
            province: row.cells[3].value.clone(),
            city: row.cells[4].value.clone(),
            district: row.cells[5].value.clone(),
            address: row.cells[6].value.clone(),
            quantity: row.cells[7].value.clone(),
            due_date: row.cells[8].value.clone(),
            terms: row.cells[9].value.clone(),
        })
        .collect()
}
