//! 在采购创建与提交中委派供应商资格及唯一付款条件规则。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SupplierAccountId;
use erp_procurement::entity::facts::{PaymentTermFact, SupplierRoleFact};
use erp_procurement::entity::purchase_order::PurchaseType;
use erp_procurement::ports::creation_basis::CreationBasisSupplierPort;
use erp_supplier::entity::supplier::eligibility::OfferingProductKind;
use erp_supplier::SupplierExt;
use mongodb::Database;
use persistence_core::Executor;
/// 绑定组合根数据库，不提前执行读取。
pub struct CreationBasisSupplierAdapter {
    db: Database,
}
impl CreationBasisSupplierAdapter {
    pub fn new(db: Database) -> Self {
        Self { db }
    }
}
#[async_trait]
impl CreationBasisSupplierPort for CreationBasisSupplierAdapter {
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        purchase_type: PurchaseType,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<()> {
        let kind = match purchase_type {
            PurchaseType::Physical => OfferingProductKind::Physical,
            PurchaseType::Virtual => OfferingProductKind::Virtual,
            PurchaseType::Service => OfferingProductKind::OfflineService,
        };
        erp_supplier::service::supplier::eligibility::ensure_offering_capability_qualified(
            &self.db,
            supplier_id,
            kind,
            on_date,
            executor,
        )
        .await
        .map_err(map_supplier_error)
    }
    async fn supplier_role(
        &self,
        id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> erp_procurement::Result<Option<SupplierRoleFact>> {
        Ok(self
            .db
            .supplier_accounts()
            .find_by_id(id, executor)
            .await?
            .map(|supplier| SupplierRoleFact {
                current_commercial_profile_revision_id: supplier.current_commercial_profile_revision_id,
            }))
    }
    fn payment_term(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
        super::super::adapters::payment_term::parse(code)
    }
    fn payment_snapshot(&self, code: &str) -> erp_core::Result<PaymentTermFact> {
        super::super::adapters::payment_term::parse_snapshot(code)
    }
}

/// 跨域错误保留业务拒绝、仓储错误与事务冲突类别。
fn map_supplier_error(error: erp_supplier::Error) -> erp_procurement::Error {
    use erp_procurement::Error as Target;
    use erp_supplier::Error as Source;
    match error {
        Source::Internal(message) => Target::Internal(message),
        Source::NotFound(message) => Target::NotFound(message),
        Source::ValidationError(message) => Target::ValidationError(message),
        Source::BusinessLogicError(message) => Target::BusinessLogicError(message),
        Source::ConflictError(message) => Target::ConflictError(message),
        Source::ReceiptDuplicate(error) => Target::ReceiptDuplicate(error),
        Source::TransientTransaction(error) => Target::TransientTransaction(error),
        Source::Forbidden(message) => Target::Forbidden(message),
        Source::Unauthenticated(message) => Target::Unauthenticated(message),
        Source::Logic(error) => Target::Logic(error),
        Source::OutcomeUnknown(error) => Target::OutcomeUnknown(error),
        Source::RepositoryError(error) => Target::RepositoryError(error),
    }
}
