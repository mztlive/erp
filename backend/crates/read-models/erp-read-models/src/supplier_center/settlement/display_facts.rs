//! 只读取当前已授权结果引用的关联身份，批量组装结算名称。

use std::collections::HashMap;

use erp_catalog::CatalogExt;
use erp_catalog::repository::prelude::*;
use erp_core::ids::{SkuId, SkuRevisionId, SupplierAccountId, SupplierOfferingRevisionId};
use erp_identity::AccessControlExt;
use erp_identity::repository::prelude::*;
use erp_supply::dto::supplier_settlement::{SupplierSettlementItemView, SupplierSettlementStatementView};
use erp_supply::repository::SupplierFulfillmentExt;
use persistence_core::NoTransaction;

use super::SupplierSettlementReadService;
use super::display_dto::SettlementItemDisplayView;
use super::display_mapping::{
    ItemDisplayFacts, OrderDisplayLabels, StatementDisplayFacts, readable_name, readable_value,
};
use crate::Result;
use crate::purchase_center::repository::supplier_names::current_legal_names_by_account_ids;
use crate::support::dedup_trimmed_nonempty;

impl SupplierSettlementReadService {
    /// 为已完成命令读取附加名称，不改变命令成功结果。
    ///
    /// # 参数
    /// * `statements` - 当前命令已授权的结算单事实。
    /// * `extra_user_ids` - 同一结果引用的补证或任务人员。
    /// # 返回
    /// 返回稀疏名称映射；附加读取失败时为空。
    /// # 错误
    /// 不返回错误。
    pub(super) async fn command_display_facts(
        &self,
        statements: &[&SupplierSettlementStatementView],
        extra_user_ids: Vec<String>,
    ) -> StatementDisplayFacts {
        match self.statement_display_facts(statements, extra_user_ids).await {
            Ok(facts) => facts,
            Err(_) => {
                tracing::warn!("供应商结算正式命令已完成，附加名称读取失败");
                StatementDisplayFacts::default()
            },
        }
    }

    /// 按当前授权结果的精确身份批量读取名称。
    ///
    /// # 参数
    /// * `statements` - 当前结算单页或详情。
    /// * `extra_user_ids` - 同一详情引用的补证或任务人员。
    /// # 返回
    /// 返回供应商及人员名称映射。
    /// # 错误
    /// 任一拥有领域读取失败时返回仓储错误。
    pub(super) async fn statement_display_facts(
        &self,
        statements: &[&SupplierSettlementStatementView],
        extra_user_ids: Vec<String>,
    ) -> Result<StatementDisplayFacts> {
        let supplier_ids = dedup_trimmed_nonempty(statements.iter().map(|value| value.supplier_id.as_str()))
            .into_iter()
            .map(SupplierAccountId::new)
            .collect::<Vec<_>>();
        let user_ids = dedup_trimmed_nonempty(
            statements
                .iter()
                .flat_map(|statement| {
                    [
                        Some(statement.prepared_by.as_str()),
                        Some(statement.difference_handler_user_id.as_str()),
                        statement.reviewed_by.as_deref(),
                    ]
                    .into_iter()
                    .flatten()
                })
                .chain(extra_user_ids.iter().map(String::as_str)),
        );
        let supplier_names = if supplier_ids.is_empty() {
            HashMap::new()
        } else {
            current_legal_names_by_account_ids(&self.db, &supplier_ids, &mut NoTransaction).await?
        };
        let user_names = self.db.accounts().names_by_ids(&user_ids, &mut NoTransaction).await?;
        Ok(StatementDisplayFacts { supplier_names, user_names })
    }

    /// 对当前结算明细集合补齐订单和商品名称。
    ///
    /// # 参数
    /// * `items` - 当前授权查询返回的明细。
    /// # 返回
    /// 返回同一顺序的名称视图。
    /// # 错误
    /// 关联事实读取失败时返回仓储错误。
    pub(super) async fn item_display_views(
        &self,
        items: Vec<SupplierSettlementItemView>,
    ) -> Result<Vec<SettlementItemDisplayView>> {
        if items.is_empty() {
            return Ok(Vec::new());
        }
        let facts = ItemDisplayFacts {
            orders: self.settlement_order_labels(&items).await?,
            item_names: self.settlement_item_names(&items).await?,
        };
        Ok(items.into_iter().map(|item| facts.item(item)).collect())
    }

    async fn settlement_order_labels(
        &self,
        items: &[SupplierSettlementItemView],
    ) -> Result<HashMap<String, OrderDisplayLabels>> {
        let order_ids =
            dedup_trimmed_nonempty(items.iter().map(|item| item.supplier_fulfillment_order_id.as_str()));
        let orders =
            self.db.supplier_fulfillment_orders().list_active_by_ids(&order_ids, &mut NoTransaction).await?;
        Ok(orders
            .into_iter()
            .map(|order| {
                let supplier_order_no = readable_value(&order.fulfillment_order_no, &order.base.id);
                let external_order_no =
                    order.external_order_no.as_deref().and_then(|no| readable_value(no, &order.base.id));
                (order.base.id, OrderDisplayLabels { supplier_order_no, external_order_no })
            })
            .collect())
    }

    async fn settlement_item_names(
        &self,
        items: &[SupplierSettlementItemView],
    ) -> Result<HashMap<String, (String, String)>> {
        let item_ids =
            dedup_trimmed_nonempty(items.iter().map(|item| item.supplier_fulfillment_item_id.as_str()));
        let fulfillment_items =
            self.db.supplier_fulfillment_items().list_active_by_ids(&item_ids, &mut NoTransaction).await?;
        let revision_ids = dedup_trimmed_nonempty(
            fulfillment_items.iter().map(|item| item.supplier_offering_revision_id.as_ref()),
        )
        .into_iter()
        .map(SupplierOfferingRevisionId::new)
        .collect::<Vec<_>>();
        let offerings = self
            .db
            .supplier_fulfillment()
            .load_offerings_by_revision_ids(&revision_ids, &mut NoTransaction)
            .await?;
        let sku_names = self
            .settlement_sku_names(offerings.values().map(|offering| offering.sku_id.clone()).collect())
            .await?;
        Ok(fulfillment_items
            .into_iter()
            .filter_map(|item| {
                let offering = offerings.get(item.supplier_offering_revision_id.as_ref())?;
                let name = readable_name(&sku_names, offering.sku_id.as_ref())?;
                Some((item.base.id, (item.supplier_fulfillment_order_id.to_string(), name)))
            })
            .collect())
    }

    async fn settlement_sku_names(&self, sku_ids: Vec<SkuId>) -> Result<HashMap<String, String>> {
        let skus = self.db.skus().find_by_ids(&sku_ids, &mut NoTransaction).await?;
        let revision_ids =
            dedup_trimmed_nonempty(skus.iter().filter_map(|sku| sku.stable.current_revision_id.as_deref()))
                .into_iter()
                .map(SkuRevisionId::new)
                .collect::<Vec<_>>();
        let revisions = self
            .db
            .sku_revisions()
            .find_by_ids(&revision_ids, &mut NoTransaction)
            .await?
            .into_iter()
            .map(|revision| (revision.base.id.clone(), revision))
            .collect::<HashMap<_, _>>();
        Ok(skus
            .into_iter()
            .filter_map(|sku| {
                let revision = revisions.get(sku.stable.current_revision_id.as_deref()?)?;
                if revision.sku_id.as_ref() != sku.base.id {
                    return None;
                }
                readable_value(&revision.name, &sku.base.id).map(|name| (sku.base.id, name))
            })
            .collect())
    }
}
