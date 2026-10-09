use erp_supply::ports::connector::common::ConnectorResult;
use serde::Deserialize;
use serde_json::Value;

use super::DangaoshushuConnector;
use super::parsing::{mapping, valid_id};
use super::transport::Request;

#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum DangaoshushuReadKind {
    Brands,
    Catalog,
    Product,
    Cities,
    Shops,
    DeliveryMap,
}

/// 供应商只读协议查询；不接受任意 URL 或写动作。
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct DangaoshushuReadQuery {
    pub kind: DangaoshushuReadKind,
    pub product_id: Option<String>,
    pub brand_id: Option<String>,
    pub city_id: Option<String>,
    #[serde(default = "first_page")]
    pub page: u32,
    #[serde(default = "page_size")]
    pub size: u32,
}
fn first_page() -> u32 {
    1
}
fn page_size() -> u32 {
    20
}

impl DangaoshushuConnector {
    /// 读取新版目录、品牌、门店、可售城市或地图范围的原始来源证据。
    /// # 参数
    /// `query` 为已授权连接的固定只读查询。
    /// # 返回
    /// 原始供应商数据；不改公司商品或供给。
    /// # 错误
    /// 缺失必要标识、非法分页或外部调用失败时返回分类错误。
    pub async fn read(&self, query: &DangaoshushuReadQuery) -> ConnectorResult<Value> {
        let mut parameters = Vec::new();
        let path = match query.kind {
            DangaoshushuReadKind::Brands => return self.brands().await,
            DangaoshushuReadKind::Catalog => {
                if query.page == 0 || !(1..=50).contains(&query.size) {
                    return Err(mapping());
                }
                parameters.extend([
                    ("page".into(), query.page.to_string()),
                    ("size".into(), query.size.to_string()),
                    ("sort_price_type".into(), "1".into()),
                ]);
                if let Some(brand) = &query.brand_id {
                    valid_id(brand)?;
                    parameters.push(("brand_id".into(), brand.clone()));
                }
                "/dsapi/product/get_product_hot_lists"
            },
            DangaoshushuReadKind::Product | DangaoshushuReadKind::Cities => {
                let id = query.product_id.as_deref().ok_or_else(mapping)?;
                valid_id(id)?;
                parameters.push(("product_id".into(), id.into()));
                if matches!(query.kind, DangaoshushuReadKind::Product) {
                    "/dsapi/product/get_product_details"
                } else {
                    "/dsapi/product/get_product_cities_info"
                }
            },
            DangaoshushuReadKind::Shops => {
                return self
                    .shops(
                        query.brand_id.as_deref().ok_or_else(mapping)?,
                        query.city_id.as_deref().ok_or_else(mapping)?,
                    )
                    .await;
            },
            DangaoshushuReadKind::DeliveryMap => {
                let city = query.city_id.as_deref().ok_or_else(mapping)?;
                valid_id(city)?;
                let product = query.product_id.as_deref().ok_or_else(mapping)?;
                valid_id(product)?;
                parameters.extend([("city_id".into(), city.into()), ("product_id".into(), product.into())]);
                "/dsapi/city/get_rules"
            },
        };
        if !matches!(query.kind, DangaoshushuReadKind::DeliveryMap)
            && let Some(city) = &query.city_id
        {
            valid_id(city)?;
            parameters.push(("city_id".into(), city.clone()));
        }
        self.transport.send(Request::Read { path, parameters }).await
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::super::test_support::recording_connector;
    use super::*;

    #[tokio::test]
    async fn raw_catalog_reads_fixed_new_api_and_preserves_composite_filter() {
        let query: DangaoshushuReadQuery = serde_json::from_value(
            json!({"kind":"catalog","brand_id":"721-101902","city_id":"2","page":2,"size":50}),
        )
        .unwrap();
        let (connector, calls) = recording_connector(vec![Ok(json!({"product_list":[]}))]);
        assert_eq!(connector.read(&query).await.unwrap(), json!({"product_list":[]}));
        let requests = calls.requests.lock().unwrap();
        let Request::Read { path, parameters } = &requests[0] else { panic!("expected readonly") };
        assert_eq!(*path, "/dsapi/product/get_product_hot_lists");
        assert!(parameters.contains(&("brand_id".into(), "721-101902".into())));
        assert!(parameters.contains(&("sort_price_type".into(), "1".into())));
    }

    #[tokio::test]
    async fn unsupported_kind_fields_and_invalid_identifiers_make_no_request() {
        assert!(serde_json::from_value::<DangaoshushuReadQuery>(json!({"kind":"submit_order"})).is_err());
        assert!(
            serde_json::from_value::<DangaoshushuReadQuery>(json!({"kind":"catalog","url":"https://other"}))
                .is_err()
        );
        let (connector, calls) = recording_connector(vec![]);
        for input in [
            json!({"kind":"catalog","page":0}),
            json!({"kind":"catalog","size":51}),
            json!({"kind":"product"}),
            json!({"kind":"product","product_id":"1&write=true"}),
        ] {
            let query: DangaoshushuReadQuery = serde_json::from_value(input).unwrap();
            assert!(connector.read(&query).await.is_err());
        }
        assert!(calls.requests.lock().unwrap().is_empty());
    }
}
