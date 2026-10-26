//! 目录采集隔离单项映射错误；整页协议和传输失败仍交由任务恢复。
use std::collections::BTreeMap;

use erp_supply::entity::failure::SupplierFailureClass;
use erp_supply::ports::connector::common::{ConnectorResult, Scan, Snapshot};
use erp_supply::ports::connector::offer::{Offer, OfferChange};
use erp_supply::ports::supplier_api_gateway::CatalogPage;
use serde_json::Value;

use super::DangaoshushuConnector;
use super::catalog::{scan_parameters, spec_value};
use super::parsing::{mapping, snapshot, source_time};
use super::transport::Request;

impl DangaoshushuConnector {
    /// 按商品补查详情，记录单项映射异常并继续读取有效规格。
    /// # 参数
    /// `scan` 为冻结窗口、游标及页面大小。
    /// # 返回
    /// 有效来源、跳过原因及按原始商品数计算的下一页。
    /// # 错误
    /// 页面结构、网络或鉴权等非单项映射失败时返回错误，禁止推进游标。
    pub(super) async fn scan_with_details(&self, scan: &Scan) -> ConnectorResult<CatalogPage> {
        let (number, parameters) = scan_parameters(scan)?;
        let value = self
            .transport
            .send(Request::Read { path: "/dsapi/product/get_product_hot_lists", parameters })
            .await?;
        let products = value["product_list"].as_array().ok_or_else(mapping)?;
        let next = if products.len() >= usize::try_from(scan.limit.get()).map_err(|_| mapping())? {
            Some(number.checked_add(1).ok_or_else(mapping)?.to_string())
        } else {
            None
        };
        let mut page = CatalogPage { items: vec![], skipped: vec![], next };
        let mut details = BTreeMap::new();
        for (index, product) in products.iter().enumerate() {
            let position = format!("第 {number} 页第 {} 个商品", index + 1);
            self.scan_product(product, &position, &mut page, &mut details).await?;
        }
        Ok(page)
    }

    async fn scan_product(
        &self,
        product: &Value,
        position: &str,
        page: &mut CatalogPage,
        details: &mut BTreeMap<String, ConnectorResult<Value>>,
    ) -> ConnectorResult<()> {
        let Some(specs) = product["specs"].as_array().filter(|specs| !specs.is_empty()) else {
            page.skipped.push(format!("{position}：DGSS_MAPPING：商品规格列表缺失或为空"));
            return Ok(());
        };
        for (index, spec) in specs.iter().enumerate() {
            let position = format!("{position}第 {} 个规格", index + 1);
            let Some(offer) = collect_item(self.offer_value(product, spec, false), &position, page)? else {
                continue;
            };
            // 同一商品详情只请求一次，映射失败也复用结果；网络失败不缓存为跳过。
            let product_id = offer.sku.product_id.as_deref().ok_or_else(mapping)?;
            if !details.contains_key(product_id) {
                details.insert(product_id.to_owned(), self.product(&offer.sku, None).await);
            }
            let root = details.get(product_id).expect("当前商品已补查详情");
            let result =
                root.as_ref().map_err(Clone::clone).and_then(|root| self.detail_observation(offer, root));
            if let Some(item) = collect_item(result, &position, page)? {
                page.items.push(item);
            }
        }
        Ok(())
    }

    fn detail_observation(&self, offer: Offer, root: &Value) -> ConnectorResult<Snapshot<OfferChange>> {
        let spec = spec_value(root, &offer.sku)?.ok_or_else(mapping)?;
        let mut offer = self.offer_value(&root["product"], spec, true)?;
        offer.details = super::catalog_details::details(&root["product"], spec, Some(root))?;
        Ok(snapshot(OfferChange::Upsert(Box::new(offer)), source_time(&spec["updated_at"])?))
    }
}

