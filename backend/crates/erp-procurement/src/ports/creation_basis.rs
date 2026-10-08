//! 采购创建与提交时的供应商资格和商务条件合同。
use async_trait::async_trait;
use erp_core::common::time::BusinessDate;
use erp_core::ids::SupplierAccountId;
use persistence_core::Executor;

use crate::entity::facts::{PaymentTermFact, SupplierRoleFact};
use crate::entity::purchase_order::PurchaseType;
/// 委派供应商资格、事实与受控付款代码解析；采购决定验证顺序和快照构造。
#[async_trait]
pub trait CreationBasisSupplierPort: Send + Sync {
    /// 在提交事务中复核本采购类型的供应商能力和已关联合同。
    ///
    /// # 参数
    /// * `supplier_id` - 待复核的供应商账号。
    /// * `purchase_type` - 本次采购类型。
    /// * `on_date` - 资格判定的业务日期。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 资格满足时无返回值。
    ///
    /// # 错误
    /// 资格不满足或事实读取失败时返回原错误，禁止继续提交。
    async fn ensure_qualified(
        &self,
        supplier_id: &SupplierAccountId,
        purchase_type: PurchaseType,
        on_date: BusinessDate,
        executor: &mut dyn Executor,
    ) -> crate::Result<()>;
    /// 在调用方事务中重读供应商；缺失保留 `None`。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商账号。
    /// * `executor` - 调用方事务执行器。
    ///
    /// # 返回
    /// 找到供应商时返回其商务修订指针事实；供应商不存在时返回 `None`。
    ///
    /// # 错误
    /// 读取供应商失败时返回对应错误。
    async fn supplier_role(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> crate::Result<Option<SupplierRoleFact>>;
    /// 调用供应商领域唯一解析器解析付款条件；不能复制代码别名规则。
    ///
    /// # 参数
    /// * `code` - 付款条件代码。
    ///
    /// # 返回
    /// 返回供应商领域解析后的 `PaymentTermFact`。
    ///
    /// # 错误
    /// 代码无法按供应商付款条件规则解析时返回对应错误。
    fn payment_term(&self, code: &str) -> erp_core::Result<PaymentTermFact>;
    /// 先分离历史快照附带的经营类目，再解析付款条件。
    ///
    /// # 参数
    /// * `code` - 可能附带经营类目的历史快照文本。
    ///
    /// # 返回
    /// 返回分离经营类目后的 `PaymentTermFact`。
    ///
    /// # 错误
    /// 分离后的付款条件代码无法解析时返回对应错误。
    fn payment_snapshot(&self, code: &str) -> erp_core::Result<PaymentTermFact>;
}
