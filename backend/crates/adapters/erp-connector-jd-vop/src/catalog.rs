//! 授权商品池扫描；失败页不返回新检查点，不由池缺项推导商品撤回。
use std::collections::BTreeSet;

use async_trait::async_trait;
use erp_core::common::time::Instant;
use erp_supply::catalog_membership::{CatalogMembershipPage, CatalogPoolScope};
use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{
    ConnectorResult, Lookup, Page, Scan, ScanWindow, Snapshot, SourceRevision, SourceStamp, SupplierSku,
};
use erp_supply::ports::connector::offer::{
    IncrementalSupport, Offer, OfferChange, OfferSource, ScopedOfferPage,
};
use erp_supply::ports::connector::transform::OfferTransform;
use hmac::{Hmac, KeyInit, Mac};
use serde::{Deserialize, Serialize};
use sha2::Sha256;

use super::transport::JdVopReadRequest;
use super::wire::{Category, PoolPage, Pools, SellPrice, SkuDetail, SkuImages};
use super::{JdVopConnector, JdVopOffer, error, mapping, protocol_required};

type HmacSha256 = Hmac<Sha256>;

#[derive(Clone, Serialize, Deserialize)]
struct Cursor {
    binding: String,
    pools: Vec<String>,
    pool: usize,
    offset: String,
}

#[async_trait]
impl OfferSource for JdVopConnector {
    async fn scope(&self) -> ConnectorResult<Option<CatalogPoolScope>> {
        self.pool_scope().await.map(Some)
    }
    fn incremental_support(&self) -> IncrementalSupport {
        IncrementalSupport::Unsupported
    }

    async fn scan(&self, scan: &Scan) -> ConnectorResult<Page<Snapshot<OfferChange>>> {
        Ok(self.scan_scoped(scan).await?.page)
    }

    async fn scan_scoped(&self, scan: &Scan) -> ConnectorResult<ScopedOfferPage> {
        validate_scan(scan)?;
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let mut cursor = self.cursor(scan.cursor.as_deref()).await?;
        if cursor.pools.is_empty() {
            let membership = self.membership_proof(&cursor, None, true, true, vec![]).await?;
            return Ok(ScopedOfferPage {
                page: Page { items: vec![], next: None },
                skipped: vec![],
                membership: Some(membership),
            });
        }
        let page: PoolPage = reader
            .read(JdVopReadRequest::PoolPage {
                pool_id: cursor.pools[cursor.pool].clone(),
                offset: cursor.offset.clone(),
                page_size: scan.limit,
            })
            .await?;
        if page.skus.len() > usize::try_from(scan.limit.get()).expect("页大小不超过100") {
            return Err(mapping("京东目录返回数量超过请求上限"));
        }
        let ids = page.skus.into_iter().map(|id| id.0).collect::<BTreeSet<_>>();
        let members = ids.iter().map(|id| SupplierSku { product_id: None, spec_id: id.clone() }).collect();
        let (items, skipped) = self.scan_offers(ids).await?;
        let previous = cursor.clone();
        let has_next = cursor.advance(&page.offset.0, &page.remain_page.0)?;
        let pool_finished = page.remain_page.0 == "0";
        let membership = self
            .membership_proof(
                &previous,
                (!pool_finished).then_some(page.offset.0),
                pool_finished,
                !has_next,
                members,
            )
            .await?;
        let next = has_next.then(|| self.encode_cursor(&cursor)).transpose()?;
        Ok(ScopedOfferPage { page: Page { items, next }, skipped, membership: Some(membership) })
    }

    async fn offer(&self, sku: &SupplierSku) -> ConnectorResult<Lookup<Snapshot<Offer>>> {
        if sku.spec_id.is_empty() || !sku.spec_id.bytes().all(|byte| byte.is_ascii_digit()) {
            return Err(mapping("京东订货编号格式无效"));
        }
        let offer = self.read_offer(&sku.spec_id).await?;
        if sku.product_id.is_some() && sku.product_id != offer.sku.product_id {
            return Err(error(
                SupplierFailureClass::MappingError,
                "JD_VOP_SOURCE_GROUP_CHANGED",
                "京东商品分组已变化，请先核验来源身份",
            ));
        }
        Ok(Lookup::Found(Snapshot { stamp: stamp(), value: offer }))
    }
}

