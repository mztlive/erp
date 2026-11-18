//! 财务附件写入对象存储前的固定入口资格准备；业务写事务仍须完整授权。
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{AccessTarget, AuthorizationService};
use erp_permissions::{Permission, internal};
use mongodb::Database;
use persistence_core::{Executor, Transactional};

use crate::Result;

/// 允许携带附件的固定财务工作流，不接受调用方传入权限或政策。
#[derive(Clone, Copy)]
pub enum FinanceAttachmentUpload {
    CustomerReceipt,
    InvoiceDraft,
    InvoicePosting,
    PurchaseInvoice,
    SalesExtraCost,
    PurchasePayment,
}

impl FinanceAttachmentUpload {
    // 把附件工作流映射到 identity 登记的预检权限，不接受调用方指定权限。
    fn permission(self) -> Permission {
        match self {
            Self::CustomerReceipt => internal::FINANCE_UPLOAD_CUSTOMER_RECEIPT,
            Self::InvoiceDraft => internal::FINANCE_UPLOAD_INVOICE_DRAFT,
            Self::InvoicePosting => internal::FINANCE_UPLOAD_INVOICE_POST,
            Self::PurchaseInvoice => internal::FINANCE_UPLOAD_PURCHASE_INVOICE,
            Self::SalesExtraCost => internal::FINANCE_UPLOAD_EXTRA_COST,
            Self::PurchasePayment => internal::FINANCE_UPLOAD_PURCHASE_PAYMENT,
        }
    }
}

/// 只负责外部文件副作用前的功能资格，不签发对象或任务授权凭证。
pub struct FinanceAttachmentService {
    db: Database,
    authorization: Arc<AuthorizationService>,
}

impl FinanceAttachmentService {
    /// 供应用装配上传预检：复用进程共享的授权服务和业务数据库，不做 I/O，也不缓存请求资格。
    pub fn new(db: Database, authorization: Arc<AuthorizationService>) -> Self {
        Self { db, authorization }
    }

    /// 应用在写入对象存储前调用，按固定财务工作流检查当前账号是否具备同一角色下的完整功能资格。
    /// 身份无效、功能资格不足或授权设施失败时拒绝上传。这一步不授予具体对象、来源或任务的办理权，
    /// 文件存储后的写事务仍须用真实目标重验完整业务授权。
    pub async fn prepare_attachment_upload(
        &self,
        actor: &AuditActor,
        upload: FinanceAttachmentUpload,
    ) -> Result<()> {
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { authorize_upload(&authorization, &actor, upload, executor).await })
            })
            .await
    }
}

// 上传预检事务只按工作流映射的固定权限检查功能资格，不产生可供后续对象或任务授权复用的凭证。
async fn authorize_upload(
    authorization: &AuthorizationService,
    actor: &AuditActor,
    upload: FinanceAttachmentUpload,
    executor: &mut dyn Executor,
) -> Result<()> {
    Ok(authorization.require(actor, &upload.permission(), AccessTarget::Operation, executor).await?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::entity::access_control::ResolvedScope;
    use erp_identity::testing::{fixed_authorization, unavailable_authorization};
    use erp_permissions::{cost, payable, purchase_payment_request, receivable, sales_order};
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::Error;

    fn actor() -> AuditActor {
        AuditActor::new("user".into(), "login".into(), AccountKind::Admin)
    }

    #[tokio::test]
    async fn fixed_finance_uploads_require_their_business_qualification_without_file_upload_menu() {
        let cases = [
            (FinanceAttachmentUpload::CustomerReceipt, vec![receivable::CUSTOMER_RECEIPT_SUBMIT]),
            (FinanceAttachmentUpload::InvoiceDraft, vec![receivable::INVOICE_CREATE]),
            (FinanceAttachmentUpload::InvoicePosting, vec![receivable::INVOICE_POST]),
            (FinanceAttachmentUpload::PurchaseInvoice, vec![payable::PURCHASE_INVOICE_ALLOCATION_POST]),
            (FinanceAttachmentUpload::SalesExtraCost, vec![cost::ENTRY_CREATE, sales_order::DETAIL]),
            (
                FinanceAttachmentUpload::PurchasePayment,
                vec![purchase_payment_request::CREDIT_LIST, purchase_payment_request::DETAIL],
            ),
        ];

        for (upload, permissions) in cases {
            let authorization = fixed_authorization(
                &permissions,
                ResolvedScope { role_clauses: vec![], user_limit: None },
                vec![],
                vec![],
            )
            .unwrap();
            authorize_upload(&authorization, &actor(), upload, &mut SnapshotExecutor::default())
                .await
                .unwrap();

            let denied = fixed_authorization(
                &[],
                ResolvedScope { role_clauses: vec![], user_limit: None },
                vec![],
                vec![],
            )
            .unwrap();
            assert!(matches!(
                authorize_upload(&denied, &actor(), upload, &mut SnapshotExecutor::default()).await,
                Err(Error::Forbidden(_))
            ));
        }
    }

    #[tokio::test]
    async fn finance_upload_preparation_keeps_authorization_facility_failure() {
        let error = authorize_upload(
            &unavailable_authorization(),
            &actor(),
            FinanceAttachmentUpload::InvoiceDraft,
            &mut SnapshotExecutor::default(),
        )
        .await
        .unwrap_err();
        assert!(matches!(error, Error::Internal(reason) if reason == "测试授权设施不可用"));
    }
}
