//! 采购上下文文件读取证明；精确采购资格、销售版本和冻结材料允许清单在同一事务内读取。

use application_core::AuditActor;
use erp_procurement::dto::purchase_order::PurchaseOrderLineView;
use erp_procurement::service::purchase_order::view_mapping::{
    revision_line_to_view, submission_line_to_view,
};
use erp_workflow::entity::approval_integration::ApprovalMaterialFile;
use persistence_core::Transactional;

use super::super::PurchaseOrderReadService;
use super::super::repository::{PurchaseOrderCenterFacts, load_purchase_order_center_facts};
use super::source_revision;
use crate::sales_center::materials::{revision_materials, validate_materials};
use crate::{Error, Result};

/// 只在服务器使用的精确采购来源证明，不得序列化为客户端资产读取资格。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PurchaseSalesMaterialReference {
    /// 冻结材料元数据，文件实际字节读取前必须与当前资产再次比较。
    pub file: ApprovalMaterialFile,
    /// 绑定当前采购范围、采购内容版本及其精确销售版本。
    relation_version: String,
}

impl PurchaseOrderReadService {
    /// 在同一事务中证明采购详情及关联销售版本的精确材料资格。
    ///
    /// # 参数
    /// * `actor` / `order_id` - 已认证账号及其有权读取的采购单。
    /// * `asset_id` - 只能来自本采购关联销售版本材料清单的文件引用。
    /// # 返回
    /// 返回冻结文件与关系版本，供存储读取前后比对；不要求普通销售或合同读取权限。
    /// # 错误
    /// 采购不可见、跨采购材料、版本关系缺失、文件内容变化或治理关闭时拒绝。
    pub async fn require_sales_material(
        &self,
        actor: &AuditActor,
        order_id: &str,
        asset_id: &str,
    ) -> Result<PurchaseSalesMaterialReference> {
        let db = self.db.clone();
        let access = self.access();
        let actor = actor.clone();
        let order_id = order_id.to_string();
        let asset_id = asset_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let (order, access_version) = access.resolve_detail(&actor, &order_id, executor).await?;
                    let facts = load_purchase_order_center_facts(&db, &order_id, executor).await?;
                    let revision = source_revision(&db, &order, &displayed_lines(&facts), executor).await?;
                    let materials = revision_materials(&db, &revision, executor).await?;
                    let file = materials
                        .files
                        .into_iter()
                        .find(|file| file.file_asset_id.as_ref() == asset_id)
                        .ok_or_else(|| Error::NotFound("采购关联销售材料不存在或无权下载".into()))?;
                    validate_materials(&db, std::slice::from_ref(&file), executor).await?;
                    Ok(PurchaseSalesMaterialReference {
                        file,
                        relation_version: format!(
                            "{access_version}:{}:{}",
                            revision.base.id, revision.content_hash
                        ),
                    })
                })
            })
            .await
    }
}

/// 文件资格沿采购页面实际展示的版本 > 提交内容优先级解释，不取当前销售草稿。
fn displayed_lines(facts: &PurchaseOrderCenterFacts) -> Vec<PurchaseOrderLineView> {
    if facts.current_revision.is_some() {
        facts.revision_lines.iter().map(revision_line_to_view).collect()
    } else {
        facts.submission_lines.iter().map(submission_line_to_view).collect()
    }
}
