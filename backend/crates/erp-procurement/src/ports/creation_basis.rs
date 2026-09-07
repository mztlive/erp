//! 采购依据创建时冻结供应商商务条件的事实合同。
use crate::entity::facts::{PaymentTermFact, SupplierRoleFact};
use async_trait::async_trait;
use erp_core::ids::SupplierAccountId;
use persistence_core::Executor;
/// 仅负责供应商事实与受控付款代码解析；采购仍决定验证顺序和快照构造。
#[async_trait]
pub trait CreationBasisSupplierPort: Send + Sync {
    /// 在调用方事务中重读供应商；缺失保留 None。
    async fn supplier_role(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<SupplierRoleFact>>;
    /// 调用供应商领域唯一解析器；不能复制代码别名规则。
    fn payment_term(&self, code: &str) -> erp_core::Result<PaymentTermFact>;
    /// 保留快照原先分离附带经营类目再解析的调用。
    fn payment_snapshot(&self, code: &str) -> erp_core::Result<PaymentTermFact>;
}
