//! 采购创建依据。旧采购确认批次已删除，选源由采购单创建路径自行承担。

use super::dto::{CreatePurchaseOrderFromBasisRequest, CreatePurchaseOrderResult, CreationBasisView};
use super::PurchaseOrderService;
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl PurchaseOrderService {
    /// 旧采购确认创建依据已删除，恒返回空列表。
    ///
    /// # 返回
    /// 空列表。
    pub async fn creation_basis_list(&self) -> Result<Vec<CreationBasisView>> {
        Ok(Vec::new())
    }

    /// 旧依据建单入口已删除。
    ///
    /// # 错误
    /// 恒返回未找到，不得回退旧确认批次。
    pub async fn create_from_basis(
        &self,
        _req: CreatePurchaseOrderFromBasisRequest,
        _actor: &AuditActor,
    ) -> Result<CreatePurchaseOrderResult> {
        Err(Error::NotFound("采购创建依据不存在".to_string()))
    }
}