impl JdVopConnector {
    async fn scan_offers(
        &self,
        ids: BTreeSet<String>,
    ) -> ConnectorResult<(Vec<Snapshot<OfferChange>>, Vec<String>)> {
        let mut items = Vec::new();
        let mut skipped = Vec::new();
        for id in ids {
            match self.read_offer(&id).await {
                Ok(offer) => {
                    items.push(Snapshot { stamp: stamp(), value: OfferChange::Upsert(Box::new(offer)) });
                },
                Err(error) if error.class == SupplierFailureClass::MappingError => {
                    skipped.push(format!("京东 SKU {id}：{}：{}", error.code, error.summary));
                },
                Err(error) => return Err(error),
            }
        }
        Ok((items, skipped))
    }

    async fn cursor(&self, encoded: Option<&str>) -> ConnectorResult<Cursor> {
        if let Some(encoded) = encoded {
            return self.decode_cursor(encoded);
        }
        let pools = self.pool_scope().await?.pool_ids;
        Ok(Cursor { binding: self.binding.clone(), pools, pool: 0, offset: "0".into() })
    }

    async fn pool_scope(&self) -> ConnectorResult<CatalogPoolScope> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let pools: Pools = reader.read(JdVopReadRequest::Pools).await?;
        let pools = pools.sku_pool_list.into_iter().map(|pool| pool.biz_pool_id.0).collect::<Vec<_>>();
        if pools.iter().any(|id| id == "0") {
            return Err(mapping("京东授权商品池编号无效"));
        }
        CatalogPoolScope::new(self.account_scope.clone(), pools)
            .map_err(|_| mapping("京东授权商品池范围无效或过大"))
    }

    async fn membership_proof(
        &self,
        cursor: &Cursor,
        next_offset: Option<String>,
        pool_finished: bool,
        scan_finished: bool,
        members: Vec<SupplierSku>,
    ) -> ConnectorResult<CatalogMembershipPage> {
        let scope = CatalogPoolScope::new(self.account_scope.clone(), cursor.pools.clone())
            .map_err(|_| mapping("京东授权商品池范围无效"))?;
        let confirmed_scope = if scan_finished { Some(self.pool_scope().await?) } else { None };
        let proof = CatalogMembershipPage {
            scope,
            pool_id: cursor.pools.get(cursor.pool).cloned(),
            offset: cursor.offset.clone(),
            next_offset,
            pool_finished,
            scan_finished,
            confirmed_scope,
            members,
        };
        proof.validate().map_err(|_| mapping("京东商品池页面范围证据无效"))?;
        Ok(proof)
    }

    async fn read_offer(&self, id: &str) -> ConnectorResult<Offer> {
        let reader = self.reader.as_ref().ok_or_else(protocol_required)?;
        let detail: SkuDetail =
            reader.read(JdVopReadRequest::Detail { sku_id: id.into(), query_ext_set: vec![13] }).await?;
        if detail.sku_id.0 != id {
            return Err(mapping("京东商品详情与请求编号不一致"));
        }
        let price: Vec<SellPrice> =
            reader.read(JdVopReadRequest::Price { sku_id: id.into(), quantity: "1".into() }).await?;
        let images: Vec<SkuImages> = reader.read(JdVopReadRequest::Images { sku_id: id.into() }).await?;
        if price.len() > 1 || images.len() > 1 {
            return Err(mapping("京东单项读取返回重复商品"));
        }
        let category_ids = detail
            .category
            .as_deref()
            .unwrap_or_default()
            .split(';')
            .filter(|id| !id.is_empty())
            .map(str::to_owned)
            .collect::<Vec<_>>();
        if category_ids.len() > 3 {
            return Err(mapping("京东商品类目层级超出合同范围"));
        }
        let categories: Vec<Category> = if category_ids.is_empty() {
            vec![]
        } else {
            reader.read(JdVopReadRequest::Categories { category_ids }).await?
        };
        self.mapper.transform(JdVopOffer {
            detail: &detail,
            price: price.first(),
            images: images.first(),
            categories: &categories,
            query_quantity: "1".parse().expect("固定目录观察数量1"),
            observed_at: Instant::now(),
        })
    }

    fn encode_cursor(&self, cursor: &Cursor) -> ConnectorResult<String> {
        let body = serde_json::to_vec(cursor).map_err(|_| mapping("京东目录检查点编码失败"))?;
        let mut mac = cursor_mac(&self.cursor_secret);
        mac.update(&body);
        Ok(format!("{}.{}", hex::encode(&body), hex::encode(mac.finalize().into_bytes())))
    }

    fn decode_cursor(&self, encoded: &str) -> ConnectorResult<Cursor> {
        if encoded.len() > 131_072 {
            return Err(mapping("京东目录检查点过大"));
        }
        let (body, signature) = encoded.split_once('.').ok_or_else(|| mapping("京东目录检查点无效"))?;
        let body = hex::decode(body).map_err(|_| mapping("京东目录检查点无效"))?;
        let signature = hex::decode(signature).map_err(|_| mapping("京东目录检查点无效"))?;
        let mut mac = cursor_mac(&self.cursor_secret);
        mac.update(&body);
        mac.verify_slice(&signature).map_err(|_| mapping("京东目录检查点已失效"))?;
        let cursor: Cursor = serde_json::from_slice(&body).map_err(|_| mapping("京东目录检查点格式无效"))?;
        if cursor.binding != self.binding
            || cursor.pool >= cursor.pools.len()
            || cursor.pools.len() > 1000
            || cursor.pools.iter().any(|id| id.is_empty() || !id.bytes().all(|byte| byte.is_ascii_digit()))
            || cursor.offset.is_empty()
            || !cursor.offset.bytes().all(|byte| byte.is_ascii_digit())
        {
            return Err(mapping("京东目录检查点与当前连接不符"));
        }
        Ok(cursor)
    }
}

