use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SourceServiceRegions, SupplierCatalogItem};
use erp_supply::ports::connector::common::{Lookup, SupplierSku};
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::repository::supplier_catalog::{self, CatalogMessageTarget};
use persistence_core::NoTransaction;

use super::{Context, SupplierCatalogCallbackProcess};
use crate::connectors::registry::SupplierConnectorRegistry;
use crate::{Error, Result};

pub(super) enum CatalogUpdate {
    Offer(Box<SupplierCatalogItem>),
    Regions { id: String, regions: SourceServiceRegions },
}

impl CatalogUpdate {
    /// 读取补查结果对应的稳定目录身份。
    /// # 参数
    /// 无。
    /// # 返回
    /// 来源目录主键。
    /// # 错误
    /// 不返回错误。
    pub(super) fn id(&self) -> &str {
        match self {
            Self::Offer(item) => &item.base.id,
            Self::Regions { id, .. } => id,
        }
    }
}

impl SupplierCatalogCallbackProcess {
    /// 读取当前提示的下一个本连接规格。
    /// # 参数
    /// 已领取的接收记录及已提交游标。
    /// # 返回
    /// 下一规格或分组已结束。
    /// # 错误
    /// 目录读取失败或提示非法时拒绝。
    pub(super) async fn next_target(&self, receipt: &SupplierCallbackReceipt) -> Result<Option<SupplierSku>> {
        let Some(hint) = receipt.catalog_hint()? else { return Ok(None) };
        let target = match hint.target_kind.as_str() {
            "sku" => {
                return Ok(Some(SupplierSku {
                    product_id: hint.product_id.clone(),
                    spec_id: hint.object_id.clone(),
                }));
            },
            "product" => CatalogMessageTarget::Product(&hint.object_id),
            "brand" => CatalogMessageTarget::Brand(&hint.object_id),
            _ => return Err(Error::ValidationError("商品回调目标类型不支持".into())),
        };
        let item = supplier_catalog::next_callback_target(
            &self.db,
            receipt.connection_id.as_ref(),
            target,
            receipt.catalog_progress.after_id.as_deref(),
            &mut NoTransaction,
        )
        .await?;
        Ok(item.map(|item| SupplierSku { product_id: Some(item.product_id), spec_id: item.spec_id }))
    }
}

/// 通过绑定协议补查当前事实，不读取回调正文业务值。
/// # 参数
/// 授权上下文、接收记录、目标规格和协议注册表。
/// # 返回
/// 待事务应用的商品或地区观察。
/// # 错误
/// 来源缺失、身份不符或事实不完整时拒绝。
pub(super) async fn query(
    context: &Context,
    receipt: &SupplierCallbackReceipt,
    sku: SupplierSku,
    registry: &SupplierConnectorRegistry,
) -> Result<CatalogUpdate> {
    let hint = receipt.catalog_hint()?.ok_or_else(|| Error::ConflictError("商品回调已完成".into()))?;
    if hint.topic == "region" {
        let areas = context
            .connector
            .service_areas()
            .ok_or_else(|| Error::BusinessLogicError("来源尚不支持可售地区补查".into()))?;
        let snapshot = areas.regions(&sku).await.map_err(|error| Error::BusinessLogicError(error.summary))?;
        let regions = SourceServiceRegions::queried(snapshot, receipt.received_at)?;
        let product =
            sku.product_id.as_deref().ok_or_else(|| Error::ValidationError("商品身份缺失".into()))?;
        return Ok(CatalogUpdate::Regions {
            id: SupplierCatalogItem::source_id(receipt.connection_id.as_ref(), product, &sku.spec_id),
            regions,
        });
    }
    query_offer(context, receipt, sku, registry).await
}

