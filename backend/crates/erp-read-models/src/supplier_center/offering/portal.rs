//! 门户供给读取：供应商归属固定由已验证身份传入，响应使用外部字段允许列表。

use application_core::{PageView, normalized_text, page_or_default, page_size_or_default};
use erp_catalog::CatalogExt;
use erp_core::ids::{SupplierAccountId, SupplierOfferingId};
use erp_identity::PortalActor;
use erp_supply::dto::supplier_offering::SupplierOfferingTermsWrite;
use erp_supply::entity::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, OfferingStatus, SupplierOfferingRevision,
};
use erp_supply::repository::SupplierOfferingExt;
use erp_supply::repository::prelude::*;
use mongodb::Database;
use persistence_core::{Executor, NoTransaction};
use serde::{Deserialize, Serialize};

use super::{SupplierOfferingView, page_view, repository_page};
use crate::supplier_center::repository::offering::SupplierOfferingListQuery;
use crate::supplier_portal::{PortalCatalogSku, portal_sku_image_source};
use crate::{Error, Result};

/// 外部供给筛选，不能接受供应商或内部责任筛选。
#[derive(Debug, Default, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct PortalOfferingParams {
    pub q: Option<String>,
    pub status: Option<OfferingStatus>,
    pub availability_status: Option<AvailabilityStatus>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

/// 供应商自己的供给字段；不承载销售价格、其他供应商或内部经营资料。
#[derive(Debug, Serialize)]
pub struct PortalOfferingView {
    pub id: String,
    pub sku_id: String,
    pub sku_no: Option<String>,
    pub product_no: Option<String>,
    pub name: Option<String>,
    pub specification: Option<String>,
    pub image_asset_id: Option<String>,
    pub unit_name: Option<String>,
    pub supplier_sku_code: String,
    pub source_type: OfferingSourceType,
    pub status: OfferingStatus,
    pub current_revision_id: Option<String>,
    pub current_revision_no: Option<u32>,
    pub version: u64,
    pub terms: Option<SupplierOfferingTermsWrite>,
    pub availability_status: Option<AvailabilityStatus>,
    pub available_quantity: Option<String>,
    pub availability_source_updated_at: Option<i64>,
    pub availability_version: Option<u64>,
    pub updated_at: Option<i64>,
    pub writable: bool,
}

/// 外部不可变条款历史。
#[derive(Debug, Serialize)]
pub struct PortalRevisionView {
    pub id: String,
    pub revision_no: u32,
    pub terms: SupplierOfferingTermsWrite,
    pub created_at: u64,
}

/// 门户供给跨域查询入口；会话及供应商启用状态由每次请求的认证入口重验。
pub struct PortalOfferingReadService {
    db: Database,
}

impl PortalOfferingReadService {
    /// 绑定供给读取所需的数据库。
    /// # 参数
    /// `db` 为组合根提供的数据库。
    /// # 返回
    /// 返回查询入口。
    /// # 错误
    /// 无。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 读取当前供应商的供给分页与统计。
    /// # 参数
    /// `actor` 为当前验证身份，`params` 仅收窄该供应商结果。
    /// # 返回
    /// 返回允许列表字段的分页。
    /// # 错误
    /// 分页非法或读取失败时拒绝。
    pub async fn offerings(
        &self,
        actor: &PortalActor,
        params: &PortalOfferingParams,
    ) -> Result<PageView<PortalOfferingView>> {
        let query = portal_query(&actor.supplier_id, params)?;
        let bundle = repository_page(&self.db, &query, &mut NoTransaction).await?;
        let page = page_view(bundle, &query)?;
        let mut items = Vec::with_capacity(page.items.len());
        for view in page.items {
            items.push(self.external_view(view, &mut NoTransaction).await?);
        }
        Ok(PageView { items, total: page.total, page: page.page, page_size: page.page_size })
    }

    /// 按当前供应商范围读取详情，未知及越权目标使用相同错误。
    /// # 参数
    /// `actor` 为验证身份，`id` 为供给标识。
    /// # 返回
    /// 返回该供应商必要的供给资料。
    /// # 错误
    /// 不存在或属于其他供应商时返回相同 `NotFound`。仓储或下层读取失败会返回对应错误。
    pub async fn offering(&self, actor: &PortalActor, id: &str) -> Result<PortalOfferingView> {
        self.require_owned(actor, id, &mut NoTransaction).await?;
        let query = SupplierOfferingListQuery {
            supplier_id: Some(SupplierAccountId::new(&actor.supplier_id)),
            offering_ids: Some(vec![SupplierOfferingId::new(id)]),
            page_size: 1,
            ..Default::default()
        };
        let page = page_view(repository_page(&self.db, &query, &mut NoTransaction).await?, &query)?;
        let view = page.items.into_iter().next().ok_or_else(hidden_target)?;
        self.external_view(view, &mut NoTransaction).await
    }

    /// 读取自己供给的历史条款，先验证供给归属再查询历史。
    /// # 参数
    /// `actor` 为验证身份，`id` 为供给标识。
    /// # 返回
    /// 返回价格及供货条件的历史。
    /// # 错误
    /// 不存在、越权或仓储失败时拒绝。
    pub async fn revisions(&self, actor: &PortalActor, id: &str) -> Result<Vec<PortalRevisionView>> {
        self.require_owned(actor, id, &mut NoTransaction).await?;
        let revisions = self
            .db
            .supplier_offering_revisions()
            .list_publication_offering_revisions(&SupplierOfferingId::new(id), &mut NoTransaction)
            .await?;
        Ok(revisions.into_iter().map(PortalRevisionView::from_revision).collect())
    }