impl Cursor {
    fn advance(&mut self, offset: &str, remaining: &str) -> ConnectorResult<bool> {
        if remaining != "0" {
            if offset == self.offset {
                return Err(mapping("京东目录偏移量未推进"));
            }
            self.offset = offset.into();
            return Ok(true);
        }
        self.pool += 1;
        self.offset = "0".into();
        Ok(self.pool < self.pools.len())
    }
}

fn cursor_mac(secret: &str) -> HmacSha256 {
    let mut mac = HmacSha256::new_from_slice(secret.as_bytes()).expect("HMAC接受任意长度密钥");
    mac.update(b"erp-jd-vop-catalog-cursor-v1\0");
    mac
}

fn stamp() -> SourceStamp {
    SourceStamp { revision: SourceRevision::Unversioned, changed_at: None, observed_at: Instant::now() }
}

fn validate_scan(scan: &Scan) -> ConnectorResult<()> {
    if !matches!(scan.window, ScanWindow::Full) {
        return Err(error(
            SupplierFailureClass::CapabilityGap,
            "JD_VOP_INCREMENTAL_UNSUPPORTED",
            "京东目录变更须通过消息定向补查或完整校对",
        ));
    }
    if scan.limit.get() > 100 {
        return Err(mapping("京东目录页大小不能超过100"));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::collections::VecDeque;
    use std::num::NonZeroU32;
    use std::sync::{Arc, Mutex};

    use serde_json::{Value, json};

    use super::super::JdVopOfferMapper;
    use super::super::transport::{JdVopReadTransport, ReadClient};
    use super::*;

    #[derive(Default)]
    struct FakeTransport {
        responses: Mutex<VecDeque<(JdVopReadRequest, ConnectorResult<Vec<u8>>)>>,
        requests: Mutex<Vec<JdVopReadRequest>>,
    }

    #[async_trait]
    impl JdVopReadTransport for FakeTransport {
        async fn read(&self, request: &JdVopReadRequest) -> ConnectorResult<Vec<u8>> {
            self.requests.lock().unwrap().push(request.clone());
            let (expected, result) = self.responses.lock().unwrap().pop_front().expect("expected request");
            assert_eq!(&expected, request);
            result
        }
    }

    fn response(request: JdVopReadRequest, result: Value) -> (JdVopReadRequest, ConnectorResult<Vec<u8>>) {
        let key = format!("{}_responce", request.method().replace('.', "_"));
        (
            request,
            Ok(serde_json::to_vec(
                &json!({key:{"openRpcResult":{"success":true,"resultCode":"0000","result":result}}}),
            )
            .unwrap()),
        )
    }

    fn connector(
        responses: Vec<(JdVopReadRequest, ConnectorResult<Vec<u8>>)>,
    ) -> (JdVopConnector, Arc<FakeTransport>) {
        let transport =
            Arc::new(FakeTransport { responses: Mutex::new(responses.into()), ..FakeTransport::default() });
        let reader = Arc::new(ReadClient::new(transport.clone(), 2, 100, 5));
        (
            JdVopConnector {
                connection_id: erp_core::ids::SupplierApiConnectionId::new("c1"),
                account_scope: "account-test".into(),
                binding: "binding-1".into(),
                cursor_secret: "secret".into(),
                reader: Some(reader),
                mapper: JdVopOfferMapper::default(),
                writer: None,
                profile: None,
                evidence_codec: None,
                message_runtime: None,
            },
            transport,
        )
    }

    fn scan(cursor: Option<String>) -> Scan {
        Scan { window: ScanWindow::Full, cursor, limit: NonZeroU32::new(2).unwrap() }
    }

    #[tokio::test]
    async fn multi_pool_scan_preserves_offsets_and_empty_page_checkpoint() {
        let first = JdVopReadRequest::PoolPage {
            pool_id: "1".into(),
            offset: "0".into(),
            page_size: NonZeroU32::new(2).unwrap(),
        };
        let second = JdVopReadRequest::PoolPage {
            pool_id: "1".into(),
            offset: "9007199254740993".into(),
            page_size: NonZeroU32::new(2).unwrap(),
        };
        let third = JdVopReadRequest::PoolPage {
            pool_id: "2".into(),
            offset: "0".into(),
            page_size: NonZeroU32::new(2).unwrap(),
        };
        let (connector, transport) = connector(vec![
            response(
                JdVopReadRequest::Pools,
                json!({"skuPoolList":[{"bizPoolId":"2","name":"two"},{"bizPoolId":"1","name":"one"},{"bizPoolId":"1","name":"duplicate"}]}),
            ),
            response(first, json!({"skus":[],"offset":"9007199254740993","remainPage":"1"})),
            response(second, json!({"skus":[],"offset":"9007199254740993","remainPage":"0"})),
            response(third, json!({"skus":[],"offset":"0","remainPage":"0"})),
            response(
                JdVopReadRequest::Pools,
                json!({"skuPoolList":[{"bizPoolId":"1","name":"one"},{"bizPoolId":"2","name":"two"}]}),
            ),
        ]);
        let first = connector.scan(&scan(None)).await.unwrap();
        assert!(first.items.is_empty());
        assert!(first.next.is_some());
        let second = connector.scan(&scan(first.next)).await.unwrap();
        assert!(second.next.is_some());
        assert!(connector.scan(&scan(second.next)).await.unwrap().next.is_none());
        assert_eq!(transport.requests.lock().unwrap().len(), 5);
    }

    #[tokio::test]
    async fn final_scope_change_invalidates_all_absence_proof_but_keeps_the_page_observation() {
        use erp_supply::catalog_membership::{CatalogMembershipScan, CatalogMembershipState};
        let (connector, transport) = connector(vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            response(
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                json!({"skus":[],"offset":"0","remainPage":"0"}),
            ),
            response(
                JdVopReadRequest::Pools,
                json!({"skuPoolList":[{"bizPoolId":"1","name":"one"},{"bizPoolId":"2","name":"new authorization"}]}),
            ),
        ]);
        let page = connector.scan_scoped(&scan(None)).await.unwrap();
        assert!(page.page.next.is_none());
        let proof = page.membership.unwrap();
        let mut checkpoint =
            CatalogMembershipScan::new("job", "c1", "supplier", proof.scope.clone()).unwrap();
        checkpoint.observe_page(&proof, Instant::from_unix_secs(1)).unwrap();
        checkpoint.authorize_reconciliation(None);
        assert_eq!(checkpoint.state, CatalogMembershipState::Uncertain);
        assert!(!checkpoint.can_reconcile());
        assert_eq!(transport.requests.lock().unwrap().len(), 3);
    }

    #[tokio::test]
    async fn no_quote_still_preserves_the_raw_pool_member_and_completion_recheck() {
        let (connector, transport) = connector(vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            response(
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                json!({"skus":["000123"],"offset":"123","remainPage":"0"}),
            ),
            response(
                JdVopReadRequest::Detail { sku_id: "000123".into(), query_ext_set: vec![13] },
                json!({"skuId":"000123","skuName":"No quote"}),
            ),
            response(JdVopReadRequest::Price { sku_id: "000123".into(), quantity: "1".into() }, json!([])),
            response(JdVopReadRequest::Images { sku_id: "000123".into() }, json!([])),
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
        ]);
        let page = connector.scan_scoped(&scan(None)).await.unwrap();
        let proof = page.membership.unwrap();
        assert_eq!(proof.members, vec![SupplierSku { product_id: None, spec_id: "000123".into() }]);
        assert_eq!(proof.confirmed_scope.as_ref(), Some(&proof.scope));
        let OfferChange::Upsert(offer) = &page.page.items[0].value else { panic!("source observation") };
        assert!(offer.quote.is_none());
        assert_eq!(transport.requests.lock().unwrap().len(), 6);
    }

    #[tokio::test]
    async fn empty_nonterminal_page_must_advance_the_provider_offset() {
        let (connector, transport) = connector(vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            response(
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                json!({"skus":[],"offset":"0","remainPage":"1"}),
            ),
        ]);
        assert!(connector.scan_scoped(&scan(None)).await.is_err());
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn failed_page_stops_without_advancing_and_restart_uses_original_cursor() {
        let (connector, transport) = connector(vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            (
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                Err(mapping("failed page")),
            ),
        ]);
        assert!(connector.scan(&scan(None)).await.is_err());
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
        let cursor = Cursor {
            binding: connector.binding.clone(),
            pools: vec!["1".into()],
            pool: 0,
            offset: "123".into(),
        };
        let token = connector.encode_cursor(&cursor).unwrap();
        assert_eq!(connector.decode_cursor(&token).unwrap().offset, "123");
        let (other, _) = super::tests::connector(vec![]);
        let mut other = other;
        other.binding = "other-account".into();
        assert!(other.decode_cursor(&token).is_err());
        assert!(connector.decode_cursor(&(token + "00")).is_err());
    }

    #[tokio::test]
    async fn real_read_pipeline_maps_detail_price_and_images_once_per_duplicate_sku() {
        let (connector, transport) = connector(vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            response(
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                json!({"skus":["123","123"],"offset":"123","remainPage":"0"}),
            ),
            response(
                JdVopReadRequest::Detail { sku_id: "123".into(), query_ext_set: vec![13] },
                json!({"skuId":"123","skuName":"Paper","spuId":"0","lowestBuy":"2","saleUnit":"pack"}),
            ),
            response(
                JdVopReadRequest::Price { sku_id: "123".into(), quantity: "1".into() },
                json!([{"skuId":"123","salePrice":"0","taxRatePercentage":"9"}]),
            ),
            response(
                JdVopReadRequest::Images { sku_id: "123".into() },
                json!([{"skuId":"123","skuImageList":[]}]),
            ),
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
        ]);
        let result = connector.scan(&scan(None)).await.unwrap();
        assert_eq!(result.items.len(), 1);
        let OfferChange::Upsert(offer) = &result.items[0].value else { panic!("no fabricated withdrawals") };
        assert_eq!(offer.sku.product_id.as_deref(), Some("sku:123"));
        assert_eq!(offer.quote.as_ref().unwrap().dropship.as_ref().unwrap().unit_price.to_string(), "0");
        assert!(result.next.is_none());
        assert_eq!(transport.requests.lock().unwrap().len(), 6);
    }

    fn item_responses(id: &str, detail: Value) -> Vec<(JdVopReadRequest, ConnectorResult<Vec<u8>>)> {
        vec![
            response(JdVopReadRequest::Detail { sku_id: id.into(), query_ext_set: vec![13] }, detail),
            response(JdVopReadRequest::Price { sku_id: id.into(), quantity: "1".into() }, json!([])),
            response(JdVopReadRequest::Images { sku_id: id.into() }, json!([])),
        ]
    }

    #[tokio::test]
    async fn malformed_item_is_isolated_while_raw_members_and_next_page_are_preserved() {
        let mut responses = vec![
            response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
            response(
                JdVopReadRequest::PoolPage {
                    pool_id: "1".into(),
                    offset: "0".into(),
                    page_size: NonZeroU32::new(2).unwrap(),
                },
                json!({"skus":["123","124"],"offset":"124","remainPage":"1"}),
            ),
        ];
        responses.extend(item_responses("123", json!({"skuId":"123","skuName":"bad","spuId":"10"})));
        responses.extend(item_responses("124", json!({"skuId":"124","skuName":"good"})));
        let (connector, transport) = connector(responses);
        let page = connector.scan_scoped(&scan(None)).await.unwrap();
        assert_eq!(page.page.items.len(), 1);
        let OfferChange::Upsert(offer) = &page.page.items[0].value else { panic!("expected valid offer") };
        assert_eq!(offer.sku.spec_id, "124");
        assert_eq!(page.skipped.len(), 1);
        assert!(page.skipped[0].contains("京东 SKU 123：JD_VOP_MAPPING_INVALID"));
        assert!(page.skipped[0].contains("SPU缺少共有名称"));
        assert_eq!(page.membership.unwrap().members.len(), 2);
        assert_eq!(connector.decode_cursor(page.page.next.as_deref().unwrap()).unwrap().offset, "124");
        assert_eq!(transport.requests.lock().unwrap().len(), 8);
    }

    #[tokio::test]
    async fn item_auth_rate_or_network_failure_never_returns_an_advanced_page() {
        for class in [
            SupplierFailureClass::AuthSignature,
            SupplierFailureClass::RateLimited,
            SupplierFailureClass::TransientFailure,
        ] {
            let (connector, transport) = connector(vec![
                response(JdVopReadRequest::Pools, json!({"skuPoolList":[{"bizPoolId":"1","name":"one"}]})),
                response(
                    JdVopReadRequest::PoolPage {
                        pool_id: "1".into(),
                        offset: "0".into(),
                        page_size: NonZeroU32::new(2).unwrap(),
                    },
                    json!({"skus":["123","124"],"offset":"124","remainPage":"1"}),
                ),
                (
                    JdVopReadRequest::Detail { sku_id: "123".into(), query_ext_set: vec![13] },
                    Err(error(class, "JD_TEST_READ_FAILED", "读取失败")),
                ),
            ]);
            let error = connector.scan_scoped(&scan(None)).await.unwrap_err();
            assert_eq!(error.class, class);
            assert_eq!(transport.requests.lock().unwrap().len(), 3);
        }
    }
}
