//! 采购创建与提交时的供应商资格和商务条件合同。
use crate::entity::facts::{PaymentTermFact, SupplierRoleFact};
use crate::entity::purchase_order::PurchaseType;
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SupplierAccountId;
use persistence_core::Executor;
/// 委派供应商资格、事实与受控付款代码解析；采购决定验证顺序和快照构造。
#[async_trait]
pub trait CreationBasisSupplierPort: Send + Sync {
    /// 在提交事务中复核本采购类型的供应商能力和已关联合同。
    ///
    /// # Errors
    /// 资格不满足或事实读取失败时返回原错误，禁止继续提交。
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        purchase_type: PurchaseType,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> crate::Result<()>;
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