async fn query_offer(
    context: &Context,
    receipt: &SupplierCallbackReceipt,
    sku: SupplierSku,
    registry: &SupplierConnectorRegistry,
) -> Result<CatalogUpdate> {
    let source =
        context.connector.offers().ok_or_else(|| Error::BusinessLogicError("来源尚不支持商品补查".into()))?;
    let Lookup::Found(snapshot) =
        source.offer(&sku).await.map_err(|error| Error::BusinessLogicError(error.summary))?
    else {
        return Err(Error::BusinessLogicError("商品或规格暂不可见，请核实原来源；不按缺项下架".into()));
    };
    if snapshot.value.sku != sku || snapshot.stamp.observed_at < receipt.received_at {
        return Err(Error::ConflictError("商品补查身份或观察时间不一致".into()));
    }
    let mut item = SupplierCatalogItem::observed(
        &context.connection.base.id,
        context.connection.supplier_id.as_ref(),
        &receipt.base.id,
        snapshot,
    )?;
    if (receipt.route_kind == "status" && item.availability == CatalogAvailability::Unknown)
        || (receipt.route_kind == "price" && item.purchase_price.is_none())
    {
        return Err(Error::BusinessLogicError("商品补查未取得明确可售状态或采购报价".into()));
    }
    item.availability_valid_until = registry
        .catalog_availability_validity(&context.connection)
        .and_then(|seconds| i64::try_from(seconds).ok())
        .and_then(|seconds| item.observed_at.unix_secs().checked_add(seconds))
        .map(Instant::from_unix_secs);
    if item.images.as_ref().is_some_and(|images| !images.is_empty())
        || item.detail_images.as_ref().is_some_and(|images| !images.is_empty())
    {
        item.media_retry_at = Some(Instant::now().unix_secs());
    }
    Ok(CatalogUpdate::Offer(Box::new(item)))
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use application_core::AuditActor;
    use erp_core::AccountKind;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_integration::entity::supplier_callback::SupplierRefreshHint;
    use erp_supply::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use serde_json::{Value, json};

    use super::*;
    use crate::connectors::dangaoshushu::test_support::connector;

    fn context(values: Vec<Value>) -> Context {
        Context {
            connection: SupplierApiConnection::new(
                SupplierApiConnectionId::new("connection-1"),
                SupplierApiConnectionData {
                    supplier_id: SupplierAccountId::new("supplier-1"),
                    connection_code: "test".into(),
                    connector_type: SupplierConnectorType::Dangaoshushu,
                    endpoint_reference: "unbound".into(),
                    credential_reference: None,
                    rate_limit_policy: None,
                    status: SupplierApiConnectionStatus::Disabled,
                },
                "actor",
            )
            .unwrap(),
            connector: Arc::new(connector(values)),
            actor: AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin),
        }
    }
    fn receipt(route: &str) -> SupplierCallbackReceipt {
        let (topic, kind) = match route {
            "status" => ("availability", "brand"),
            "cities" => ("region", "product"),
            "price" => ("price", "sku"),
            _ => ("product", "product"),
        };
        SupplierCallbackReceipt::received(
            "receipt".into(),
            SupplierApiConnectionId::new("connection-1"),
            Instant::from_unix_secs(1),
            route.into(),
            "cipher-not-business-facts".into(),
            vec![SupplierRefreshHint {
                topic: topic.into(),
                target_kind: kind.into(),
                object_id: if route == "price" { "spec-1" } else { "product-1" }.into(),
                product_id: Some("product-1".into()),
                funds_transaction_id: None,
            }],
        )
        .unwrap()
    }
    fn sku() -> SupplierSku {
        SupplierSku { product_id: Some("product-1".into()), spec_id: "spec-1".into() }
    }
    fn product() -> Value {
        json!({"brand":{"status":"1"},"product":{"id":"product-1","title":"查询得到的新名称","status":"1","can_buy":"1"},
            "specs":[{"id":"spec-1","name":"6寸","clearing_price":"123.45","deleted":"0","can_buy":"1","stock":"5"}]})
    }

    #[tokio::test]
    async fn catalog_callbacks_query_actual_adapter_for_attributes_price_and_availability() {
        let registry = SupplierConnectorRegistry::default();
        for route in ["product", "price", "status"] {
            let mut response = product();
            if route == "status" {
                response["product"]["can_buy"] = json!("0");
            }
            let update = query(&context(vec![response]), &receipt(route), sku(), &registry).await.unwrap();
            let CatalogUpdate::Offer(item) = update else { panic!("商品补查") };
            assert_eq!(item.connection_id, "connection-1");
            assert_eq!(item.name, "查询得到的新名称");
            assert_eq!(item.purchase_price.as_deref(), Some("123.45"));
            assert_eq!(
                item.availability,
                if route == "status" {
                    CatalogAvailability::Unavailable
                } else {
                    CatalogAvailability::Available
                }
            );
            assert_eq!(item.job_id, "receipt");
        }
    }

    #[tokio::test]
    async fn catalog_callbacks_reject_incomplete_price_status_or_wrong_identity() {
        let registry = SupplierConnectorRegistry::default();
        for (route, field) in [("price", "clearing_price"), ("status", "can_buy")] {
            let mut response = product();
            response["specs"][0][field] = Value::Null;
            assert!(query(&context(vec![response]), &receipt(route), sku(), &registry).await.is_err());
        }
        let mut response = product();
        response["product"]["id"] = json!("another-product");
        assert!(query(&context(vec![response]), &receipt("product"), sku(), &registry).await.is_err());
        let mut future = receipt("price");
        future.received_at = Instant::from_unix_secs(Instant::now().unix_secs() + 100);
        assert!(query(&context(vec![product()]), &future, sku(), &registry).await.is_err());
        let mut response = product();
        response["specs"] = json!([]);
        assert!(query(&context(vec![response]), &receipt("product"), sku(), &registry).await.is_err());
    }

    #[tokio::test]
    async fn catalog_city_callback_uses_full_query_preserves_empty_and_rejects_partial_mapping() {
        let registry = SupplierConnectorRegistry::default();
        for (response, expected) in
            [(json!([]), vec![]), (json!([{"city_id":"2"}]), vec!["110100".to_string()])]
        {
            let update = query(&context(vec![response]), &receipt("cities"), sku(), &registry).await.unwrap();
            let CatalogUpdate::Regions { id, regions } = update else { panic!("地区补查") };
            assert_eq!(id, SupplierCatalogItem::source_id("connection-1", "product-1", "spec-1"));
            assert_eq!(regions.included, expected);
        }
        let response = json!([{"city_id":"2"},{"city_id":"unmapped"}]);
        assert!(query(&context(vec![response]), &receipt("cities"), sku(), &registry).await.is_err());
    }
}
