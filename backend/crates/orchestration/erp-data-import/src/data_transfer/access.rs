//! 结果归属与业务授权分别校验：导出可共享读取，业务写入仍限定原提交人。
use application_core::AuditActor;
use erp_core::AccountKind;

use super::TransferKind;
use crate::{Error, Result};

impl TransferKind {
    /// 校验结果类型与归属，不替代当前业务权限和产物范围校验。
    /// # 参数
    /// `owner` 为原提交人，`actor` 为当前读取人。
    /// # 返回
    /// 本人结果或管理端读取导出时允许继续进入业务授权。
    /// # 错误
    /// 非本人读取导入，或门户账号读取他人结果时拒绝。
    pub(super) fn authorize_reader(self, owner: &str, actor: &AuditActor) -> Result<()> {
        if owner == actor.id() || (actor.kind() == AccountKind::Admin && self.is_export()) {
            return Ok(());
        }
        Err(Error::Forbidden("导入预览和批量处理结果仅供原提交人使用".into()))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn another_admin_account_passes_export_ownership_but_not_import_ownership() {
        let reader = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        for kind in
            [TransferKind::ProductPoolExcel, TransferKind::SupplierQuoteArchive, TransferKind::SalesOrders]
        {
            assert!(kind.authorize_reader("owner", &reader).is_ok());
        }
        for kind in [
            TransferKind::InquiryPreview,
            TransferKind::InquiryCreationPreview,
            TransferKind::SupplierQuotePreview,
            TransferKind::SupplyBatch,
            TransferKind::PortalBatch,
        ] {
            assert!(kind.authorize_reader("owner", &reader).is_err());
            assert!(kind.authorize_reader("reader", &reader).is_ok());
        }
    }

    #[test]
    fn portal_identity_cannot_read_another_accounts_export_or_import() {
        let reader = AuditActor::new("supplier".into(), "supplier".into(), AccountKind::Supplier);
        assert!(TransferKind::SalesOrders.authorize_reader("owner", &reader).is_err());
        assert!(TransferKind::PortalBatch.authorize_reader("owner", &reader).is_err());
        assert!(TransferKind::PortalBatch.authorize_reader("supplier", &reader).is_ok());
    }
}
