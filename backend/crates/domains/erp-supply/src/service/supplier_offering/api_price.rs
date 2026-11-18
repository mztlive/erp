//! API 来源报价的创建校验与同步落盘，沿用调用方事务。
use application_core::AuditActor;
use erp_core::ids::SupplierOfferingRevisionId;
use id_generator::next_id;
use persistence_core::Executor;

use super::SupplierOfferingService;
use crate::entity::supplier_catalog::SupplierCatalogItem;
use crate::entity::supplier_offering::{OfferingSourceType, SupplierOffering, SupplierOfferingRevision};
use crate::repository::SupplierOfferingExt;
use crate::repository::supplier_catalog::SupplierCatalogExt;
use crate::{Error, Result};

impl SupplierOfferingService {
    /// 创建 API 供给时在同一执行器内核验来源采购价，防止绕过选品入口改价。
    /// # 参数
    /// `offering`、`revision` 为待持久化事实，`executor` 为实际写入事务。
    /// # 返回
    /// 非 API 来源直接通过；API 来源必须与供应商目录报价一致。
    /// # 错误
    /// 缺少对应来源目录、采购价被改写或编造集采条件时拒绝。
    pub(super) async fn ensure_api_create_price(
        &self,
        offering: &SupplierOffering,
        revision: &SupplierOfferingRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if offering.source_type != OfferingSourceType::Api {
            return Ok(());
        }
        self.api_catalog_item(offering, executor).await?.ensure_supply_prices(revision)
    }

    /// 将服务器读取的来源采购约束绑定到待创建修订。
    /// # 参数
    /// `offering` 为已核验连接的供给，`revision` 为待写修订，执行器沿用创建事务。
    /// # 返回
    /// API 供给保存真实来源条件，普通供给保持原语义。
    /// # 错误
    /// 来源缺失、采购价或来源税率被改写时拒绝。
    pub(super) async fn bind_api_purchase_terms(
        &self,
        offering: &SupplierOffering,
        revision: &mut SupplierOfferingRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if offering.source_type == OfferingSourceType::Api {
            self.api_catalog_item(offering, executor).await?.bind_purchase_terms(revision)?;
        }
        Ok(())
    }

    async fn api_catalog_item(
        &self,
        offering: &SupplierOffering,
        executor: &mut dyn Executor,
    ) -> Result<SupplierCatalogItem> {
        let connection = offering
            .source_connection_id
            .as_ref()
            .ok_or_else(|| Error::ValidationError("API 供给缺少来源连接".into()))?;
        let product = offering
            .supplier_product_code
            .as_deref()
            .ok_or_else(|| Error::ValidationError("API 供给缺少来源商品编号".into()))?;
        let id = SupplierCatalogItem::source_id(connection.as_ref(), product, &offering.supplier_sku_code);
        self.db
            .supplier_catalog_items()
            .find_by_id(&id, executor)
            .await?
            .filter(|item| item.supplier_id == offering.supplier_id.as_ref())
            .ok_or_else(|| Error::ValidationError("API 供给缺少对应的供应商目录，请先同步商品".into()))
    }

    /// 将已核验的来源报价同步到供给修订，不修改状态、公司销售价或历史修订。
    /// # 参数
    /// `offering` 与 `item` 已由目录同步用例核验来源身份，`actor` 为同步操作人；
    /// `executor` 沿用目录、可供及审计所在事务。
    /// # 返回
    /// 报价变化时追加修订并更新当前指针，未变化或缺报价时不写修订。
    /// # 错误
    /// 来源类型错误、当前修订缺失、非法报价、版本冲突或持久化失败时拒绝。
    pub async fn sync_source_price(
        &self,
        offering: &mut SupplierOffering,
        item: &SupplierCatalogItem,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        if offering.source_type != OfferingSourceType::Api {
            return Err(Error::ValidationError("仅 API 来源供给允许同步采购报价".into()));
        }
        if item.requires_identity_review || item.purchase_price.is_none() {
            return Ok(());
        }

        let id = offering
            .stable
            .current_revision_id
            .as_deref()
            .ok_or_else(|| Error::BusinessLogicError("API 供给当前条款缺失".into()))?;
        let current = self
            .db
            .supplier_offering_revisions()
            .find_by_id(id, executor)
            .await?
            .ok_or_else(|| Error::NotFound("API 供给当前条款不存在".into()))?;

        let current_no = self.current_revision_no(offering, executor).await?;
        let next_no = offering.next_revision_no(current_no, current_no)?;
        let Some(next) =
            current.with_source_observation(SupplierOfferingRevisionId::new(next_id()), next_no, item)?
        else {
            return Ok(());
        };

        offering.stable.current_revision_id = Some(next.base.id.clone());
        offering.stable.updated_by = actor.id().into();
        self.db.supplier_offering_repository().append_revision(offering, &next, executor).await?;
        Ok(())
    }
}
