//! 在采购创建的原重验位置读取供应商并调用其唯一付款条件解析器。
use async_trait::async_trait;
use erp_core::ids::SupplierAccountId;
use erp_procurement::entity::facts::{PaymentTermFact, SupplierRoleFact};
use erp_procurement::ports::creation_basis::CreationBasisSupplierPort;
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
