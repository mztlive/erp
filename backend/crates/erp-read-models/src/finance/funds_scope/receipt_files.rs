//! 整笔回款凭证按完整回款详情范围读取。
use application_core::AuditActor;
use persistence_core::Transactional;

use super::FundsAccess;
use crate::finance::document_files::{FinancialFileView, financial_document_files};
use crate::{Error, Result};

impl FundsAccess {
    /// 在同一读取事务验证整单权限并列出回款凭证。
    /// # 参数
    /// 回款身份与当前操作人。
    /// # 返回
    /// 可读取的回款凭证。
    /// # 错误
    /// 部分份额授权、单据不可见或读取失败时拒绝。
    pub async fn customer_receipt_files_scoped(
        &self,
        id: &str,
        actor: &AuditActor,
    ) -> Result<Vec<FinancialFileView>> {
        let this = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let receipt = this.load_customer_receipt_detail(&id, &actor, executor).await?.data;
                    ensure_whole_receipt(receipt.permission_limited)?;
                    financial_document_files(&this.db, &id, &receipt.receipt_no, executor).await
                })
            })
            .await
    }
}

/// 凭证不能按核销份额裁剪。
fn ensure_whole_receipt(limited: bool) -> Result<()> {
    if limited {
        return Err(Error::Forbidden("当前仅可查看部分核销份额，不能下载整笔回款凭证".into()));
    }
    Ok(())
}
#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn receipt_files_require_full_scope() {
        ensure_whole_receipt(false).unwrap();
        assert!(matches!(ensure_whole_receipt(true), Err(Error::Forbidden(_))));
    }
}
