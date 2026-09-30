//! 供命名用例在原事务内校验完整财务来源读取资格。

use application_core::AuditActor;
use erp_procurement::PurchaseAccess;
use persistence_core::Executor;

use super::FundsAccess;
use crate::{Error, Result};

impl FundsAccess {
    /// 在原事务中校验来源单据完整读取，部分份额不授权整单管理。
    ///
    /// # 参数
    /// * `actor` - 已认证调用人。
    /// * `resource` / `id` - 实际财务来源类型与主键。
    /// * `purchase_access` - 采购方向访问器。
    /// * `executor` - 调用方事务，不另开事务。
    /// # 返回
    /// 完整来源可读返回 true；不可见与不存在返回 false。
    /// # 错误
    /// 持久化失败、授权漂移或不支持的来源类型返回错误。
    pub async fn source_document_readable(
        &self,
        actor: &AuditActor,
        resource: &str,
        id: &str,
        purchase_access: Option<&PurchaseAccess>,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        let result = match resource {
            "customer_receipt" => self
                .load_customer_receipt_detail(id, actor, executor)
                .await
                .map(|row| !row.data.permission_limited),
            "sales_invoice_request" => {
                self.load_request_detail(id, actor, executor).await.map(|row| !row.data.permission_limited)
            },
            "receivable_account" => self.guard_receivable_account(actor, id, executor).await.map(|()| true),
            "supplier_payment" => self
                .load_supplier_payment_detail(id, actor, required_purchase(purchase_access)?, executor)
                .await
                .map(|row| !row.data.permission_limited),
            "payable_account" => self
                .load_payable_account_detail(id, actor, required_purchase(purchase_access)?, executor)
                .await
                .map(|row| !row.data.permission_limited),
            "invoice" => self
                .load_invoice_detail(id, actor, required_purchase(purchase_access)?, executor)
                .await
                .map(|row| !row.data.permission_limited),
            _ => return Err(Error::ValidationError("不支持的财务来源类型".into())),
        };
        match result {
            Err(Error::NotFound(_) | Error::Forbidden(_)) => Ok(false),
            other => other,
        }
    }
}

/// 采购方向来源必须带已装配的领域访问器，不补公司范围。
fn required_purchase(access: Option<&PurchaseAccess>) -> Result<&PurchaseAccess> {
    access.ok_or_else(|| Error::Forbidden("财务来源读取缺少采购访问器".into()))
}
