use erp_core::common::time::Instant;
use erp_integration::entity::supplier_callback::SupplierCallbackReceipt;
use erp_supply::connector_registry::SupplierConnectorRegistry;
use erp_supply::entity::supplier_api::SupplierApiConnection;
use erp_supply::entity::supplier_catalog::{CatalogAvailability, SourceServiceRegions, SupplierCatalogItem};
use erp_supply::ports::connector::SupplierConnector;
use erp_supply::ports::connector::common::{Lookup, SupplierSku};
use erp_supply::ports::supplier_api_gateway::SupplierApiGateway;
use erp_supply::repository::supplier_catalog::{self, CatalogMessageTarget};
use persistence_core::NoTransaction;

use super::{Context, SupplierCatalogCallbackProcess};
use crate::{Error, Result};

/// 已经补查的来源观察；仍须在原事务中重验授权、租约和版本后才能应用。
pub enum CatalogUpdate {
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
    query_current(&context.connection, context.connector.as_ref(), receipt, sku, registry).await
}

/// 使用已绑定协议补查回调目标，不把推送正文当成业务事实。
/// # 参数
/// `connection`、`connector` 为调用方已授权并核对绑定的服务端上下文；
/// `receipt`、`sku` 为原回执和待补查目标；`registry` 提供原可售有效期策略。
/// # 返回
/// 返回经来源身份、观察时间和完整性校验的商品或地区观察，不写业务数据。
/// # 错误
/// 原回执失效、来源缺失、身份不符或报价及可售事实不完整时返回原错误。
pub async fn query_current(
    connection: &SupplierApiConnection,
    connector: &dyn SupplierConnector,
    receipt: &SupplierCallbackReceipt,
    sku: SupplierSku,
    registry: &SupplierConnectorRegistry,
) -> Result<CatalogUpdate> {
    let hint = receipt.catalog_hint()?.ok_or_else(|| Error::ConflictError("商品回调已完成".into()))?;
    if hint.topic == "region" {
        let areas = connector
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
    query_offer(connection, connector, receipt, sku, registry).await
}

async fn query_offer(
    connection: &SupplierApiConnection,
    connector: &dyn SupplierConnector,
    receipt: &SupplierCallbackReceipt,
    sku: SupplierSku,
    registry: &SupplierConnectorRegistry,
) -> Result<CatalogUpdate> {
    let hint = receipt.catalog_hint()?.ok_or_else(|| Error::ConflictError("商品回调已完成".into()))?;
    let source =
        connector.offers().ok_or_else(|| Error::BusinessLogicError("来源尚不支持商品补查".into()))?;
    let Lookup::Found(snapshot) =
        source.offer(&sku).await.map_err(|error| Error::BusinessLogicError(error.summary))?
    else {
        return Err(Error::BusinessLogicError("商品或规格暂不可见，请核实原来源；不按缺项下架".into()));
    };
    if snapshot.value.sku != sku || snapshot.stamp.observed_at < receipt.received_at {
        return Err(Error::ConflictError("商品补查身份或观察时间不一致".into()));
    }
    let mut item = SupplierCatalogItem::observed(
        &connection.base.id,
        connection.supplier_id.as_ref(),
        &receipt.base.id,
        snapshot,
    )?;
    if (hint.topic == "availability" && item.availability == CatalogAvailability::Unknown)
        || (hint.topic == "price" && item.purchase_price.is_none())
    {
        return Err(Error::BusinessLogicError("商品补查未取得明确可售状态或采购报价".into()));
    }
    item.availability_valid_until = registry
        .catalog_availability_validity(connection)
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
    use async_trait::async_trait;
    use erp_core::AccountKind;
    use erp_core::ids::{SupplierAccountId, SupplierApiConnectionId};
    use erp_integration::entity::supplier_callback::SupplierRefreshHint;
    use erp_supply::entity::supplier_api::{
        SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierConnectorType,
    };
    use erp_supply::ports::connector::SupplierConnector;
    use erp_supply::ports::connector::common::{
        ConnectorResult, Page, Scan, Snapshot, SourceRevision, SourceStamp,
    };
    use erp_supply::ports::connector::offer::{
        IncrementalSupport, Offer, OfferChange, OfferDetails, OfferSource, PriceTier, RegionCoverage,
        ServiceAreas, SupplyQuote,
    };

    use super::*;
    struct Source {
        id: SupplierApiConnectionId,
        offer: Option<Snapshot<Offer>>,
        regions: RegionCoverage,
    }
    impl SupplierConnector for Source {
        fn connection_id(&self) -> &SupplierApiConnectionId {
            &self.id
        }
        fn offers(&self) -> Option<&dyn OfferSource> {
            Some(self)
        }
        fn service_areas(&self) -> Option<&dyn ServiceAreas> {
            Some(self)
        }
    }
    #[async_trait]
    impl OfferSource for Source {
        fn incremental_support(&self) -> IncrementalSupport {
            IncrementalSupport::Unsupported
        }
        async fn scan(&self, _: &Scan) -> ConnectorResult<Page<Snapshot<OfferChange>>> {
            unreachable!("定向补查不能扫描目录")
        }
        async fn offer(&self, _: &SupplierSku) -> ConnectorResult<Lookup<Snapshot<Offer>>> {
            Ok(self.offer.clone().map_or(Lookup::NotVisible, Lookup::Found))
        }
    }
    #[async_trait]
    impl ServiceAreas for Source {
        async fn regions(&self, _: &SupplierSku) -> ConnectorResult<Snapshot<RegionCoverage>> {
            Ok(Snapshot { stamp: stamp(), value: self.regions.clone() })
        }
    }
    fn stamp() -> SourceStamp {
        SourceStamp { revision: SourceRevision::Unversioned, changed_at: None, observed_at: Instant::now() }
    }
    fn offer() -> Snapshot<Offer> {
        Snapshot {
            stamp: stamp(),
            value: Offer {
                sku: sku(),
                name: "查询得到的新名称".into(),
                specification: "6寸".into(),
                unit: Some("个".into()),
                quote: Some(SupplyQuote {
                    dropship: Some(PriceTier {
                        unit_price: "123.45".parse().unwrap(),
                        minimum_quantity: None,
                    }),
                    bulk: None,
                    tax_rate: None,
                    valid_until: None,
                }),
                details: OfferDetails { availability: CatalogAvailability::Available, ..Default::default() },
            },
        }
    }
    fn context(offer: Option<Snapshot<Offer>>, regions: RegionCoverage) -> Context {
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
            connector: Arc::new(Source { id: SupplierApiConnectionId::new("connection-1"), offer, regions }),
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

    #[tokio::test]
    async fn catalog_callbacks_query_bound_source_for_attributes_price_and_availability() {
        let registry = SupplierConnectorRegistry::default();
        for route in ["product", "price", "status"] {
            let mut response = offer();
            if route == "status" {
                response.value.details.availability = CatalogAvailability::Unavailable;
            }
            let context = context(Some(response), RegionCoverage::Unreported);
            let CatalogUpdate::Offer(item) =
                query(&context, &receipt(route), sku(), &registry).await.unwrap()
            else {
                panic!("商品补查")
            };
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
    async fn generic_catalog_topics_keep_price_and_availability_validation() {
        let registry = SupplierConnectorRegistry::default();
        for route in ["price", "status"] {
            let mut receipt = receipt(route);
            receipt.route_kind = "catalog".into();
            query(&context(Some(offer()), RegionCoverage::Unreported), &receipt, sku(), &registry)
                .await
                .unwrap();
            let mut missing = offer();
            if route == "price" {
                missing.value.quote = None;
            } else {
                missing.value.details.availability = CatalogAvailability::Unknown;
            }
            assert!(
                query(&context(Some(missing), RegionCoverage::Unreported), &receipt, sku(), &registry)
                    .await
                    .is_err()
            );
        }
    }
    #[tokio::test]
    async fn catalog_callbacks_reject_incomplete_price_status_or_wrong_identity() {
        let registry = SupplierConnectorRegistry::default();
        for route in ["price", "status"] {
            let mut response = offer();
            if route == "price" {
                response.value.quote = None;
            } else {
                response.value.details.availability = CatalogAvailability::Unknown;
            }
            assert!(
                query(
                    &context(Some(response), RegionCoverage::Unreported),
                    &receipt(route),
                    sku(),
                    &registry
                )
                .await
                .is_err()
            );
        }
        let mut wrong = offer();
        wrong.value.sku.product_id = Some("another-product".into());
        assert!(
            query(&context(Some(wrong), RegionCoverage::Unreported), &receipt("product"), sku(), &registry)
                .await
                .is_err()
        );
        let mut future = receipt("price");
        future.received_at = Instant::from_unix_secs(Instant::now().unix_secs() + 100);
        assert!(
            query(&context(Some(offer()), RegionCoverage::Unreported), &future, sku(), &registry)
                .await
                .is_err()
        );
        assert!(
            query(&context(None, RegionCoverage::Unreported), &receipt("product"), sku(), &registry)
                .await
                .is_err()
        );
    }
    #[tokio::test]
    async fn catalog_city_callback_uses_full_query_preserves_empty_and_rejects_partial_mapping() {
        let registry = SupplierConnectorRegistry::default();
        for expected in [vec![], vec!["110100".to_string()]] {
            let context = context(None, RegionCoverage::Included(expected.clone()));
            let CatalogUpdate::Regions { id, regions } =
                query(&context, &receipt("cities"), sku(), &registry).await.unwrap()
            else {
                panic!("地区补查")
            };
            assert_eq!(id, SupplierCatalogItem::source_id("connection-1", "product-1", "spec-1"));
            assert_eq!(regions.included, expected);
        }
        assert!(
            query(&context(None, RegionCoverage::Unreported), &receipt("cities"), sku(), &registry)
                .await
                .is_err()
        );
    }
}
