//! 开票申请协议。定义和审批人由服务端绑定，客户端不能指定。
use application_core::QueryIds;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use crate::entity::receivable::{InvoiceRequestData, InvoiceRequestStatus};

/// 原子创建并提交，或修改撤回后的草稿再提交。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SubmitInvoiceRequest {
    pub receivable_account_id: String,
    pub request_id: Option<String>,
    pub expected_version: Option<u64>,
    pub data: InvoiceRequestData,
    pub idempotency_key: String,
}
/// 撤回命令；保留同一载荷重试以恢复未知结果。
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct CancelInvoiceRequest {
    pub expected_version: u64,
    pub reason: String,
    pub idempotency_key: String,
}
/// 申请列表筛选，账户和销售单条件与状态取交集。
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct InvoiceRequestQuery {
    /// 跨页必须携带当前授权和业务版本。
    pub scope_version: Option<String>,
    /// 关联销售单当前负责销售，逗号分隔，最多 100 项；只收窄授权结果。
    pub sales_owner_user_ids: Option<QueryIds>,
    /// 申请人（申请创建人），逗号分隔，最多 100 项；只收窄授权结果。
    pub applicant_user_ids: Option<QueryIds>,
    /// 当前开票处理人（关联工作项当前负责人），逗号分隔，最多 100 项。
    pub handler_user_ids: Option<QueryIds>,
    /// 关联销售单当前业务组织，逗号分隔，最多 100 项；只收窄授权结果。
    pub org_unit_ids: Option<QueryIds>,
    /// 组织筛选是否包含有效下级；缺省为 false。
    pub include_descendants: Option<bool>,
    pub sales_order_id: Option<String>,
    pub customer_id: Option<String>,
    pub receivable_account_id: Option<String>,
    pub work_item_id: Option<String>,
    pub status: Option<InvoiceRequestStatus>,
    pub q: Option<String>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

impl InvoiceRequestQuery {
    /// 校验人员组织筛选组合；未知字段由反序列化直接拒绝。
    ///
    /// # 返回
    /// 组合合法时成功。
    ///
    /// # 错误
    /// 版本超长、包含下级但未提供组织时返回 `ValidationError`。
    pub fn validate_scope_filters(&self) -> crate::Result<()> {
        if self.scope_version.as_ref().is_some_and(|version| version.is_empty() || version.len() > 256) {
            return Err(crate::Error::ValidationError("范围版本非法".into()));
        }
        if self.include_descendants == Some(true) && self.org_unit_ids.is_none() {
            return Err(crate::Error::ValidationError("包含下级时必须提供组织筛选".into()));
        }
        Ok(())
    }
}
/// 销售应收的开票申请额度摘要。各字段含税，已登记不重复占用。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InvoiceRequestAmounts {
    pub receivable_account_id: String,
    pub available_amount: Amount,
    pub pending_amount: Amount,
    pub approved_remaining_amount: Amount,
    pub invoiced_amount: Amount,
}
