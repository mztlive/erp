//! 内部开通与定向开放列表的安全视图，始终重验供应商范围。

use application_core::{AuditActor, PageView};
use erp_identity::PortalAccountView;
use persistence_core::NoTransaction;
use serde::{Deserialize, Serialize};

use super::repository::management::{accounts_page, grants_page};
use super::repository::query::PortalQuery;
use super::{PortalAdminListParams, PortalListParams, SupplierPortalReadService};
use crate::{Error, Result};

/// 内部查看的定向开放关系，不包含公司销售价格或任何供应商报价。
#[derive(Debug, Serialize, Deserialize)]
pub struct PortalGrantView {
    pub id: String,
    pub supplier_id: String,
    pub sku_id: String,
    pub active: bool,
    pub version: u64,
    pub sku_no: Option<String>,
    pub name: Option<String>,
    pub specification: Option<String>,
}

impl SupplierPortalReadService {
    /// 查看已授权供应商的实名门户账号，过滤哈希及内部身份事实。
    ///
    /// # 参数
    /// `actor` 为内部账号；`params` 指定供应商及有界分页、账号名搜索和启停状态。
    /// # 返回
    /// 包含固定门户岗位、启停及会话版本的安全分页视图。
    /// # 错误
    /// 内部资格、供应商范围、筛选或查询失败时拒绝。
    pub async fn accounts(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<PortalAccountView>> {
        self.require_management_supplier(actor, &params.supplier_id).await?;
        let (query, active) = management_query(params)?;
        accounts_page(&self.db, &params.supplier_id, &query, active, &mut NoTransaction).await
    }

    /// 查看已授权供应商的定向 SKU 开放关系及必要商品资料。
    ///
    /// # 参数
    /// `actor` 为内部账号；`params` 指定供应商及有界分页、商品搜索和启停状态。
    /// # 返回
    /// 当前及已撤销开放关系的分页，不包含其他报价或销售价。
    /// # 错误
    /// 内部资格、供应商范围、筛选或查询失败时拒绝。
    pub async fn grants(
        &self,
        actor: &AuditActor,
        params: &PortalAdminListParams,
    ) -> Result<PageView<PortalGrantView>> {
        self.require_management_supplier(actor, &params.supplier_id).await?;
        let (query, active) = management_query(params)?;
        grants_page(&self.db, &params.supplier_id, &query, active, &mut NoTransaction).await
    }

    /// 所有内部列表读取先证明同一正式供应商详情资格。
    async fn require_management_supplier(&self, actor: &AuditActor, supplier_id: &str) -> Result<()> {
        if supplier_id.trim().is_empty()
            || !self
                .internal_authorization(actor)?
                .supplier_readable(actor, supplier_id, &mut NoTransaction)
                .await?
        {
            return Err(Error::NotFound("供应商不存在或无权查看".into()));
        }
        Ok(())
    }
}

/// 统一分页与字面量搜索，管理状态只支持明确启停。
fn management_query(params: &PortalAdminListParams) -> Result<(PortalQuery, Option<bool>)> {
    let query = PortalQuery::new(&PortalListParams {
        q: params.q.clone(),
        status: None,
        page: params.page,
        page_size: params.page_size,
    })?;
    let active = match params
        .status
        .as_deref()
        .map(str::trim)
        .filter(|status| !status.is_empty())
        .map(str::to_ascii_uppercase)
        .as_deref()
    {
        None => None,
        Some("ACTIVE") => Some(true),
        Some("INACTIVE") => Some(false),
        Some(_) => return Err(Error::ValidationError("启停状态须为 ACTIVE 或 INACTIVE".into())),
    };
    Ok((query, active))
}

#[cfg(test)]
mod tests {
    use super::{PortalAdminListParams, management_query};

    #[test]
    fn management_status_has_no_application_or_internal_identity_aliases() {
        let mut params = PortalAdminListParams {
            supplier_id: "supplier1".into(),
            q: Some("  名称  ".into()),
            status: Some("inactive".into()),
            page: Some(2),
            page_size: Some(20),
        };
        let (query, active) = management_query(&params).unwrap();
        assert_eq!((query.page, query.skip, query.q.as_deref(), active), (2, 20, Some("名称"), Some(false)));
        params.status = Some("SUSPENDED".into());
        assert!(management_query(&params).is_err());
    }
}
