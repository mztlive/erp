//! 使用供应商分类字典补齐来源分类名称，不创建公司分类。
use std::collections::BTreeMap;

use erp_supply::ports::connector::common::{ConnectorResult, Snapshot};
use erp_supply::ports::connector::offer::OfferChange;

use super::DangaoshushuConnector;
use super::parsing::{id, mapping};
use super::transport::Request;

impl DangaoshushuConnector {
    // 目录单项查询或整页采集需要展示来源分类时，按当前连接读取各层级编号和名称。
    // 远端失败、字典格式不符或分类编号重复均返回错误。
    pub(super) async fn category_names(&self) -> ConnectorResult<BTreeMap<String, String>> {
        let value = self
            .transport
            .send(Request::Read { path: "/dsapi/product/get_cats_info", parameters: vec![] })
            .await?;

        let mut names = BTreeMap::new();
        for category in value.as_array().ok_or_else(mapping)? {
            let category_id = id(&category["id"])?;
            let name = category["name"]
                .as_str()
                .map(str::trim)
                .filter(|name| !name.is_empty())
                .ok_or_else(mapping)?;
            if names.insert(category_id, name.to_owned()).is_some() {
                return Err(mapping());
            }
        }
        Ok(names)
    }

    // 一页规格解析完成后，有来源分类编号才补查字典，同页只查一次。
    // 只填更新项的名称，不改编号或撤回记录；查不到的名称保持未知，读取失败时本页不得推进检查点。
    pub(super) async fn name_catalog_categories(
        &self,
        items: &mut [Snapshot<OfferChange>],
    ) -> ConnectorResult<()> {
        if !items.iter().any(
            |item| matches!(&item.value, OfferChange::Upsert(offer) if offer.details.category_id.is_some()),
        ) {
            return Ok(());
        }

        let names = self.category_names().await?;
        for item in items {
            if let OfferChange::Upsert(offer) = &mut item.value {
                offer.details.category_name =
                    offer.details.category_id.as_ref().and_then(|id| names.get(id)).cloned();
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::num::NonZeroU32;

    use erp_supply::entity::failure::SupplierFailureClass;
    use erp_supply::ports::connector::common::{Lookup, Scan, ScanWindow};
    use erp_supply::ports::connector::offer::OfferSource;
    use serde_json::{Value, json};

    use super::super::error;
    use super::super::test_support::recording_connector;
    use super::*;

    fn scan() -> Scan {
        Scan { window: ScanWindow::Full, cursor: None, limit: NonZeroU32::new(1).unwrap() }
    }

    fn listed() -> Value {
        json!({"product_list":[{
            "id":"p1","title":"商品","cat_id":"1","cat_id2":0,"cat_id3":0,
            "specs":[{"spec_id":"s1","name":"6寸"},{"spec_id":"s2","name":"8寸"}]
        }]})
    }

    fn detail() -> Value {
        json!({
            "product":{"id":"p1","title":"商品","cat_id":"1","cat_id2":"34","cat_id3":"89","label_name":"热卖"},
            "specs":[{"id":"s1","name":"6寸"},{"id":"s2","name":"8寸"}]
        })
    }

    fn dictionary() -> Value {
        json!([
            {"id":"1","parent_id":"0","level":"1","name":"蛋糕"},
            {"id":34,"parent_id":"1","level":"2","name":"蛋糕甜品"},
            {"id":"89","parent_id":"34","level":"3","name":" 下午茶甜品 "}
        ])
    }

    #[tokio::test]
    async fn detailed_sync_names_every_spec_with_one_dictionary_request_and_preserves_paging() {
        let (connector, transport) = recording_connector(vec![
            Ok(listed()),
            Ok(detail()),
            Ok(dictionary()),
            Ok(detail()),
            Ok(dictionary()),
        ]);
        let page = connector.scan_with_details(&scan()).await.unwrap();
        assert_eq!(page.page.next.as_deref(), Some("2"));
        assert!(page.skipped.is_empty());
        assert_eq!(page.page.items.len(), 2);
        for item in &page.page.items {
            let OfferChange::Upsert(offer) = &item.value else { panic!("商品观察") };
            assert_eq!(offer.details.category_id.as_deref(), Some("89"));
            assert_eq!(offer.details.category_name.as_deref(), Some("下午茶甜品"));
        }
        assert_eq!(transport.requests.lock().unwrap().len(), 3);

        let OfferChange::Upsert(offer) = &page.page.items[0].value else { panic!("商品观察") };
        let Lookup::Found(current) = connector.offer(&offer.sku).await.unwrap() else {
            panic!("商品详情")
        };
        assert_eq!(current.value.details, offer.details);

        let requests = transport.requests.lock().unwrap();
        for index in [2, 4] {
            let Request::Read { path, parameters } = &requests[index] else { panic!("只读请求") };
            assert_eq!(*path, "/dsapi/product/get_cats_info");
            assert!(parameters.is_empty());
        }
    }

    #[tokio::test]
    async fn list_scan_names_parent_category_when_child_ids_are_zero() {
        let (connector, _) = recording_connector(vec![Ok(listed()), Ok(dictionary())]);
        let page = connector.scan(&scan()).await.unwrap();
        for item in &page.items {
            let OfferChange::Upsert(offer) = &item.value else { panic!("商品观察") };
            assert_eq!(offer.details.category_id.as_deref(), Some("1"));
            assert_eq!(offer.details.category_name.as_deref(), Some("蛋糕"));
        }
    }

    #[tokio::test]
    async fn missing_dictionary_entry_keeps_source_id_without_using_product_label() {
        let (connector, _) = recording_connector(vec![Ok(listed()), Ok(detail()), Ok(json!([]))]);
        let page = connector.scan_with_details(&scan()).await.unwrap();
        let OfferChange::Upsert(offer) = &page.page.items[0].value else { panic!("商品观察") };
        assert_eq!(offer.details.category_id.as_deref(), Some("89"));
        assert!(offer.details.category_name.is_none());
    }

    #[tokio::test]
    async fn malformed_dictionary_and_transport_failure_do_not_return_a_completed_page() {
        for dictionary in [
            json!({"unexpected":[]}),
            json!([{"id":"1","name":" "}]),
            json!([{"id":"1","name":123}]),
            json!([{"id":"","name":"蛋糕"}]),
            json!([{"id":"1","name":"蛋糕"},{"id":1,"name":"零食"}]),
        ] {
            let (connector, _) = recording_connector(vec![Ok(listed()), Ok(detail()), Ok(dictionary)]);
            assert_eq!(
                connector.scan_with_details(&scan()).await.unwrap_err().class,
                SupplierFailureClass::MappingError
            );
        }

        let failure = error(SupplierFailureClass::TransientFailure, "TEST_NETWORK", "分类读取失败");
        let (connector, _) = recording_connector(vec![Ok(listed()), Ok(detail()), Err(failure.clone())]);
        assert_eq!(connector.scan_with_details(&scan()).await.unwrap_err(), failure);
    }
}