    async fn require_owned(&self, actor: &PortalActor, id: &str, executor: &mut dyn Executor) -> Result<()> {
        let owned = self
            .db
            .supplier_offerings()
            .find_by_id(id, executor)
            .await?
            .is_some_and(|offering| offering.supplier_id.as_ref() == actor.supplier_id);
        if !owned {
            return Err(hidden_target());
        }
        Ok(())
    }

    async fn external_view(
        &self,
        view: SupplierOfferingView,
        executor: &mut dyn Executor,
    ) -> Result<PortalOfferingView> {
        let sku = self.db.skus().find_by_id(&view.sku_id, executor).await?;
        let specification =
            sku.as_ref().and_then(|sku| PortalCatalogSku::specification_text(&sku.specification_signature));
        let image_asset_id =
            portal_sku_image_source(&self.db, &view.sku_id, executor).await?.map(|source| source.file_id);
        let unit_name = match sku {
            Some(sku) => self
                .db
                .unit_of_measures()
                .find_by_id(sku.base_unit_id.as_ref(), executor)
                .await?
                .map(|unit| unit.name),
            None => None,
        };
        Ok(PortalOfferingView::from_internal(view, specification, image_asset_id, unit_name))
    }
}

fn hidden_target() -> Error {
    Error::NotFound("供给不存在或无权查看".into())
}

fn portal_query(supplier_id: &str, params: &PortalOfferingParams) -> Result<SupplierOfferingListQuery> {
    if params.page == Some(0) || params.page_size.is_some_and(|size| size == 0 || size > 100) {
        return Err(Error::ValidationError("分页大小必须在1至100之间，页码从1开始".into()));
    }
    Ok(SupplierOfferingListQuery {
        supplier_id: Some(SupplierAccountId::new(supplier_id)),
        keyword: normalized_text(params.q.as_deref()),
        status: params.status,
        availability_status: params.availability_status,
        page: page_or_default(params.page),
        page_size: page_size_or_default(params.page_size),
        ..Default::default()
    })
}

impl PortalOfferingView {
    fn from_internal(
        view: SupplierOfferingView,
        specification: Option<String>,
        image_asset_id: Option<String>,
        unit_name: Option<String>,
    ) -> Self {
        let terms = terms_from_view(&view);
        Self {
            id: view.id,
            sku_id: view.sku_id,
            sku_no: view.sku_no,
            product_no: view.product_no,
            name: view.sku_name,
            specification,
            image_asset_id,
            unit_name,
            supplier_sku_code: view.supplier_sku_code,
            source_type: view.source_type,
            status: view.status,
            current_revision_id: view.current_revision_id,
            current_revision_no: view.current_revision_no,
            version: view.version,
            terms,
            availability_status: view.availability_status,
            available_quantity: view.available_quantity,
            availability_source_updated_at: view.availability_source_updated_at,
            availability_version: view.availability_version,
            updated_at: view.availability_source_updated_at,
            writable: view.source_type != OfferingSourceType::Api,
        }
    }
}

fn terms_from_view(view: &SupplierOfferingView) -> Option<SupplierOfferingTermsWrite> {
    Some(SupplierOfferingTermsWrite {
        dropship_supply_price_gross: view.dropship_supply_price_gross.clone()?,
        bulk_supply_price_gross: view.bulk_supply_price_gross.clone()?,
        input_tax_rate: view.input_tax_rate.clone()?,
        bulk_minimum_order_quantity: view.bulk_minimum_order_quantity.clone()?,
        supply_region: view.supply_region.clone(),
        product_capabilities: view.product_capabilities.clone(),
        valid_from: view.valid_from.clone()?,
        valid_to: view.valid_to.clone(),
        dropship_express: view.dropship_express.clone(),
        freight_amount: view.freight_amount.clone(),
        service_fee_amount: view.service_fee_amount.clone(),
    })
}

impl PortalRevisionView {
    fn from_revision(revision: SupplierOfferingRevision) -> Self {
        Self {
            id: revision.base.id,
            revision_no: revision.revision.revision_no,
            created_at: revision.base.created_at,
            terms: SupplierOfferingTermsWrite {
                dropship_supply_price_gross: revision.dropship_supply_price_gross.to_string(),
                bulk_supply_price_gross: revision.bulk_supply_price_gross.to_string(),
                input_tax_rate: revision.input_tax_rate.to_string(),
                bulk_minimum_order_quantity: revision.bulk_minimum_order_quantity.to_string(),
                supply_region: revision.supply_region,
                product_capabilities: revision.product_capabilities,
                valid_from: revision.valid_from.to_string(),
                valid_to: revision.valid_to.map(|date| date.to_string()),
                dropship_express: revision.dropship_express,
                freight_amount: revision.freight_amount.map(|value| value.to_string()),
                service_fee_amount: revision.service_fee_amount.map(|value| value.to_string()),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn filters_keep_bound_supplier_and_reject_foreign_scope_inputs() {
        let params: PortalOfferingParams =
            serde_json::from_value(serde_json::json!({"q":" ABC ","page":2})).unwrap();
        let query = portal_query("supplier-a", &params).unwrap();
        assert_eq!(query.supplier_id.unwrap().as_ref(), "supplier-a");
        assert_eq!(query.keyword.as_deref(), Some("ABC"));
        assert!(
            serde_json::from_value::<PortalOfferingParams>(serde_json::json!({"supplier_id":"supplier-b"}))
                .is_err()
        );
        assert!(
            portal_query("supplier-a", &PortalOfferingParams { page_size: Some(101), ..Default::default() })
                .is_err()
        );
    }
}