/// 只隔离有明确商品位置的映射错误，禁止把传输失败转换成跳过。
fn collect_item<T>(
    result: ConnectorResult<T>,
    position: &str,
    page: &mut CatalogPage,
) -> ConnectorResult<Option<T>> {
    match result {
        Ok(item) => Ok(Some(item)),
        Err(error) if error.class == SupplierFailureClass::MappingError => {
            page.skipped.push(format!("{position}：{}：{}", error.code, error.summary));
            Ok(None)
        },
        Err(error) => Err(error),
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use erp_supply::ports::connector::common::ScanWindow;
    use serde_json::json;

    use super::super::error;
    use super::super::test_support::recording_connector;
    use super::*;

    fn scan(limit: u32) -> Scan {
        Scan { window: ScanWindow::Full, cursor: Some("135".into()), limit: NonZeroU32::new(limit).unwrap() }
    }

    fn listed() -> Value {
        json!({"id":"p1","title":"商品","specs":[{"spec_id":"s1","name":"规格"}]})
    }

    #[tokio::test]
    async fn skips_bad_specs_and_products_without_losing_valid_siblings_or_pagination() {
        let (connector, transport) = recording_connector(vec![
            Ok(json!({"product_list":[
                {"id":"bad","title":"  ","specs":[{"spec_id":"bad","name":"规格"}]},
                {"id":"p1","title":"商品","specs":[
                    {"spec_id":"bad","name":" "}, {"spec_id":"s1","name":"规格"},
                    {"spec_id":"s2","name":"规格2"}
                ]},
                {"id":"empty","title":"无规格","specs":[]},
                listed()
            ]})),
            Ok(json!({"product":{"id":"p1","title":"商品"},"specs":[
                {"id":"s1","name":"规格"},{"id":"s2","name":"规格2"}
            ]})),
        ]);
        let page = connector.scan_with_details(&scan(4)).await.unwrap();
        assert_eq!(page.items.len(), 3);
        assert_eq!(page.skipped.len(), 3);
        assert!(page.skipped[0].contains("第 135 页第 1 个商品第 1 个规格"));
        assert!(page.skipped[1].contains("第 135 页第 2 个商品第 1 个规格"));
        assert_eq!(page.next.as_deref(), Some("136"));
        assert_eq!(transport.requests.lock().unwrap().len(), 2);
    }

    #[tokio::test]
    async fn detail_mapping_failure_skips_only_affected_spec_and_missing_spec_is_not_imported() {
        let (connector, _) = recording_connector(vec![
            Ok(json!({"product_list":[{"id":"p1","title":"商品","specs":[
                {"spec_id":"s1","name":"规格"},{"spec_id":"s2","name":"规格2"},
                {"spec_id":"missing","name":"不存在"}
            ]}]})),
            Ok(json!({"product":{"id":"p1","title":"商品"},"specs":[
                {"id":"s1","name":"规格","clearing_price":"invalid"},
                {"id":"s2","name":"规格2"}
            ]})),
        ]);
        let page = connector.scan_with_details(&scan(5)).await.unwrap();
        assert_eq!(page.skipped.len(), 2);
        assert_eq!(page.items.len(), 1);
        let OfferChange::Upsert(offer) = &page.items[0].value else { panic!("expected offer") };
        assert_eq!(offer.sku.spec_id, "s2");
        assert!(page.next.is_none());
    }

    #[tokio::test]
    async fn all_invalid_page_still_advances_but_malformed_page_and_transport_failures_stop() {
        let (connector, _) = recording_connector(vec![Ok(json!({"product_list":[{"specs":[]}]}))]);
        let page = connector.scan_with_details(&scan(1)).await.unwrap();
        assert!(page.items.is_empty());
        assert_eq!(page.skipped.len(), 1);
        assert_eq!(page.next.as_deref(), Some("136"));
        let (connector, _) = recording_connector(vec![Ok(json!({"unexpected":[]}))]);
        assert!(connector.scan_with_details(&scan(1)).await.is_err());
        for class in [
            SupplierFailureClass::AuthSignature,
            SupplierFailureClass::TransientFailure,
            SupplierFailureClass::RateLimited,
        ] {
            let failure = error(class, "TEST_FAILURE", "接口调用失败");
            let (connector, _) =
                recording_connector(vec![Ok(json!({"product_list":[listed()]})), Err(failure.clone())]);
            assert_eq!(connector.scan_with_details(&scan(5)).await.unwrap_err(), failure);
        }
    }
}
