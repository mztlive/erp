//! 销售凭证按来源销售单授权，禁止由任意资产身份取得读取资格。
use application_core::AuditActor;
use erp_core::ids::BusinessDocumentId;
use erp_support::FileAssetExt;
use erp_support::repository::prelude::*;
use persistence_core::Transactional;

use super::SalesOrderReadService;
use crate::sales_center::access::SalesAccess;
use crate::{Error, Result};

impl SalesOrderReadService {
    /// 在同一事务中证明销售详情资格与精确建单凭证关联。
    ///
    /// # 参数
    /// * `actor` - 当前已认证用户
    /// * `order_id` / `asset_id` - 来源销售单与其中一份建单凭证
    /// # 返回
    /// 返回当前销售单版本，供读取文件后的来源一致性复验。
    /// # 错误
    /// 账号、范围或精确关系无效时拒绝。凭证不在销售单或附件中时返回 `NotFound`。
    /// 未注入授权源时返回 `Internal`。仓储或事务失败会返回对应错误。
    pub async fn require_sales_evidence(
        &self,
        actor: &AuditActor,
        order_id: &str,
        asset_id: &str,
    ) -> Result<u64> {
        let db = self.db.clone();
        let access = SalesAccess::new(db.clone(), self.require_rbac()?.clone());
        let actor = actor.clone();
        let order_id = order_id.to_string();
        let asset_id = asset_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let order = access.require_object(&actor, "detail", &order_id, &[], executor).await?;
                    if !order.evidence_file_asset_ids.iter().any(|id| id.as_ref() == asset_id) {
                        return Err(Error::NotFound("销售单凭证不存在或无权下载".into()));
                    }
                    let attachments = db
                        .document_attachments()
                        .list_by_document(&BusinessDocumentId::new(order_id), executor)
                        .await?;
                    if !attachments.iter().any(|attachment| attachment.file_asset_id.as_ref() == asset_id) {
                        return Err(Error::NotFound("销售单凭证关联不存在或已变化".into()));
                    }
                    Ok(order.base.version)
                })
            })
            .await
    }
}
