//! 新建入口只持久化客户绑定的预览，不提前创建空询价。

use application_core::AuditActor;
use entity_core::BaseModel;
use erp_core::common::time::Instant;
use erp_identity::Permission;
use erp_identity::service::access_control::resolve::DataScopeService;
use erp_sales::entity::sales_inquiry::{InquiryCreateData, InquiryRequirementData, SalesInquiry};
use erp_sales::entity::sales_inquiry_import::{
    INQUIRY_IMPORT_MAX_BYTES, InquiryImportPreview, InquiryImportValidation,
};
use erp_sales::repository::extensions::SalesInquiryImportExt;
use id_generator::next_id;
use persistence_core::{Executor, NoTransaction, Transactional};
use sha2::{Digest, Sha256};

use super::facts::sku_facts;
use super::parse::{self, Workbook};
use super::preview::{delivery_rows, locate_issue, product_rows};
use super::{InquiryImportPreviewView, SalesInquiryImportProcess};
use crate::business_ownership::required_business_org;
use crate::sales_inquiry::SalesInquiryProcess;
use crate::sales_inquiry::access::SalesInquiryAccess;
use crate::{Error, Result};

impl SalesInquiryImportProcess {
    /// 上传客户文件并生成新建预览，不创建询价草稿。
    /// # 参数
    /// `actor`、`customer` 绑定当前销售与客户，文件为有界 XLSX。
    /// # 返回
    /// 客户专用预览、逐行错误及待确认的合并明细。
    /// # 错误
    /// 权限、客户、文件或持久化无效时拒绝。
    pub async fn preview_creation(
        &self,
        actor: &AuditActor,
        customer: &str,
        file_name: &str,
        bytes: Vec<u8>,
    ) -> Result<InquiryImportPreviewView> {
        self.creation_header(actor, customer, &mut NoTransaction).await?;
        if bytes.is_empty()
            || bytes.len() > INQUIRY_IMPORT_MAX_BYTES
            || !file_name.to_ascii_lowercase().ends_with(".xlsx")
        {
            return Err(Error::ValidationError("请选择非空且不超过 5 MiB 的 XLSX 文件".into()));
        }
        let fingerprint = hex::encode(Sha256::digest(&bytes));
        let workbook = tokio::task::spawn_blocking(move || parse::parse(&bytes))
            .await
            .map_err(|_| Error::Internal("工作簿解析任务失败".into()))??;
        let this = self.clone();
        let actor = actor.clone();
        let customer = customer.to_string();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.save_creation_preview(&actor, &customer, fingerprint, workbook, executor).await
                })
            })
            .await
    }

    pub(super) async fn creation_header(
        &self,
        actor: &AuditActor,
        customer: &str,
        executor: &mut dyn Executor,
    ) -> Result<InquiryCreateData> {
        let permissions =
            [Permission::parse("sales_inquiry:import")?, Permission::parse("sellable_sku:list")?];
        let access = DataScopeService::new(self.db.clone(), self.rbac.clone())
            .resolve_permissions(actor, "sales_inquiry", "create", &permissions, executor)
            .await?;
        let name = SalesInquiryProcess::new(self.db.clone(), self.rbac.clone())
            .require_customer(actor, customer, executor)
            .await?;
        let data = InquiryCreateData {
            customer_id: customer.into(),
            customer_name: name,
            sales_owner_user_id: actor.id().into(),
            business_org_unit_id: required_business_org(&self.db, actor.id(), executor).await?,
            deadline_at: Instant::from_unix_secs(0),
            copied_from_inquiry_id: None,
            draft: InquiryRequirementData::default(),
        };
        let candidate = SalesInquiry::create("preview".into(), "preview".into(), data.clone())?;
        if !SalesInquiryAccess::allows_sales(&access, &candidate) {
            return Err(Error::Forbidden("没有该业务责任范围的询价导入创建权限".into()));
        }
        Ok(data)
    }

    async fn save_creation_preview(
        &self,
        actor: &AuditActor,
        customer: &str,
        fingerprint: String,
        workbook: Workbook,
        executor: &mut dyn Executor,
    ) -> Result<InquiryImportPreviewView> {
        self.creation_header(actor, customer, executor).await?;
        let products = product_rows(&workbook);
        let deliveries = delivery_rows(&workbook);
        let codes = products.iter().map(|row| row.sku_code.clone()).collect::<Vec<_>>();
        let facts = sku_facts(&self.db, &codes, executor).await?;
        let validated = InquiryImportValidation::build(
            &products,
            &deliveries,
            &facts,
            &InquiryRequirementData::default(),
            next_id,
        );
        let mut errors = workbook.errors;
        errors.extend(
            validated
                .errors
                .into_iter()
                .map(|issue| locate_issue(issue, workbook.product_sheet, workbook.automatic_delivery)),
        );
        let preview = InquiryImportPreview {
            base: BaseModel::new(next_id()),
            inquiry_id: String::new(),
            actor_id: actor.id().into(),
            creation_customer_id: Some(customer.into()),
            expected_version: 0,
            input_fingerprint: fingerprint,
            errors,
            merges: validated.merges,
            data: validated.data,
            expires_at: Instant::now().unix_secs() + 3600,
            result: None,
        };
        self.db.inquiry_import_previews().create(&preview, executor).await?;
        Ok(preview.into())
    }
}
