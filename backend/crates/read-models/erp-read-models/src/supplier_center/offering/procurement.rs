//! 供给列表采购负责人筛选：规则解析，不落主档。

use std::collections::{HashMap, HashSet};
use std::sync::Arc;

use async_trait::async_trait;
use erp_catalog::CatalogExt;
use erp_procurement::entity::procurement_responsibility::ProcurementResponsibilityRuleSet;
use erp_procurement::repository::ProcurementResponsibilityExt;
use erp_procurement::repository::prelude::*;
use erp_supply::SupplierOfferingExt;
use erp_supply::repository::supplier_offering::procurement::ProcurementOfferingRepositoryExt;
use mongodb::Database;
use persistence_core::Executor;

use crate::Result;
use crate::catalog_center::procurement::{
    ensure_authorized_count, load_products, load_revisions, match_sku_owner, prefetch_category_chains,
};

/// 在授权供给集合内按采购规则解析负责人。
#[async_trait]
pub trait OfferingProcurementOwners: Send + Sync {
    /// 应用供给采购负责人筛选时，给出采购负责人落在请求集合内的供给 ID。
    ///
    /// # 参数
    /// * `authorized_ids` - 已按完整条件裁剪的供给候选
    /// * `owner_user_ids` - 请求的采购负责人
    /// * `executor` - 与授权相同的执行器
    ///
    /// # 返回
    /// 返回命中的供给 ID。
    ///
    /// # 错误
    /// 授权集合超限时整体拒绝；缺规则的供给不命中。
    async fn matching_offering_ids(
        &self,
        authorized_ids: &[String],
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>>;
}

/// 生产实现：只读加载现行采购责任规则并按 SKU 批量 resolve。
pub struct MongoOfferingProcurementOwners {
    db: Database,
}

impl MongoOfferingProcurementOwners {
    /// 装配供给采购负责人筛选时，绑定采购规则与供给集合。
    ///
    /// # 返回
    /// 返回未执行 I/O 的解析器。
    pub fn new(db: Database) -> Self {
        Self { db }
    }

    /// 将供给采购负责人筛选的规则解析器包装为可注入的共享端口。
    ///
    /// # 返回
    /// 返回供给列表可注入的采购负责人解析器。
    pub fn shared(db: Database) -> Arc<dyn OfferingProcurementOwners> {
        Arc::new(Self::new(db))
    }
}

#[async_trait]
impl OfferingProcurementOwners for MongoOfferingProcurementOwners {
    async fn matching_offering_ids(
        &self,
        authorized_ids: &[String],
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        ensure_authorized_count(authorized_ids.len())?;
        if authorized_ids.is_empty() {
            return Ok(Vec::new());
        }

        let rules = self
            .db
            .procurement_responsibility_rules()
            .list_active_procurement_responsibility_rules(executor)
            .await?;

        let offerings = self.db.supplier_offerings().procurement_facts(authorized_ids, executor).await?;
        let sku_ids: Vec<String> = offerings.iter().map(|row| row.sku_id.clone()).collect();
        let skus = self.db.catalog().procurement_skus(&sku_ids, executor).await?;
        let product_ids = skus
            .iter()
            .map(|sku| sku.product_id.clone())
            .collect::<HashSet<_>>()
            .into_iter()
            .collect::<Vec<_>>();
        let products = load_products(&self.db, &product_ids, executor).await?;
        let revisions = load_revisions(&self.db, &products, executor).await?;

        let rule_set = ProcurementResponsibilityRuleSet::new(&rules);
        let mut categories = prefetch_category_chains(&self.db, &revisions, executor).await;
        let sku_by_id: HashMap<_, _> = skus.iter().map(|sku| (sku.id.as_str(), sku)).collect();

        let mut matched = HashSet::new();
        for offering in offerings {
            let Some(sku) = sku_by_id.get(offering.sku_id.as_str()) else {
                continue;
            };
            if match_sku_owner(&mut categories, &products, &revisions, &rule_set, sku, &wanted, executor)
                .await
                .is_some()
            {
                matched.insert(offering.id);
            }
        }
        Ok(matched.into_iter().collect())
    }
}

/// 内存桩：测试维护人与采购负责人分列。
#[derive(Default)]
pub struct MapOfferingProcurementOwners {
    /// 供给 ID 到采购负责人。
    pub owners: HashMap<String, String>,
}

#[async_trait]
impl OfferingProcurementOwners for MapOfferingProcurementOwners {
    async fn matching_offering_ids(
        &self,
        authorized_ids: &[String],
        owner_user_ids: &[String],
        _executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let wanted: HashSet<&str> = owner_user_ids.iter().map(String::as_str).collect();
        Ok(self
            .owners
            .iter()
            .filter(|(offering_id, owner)| {
                wanted.contains(owner.as_str()) && authorized_ids.iter().any(|id| id == *offering_id)
            })
            .map(|(id, _)| id.clone())
            .collect())
    }
}
