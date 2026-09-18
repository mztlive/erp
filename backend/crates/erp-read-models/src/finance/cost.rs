//! 成本独立读取授权：成本资源范围与销售对象范围求交，全部裁剪后分页。
mod paging;
mod snapshot;

use application_core::{AuditActor, PageView};
use erp_finance::dto::cost::{
    CostAllocationListParams, CostAllocationView, CostEntryListParams, ScopedCostEntryView,
};
use erp_identity::SharedRbacService;
use mongodb::Database;
use serde::Serialize;

use crate::{Error, Result};

/// 查询直接复用领域 DTO；避免将 URL 数值参数经 serde flatten 再解码。
pub type CostReadParams = CostEntryListParams;
/// 分配读取的领域查询合同。
pub type AllocationReadParams = CostAllocationListParams;
/// 成本页面或详情的安全范围元信息，不返回原始授权集合。
#[derive(Debug, Serialize)]
pub struct CostReadResult<T> {
    #[serde(flatten)]
    pub data: T,
    pub scope_version: String,
    pub policy_version: u64,
    pub organization_version: u64,
    pub as_of: String,
    pub empty_reason: Option<&'static str>,
    pub scope_summary: &'static str,
    pub ownership_basis: &'static str,
}
#[derive(Clone)]
pub struct CostReadModel {
    db: Database,
    rbac: SharedRbacService,
}
impl CostReadModel {
    /// 绑定应用数据库及身份服务，不保存授权缓存。
    ///
    /// # 返回
    /// 返回成本读取服务；构造不执行 I/O。
    pub fn new(db: Database, rbac: SharedRbacService) -> Self {
        Self { db, rbac }
    }

    /// 以同一份授权分配生成列表、总数及稳定分页。
    ///
    /// # 错误
    /// 无权限、范围变化、业务参数非法或候选超限时拒绝。
    pub async fn list(
        &self,
        params: CostReadParams,
        actor: &AuditActor,
    ) -> Result<CostReadResult<PageView<ScopedCostEntryView>>> {
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let mut snapshot =
            self.checked(&params, None, "cost_entry", "list", actor, params.scope_version.as_deref()).await?;
        let rows = std::mem::take(&mut snapshot.rows);
        Ok(snapshot.result(paging::costs(rows, query.paging)))
    }

    /// 独立详情重新解析成本详情动作及销售对象读取范围。
    ///
    /// # 错误
    /// 不可见与不存在统一为 NotFound；撤权后不交付旧宽范围事实。
    pub async fn detail(&self, id: &str, actor: &AuditActor) -> Result<CostReadResult<ScopedCostEntryView>> {
        let mut snapshot =
            self.checked(&empty_query(), Some(id), "cost_entry", "detail", actor, None).await?;
        let row = snapshot.rows.pop().ok_or_else(|| Error::NotFound("成本不存在或无权查看".into()))?;
        Ok(snapshot.result(row))
    }
    /// 分配列表使用自己的读取权限，只输出当前授权且匹配业务条件的行。
    ///
    /// # 错误
    /// 动作缺失、范围版本变化或候选超限时拒绝，不按成本整笔金额推导份额。
    pub async fn allocations(
        &self,
        params: AllocationReadParams,
        actor: &AuditActor,
    ) -> Result<CostReadResult<PageView<CostAllocationView>>> {
        validator::Validate::validate(&params)?;
        let query = params.normalized()?;
        ensure_page(query.paging.page, params.scope_version.as_deref())?;
        let id = query.cost_entry_id.as_ref().map(|id| id.as_ref());
        let mut snapshot = self
            .checked(&empty_query(), id, "cost_allocation", "list", actor, params.scope_version.as_deref())
            .await?;
        let entries = std::mem::take(&mut snapshot.rows);
        let rows = entries.into_iter().flat_map(|entry| entry.allocations).collect();
        let page = paging::allocations(rows, &snapshot.allocation_created_at, &query)?;
        Ok(snapshot.result(page))
    }
}
/// 首页面后必须携带同一范围版本，禁止不同授权页拼接。
fn ensure_page(page: u64, version: Option<&str>) -> Result<()> {
    crate::support::ensure_deep_page(page, version)
}
/// 使用饱和乘法避免非法大页码溢出。
fn page_offset(page: u64, size: u32) -> usize {
    usize::try_from(page.saturating_sub(1).saturating_mul(u64::from(size))).unwrap_or(usize::MAX)
}
fn empty_query() -> CostEntryListParams {
    CostEntryListParams {
        scope_version: None,
        cost_type: None,
        cost_stage: None,
        cost_scope: None,
        supplier_id: None,
        source_document_id: None,
        page: None,
        page_size: None,
        sort_by: None,
        sort_dir: None,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn scoped_queries_accept_versions_but_never_ignore_unknown_person_filters() {
        let params: CostReadParams =
            serde_json::from_value(serde_json::json!({"page": 2, "scope_version": "v1"})).unwrap();
        assert_eq!(params.scope_version.as_deref(), Some("v1"));
        assert!(
            serde_json::from_value::<CostReadParams>(serde_json::json!({"owner_user_ids": "someone"}))
                .is_err()
        );
        assert!(
            serde_json::from_value::<AllocationReadParams>(serde_json::json!({"owner": "someone"})).is_err()
        );
    }
    #[test]
    fn later_pages_require_version_and_extreme_pages_never_wrap() {
        assert!(ensure_page(1, None).is_ok());
        assert!(ensure_page(2, None).is_err());
        assert!(ensure_page(2, Some("")).is_err());
        assert!(ensure_page(2, Some("v1")).is_ok());
        assert_eq!(page_offset(u64::MAX, 100), usize::MAX);
    }
}
