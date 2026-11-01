//! 整张发票的附件只向具有整票读取范围的账号开放。
use application_core::AuditActor;
use erp_core::ids::BusinessDocumentId;
use erp_procurement::PurchaseAccess;
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use persistence_core::Transactional;

use super::FundsAccess;
use crate::finance::document_files::{FinancialFileView, readable_file};
use crate::{Error, Result};

impl FundsAccess {
    /// 在同一读取事务中验证整票资格并列出附件。
    /// # 参数
    /// 发票身份、当前账号及采购来源授权器。
    /// # 返回
    /// 文件名称、类型和下载所需身份，不返回存储地址。
    /// # 错误
    /// 不可见发票、部分份额授权或文件读取失败时拒绝。
    pub async fn invoice_files_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
        purchase_access: &PurchaseAccess,
    ) -> Result<Vec<FinancialFileView>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_string();
        let purchase_access = purchase_access.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let invoice =
                        this.load_invoice_detail(&id, &actor, &purchase_access, executor).await?.data;
                    ensure_whole_invoice(invoice.permission_limited)?;
                    let attachments = this
                        .db
                        .document_attachments()
                        .list_by_document(&BusinessDocumentId::new(&id), executor)
                        .await?;
                    let ids = attachments.into_iter().map(|row| row.file_asset_id).collect::<Vec<_>>();
                    let files = this.db.file_assets().find_by_ids(&ids, executor).await?;
                    Ok(files
                        .into_iter()
                        .filter(readable_file)
                        .map(|file| {
                            FinancialFileView::new(
                                id.clone(),
                                invoice.invoice_no.clone().unwrap_or_else(|| "附件登记".into()),
                                file,
                            )
                        })
                        .collect())
                })
            })
            .await
    }
}

/// 整票文件不能按份额裁剪，部分授权必须拒绝。
fn ensure_whole_invoice(permission_limited: bool) -> Result<()> {
    if permission_limited {
        return Err(Error::Forbidden("当前仅可查看部分核销份额，不能下载整张发票文件".into()));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn invoice_files_require_whole_document_scope() {
        assert!(ensure_whole_invoice(false).is_ok());
        assert!(matches!(ensure_whole_invoice(true), Err(Error::Forbidden(_))));
    }
}
