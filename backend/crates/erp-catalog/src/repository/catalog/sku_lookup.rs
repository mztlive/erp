use std::collections::{HashMap, HashSet};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::{ProductId, SkuId};
use mongodb::bson::{Document, doc};
use persistence_core::{Executor, PageResult, Result, insert_literal_regex_filter, mongo_ops};

use super::CatalogRepository;
use super::shared::{SKU_REVISIONS, SKUS, in_filter, max_revision_no};
use super::sku::{SkuFilter, SkuRow};
use super::sku_revision::{
    SKU_REVISION_ATTRIBUTE_VALUES, SkuRevisionFilter, SkuRevisionRow, select_current_sku_revisions,
    sku_revision_keyword_filter,
};
use crate::entity::catalog::{Sku, SkuRevision, SkuRevisionAttributeValue};
use crate::repository::CatalogExt;
use crate::repository::owned::{ProductRepository, SkuRepository, SkuRevisionRepository};

impl<'a> CatalogRepository<'a> {
    /// 按 SKU 编号或当前修订名称解析公司 SKU 主键。
    ///
    /// 两个字段均按字面量部分匹配并忽略大小写；名称命中只接受稳定 SKU 当前修订，
    /// 避免历史名称继续污染供给列表关键字筛选。
    ///
    /// # 参数
    /// * `keyword` - 已去除首尾空白的关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重并按主键排序的命中 SKU 主键；无命中时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn resolve_sku_ids_by_keyword(
        &self,
        keyword: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuId>> {
        self.keyword_sku_ids(keyword, false, executor).await
    }

    /// 解析库存搜索的 SKU 编码、当前名称和规格，不匹配历史修订。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败。
    pub async fn inventory_sku_ids(&self, keyword: &str, executor: &mut dyn Executor) -> Result<Vec<SkuId>> {
        self.keyword_sku_ids(keyword, true, executor).await
    }

    /// 共享当前修订匹配规则；库存搜索额外包含规格。
    async fn keyword_sku_ids(
        &self,
        keyword: &str,
        include_specification: bool,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuId>> {
        let mut sku_filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut sku_filter, "sku_no", Some(keyword));
        let mut skus = SkuRepository::new(self.db, SKUS).find_many(sku_filter, executor).await?;

        let revision_filter = sku_revision_keyword_filter(keyword, include_specification);
        let revisions =
            SkuRevisionRepository::new(self.db, SKU_REVISIONS).find_many(revision_filter, executor).await?;
        if !revisions.is_empty() {
            let revision_ids = revisions.into_iter().map(|revision| revision.base.id);
            skus.extend(
                SkuRepository::new(self.db, SKUS)
                    .find_many(in_filter("current_revision_id", revision_ids), executor)
                    .await?,
            );
        }

        let mut ids = skus.into_iter().map(|sku| SkuId::new(sku.base.id)).collect::<Vec<_>>();
        ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        Ok(ids)
    }

    /// 按 SPU 编号和 SKU 编号解析供给筛选所需的公司 SKU 主键。
    ///
    /// 两个字段均按字面量部分匹配并忽略大小写；同时提供时取交集。任一已提供
    /// 条件无命中时返回空集合，`None` 只表示两个条件均未提供。
    ///
    /// # 参数
    /// * `product_no` - 公司商品编号筛选
    /// * `sku_no` - 公司 SKU 编号筛选
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回可选的去重 SKU 主键集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn resolve_sku_ids_by_codes(
        &self,
        product_no: Option<&str>,
        sku_no: Option<&str>,
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<SkuId>>> {
        let by_product = match product_no {
            Some(product_no) => Some(self.sku_ids_by_product_no(product_no, executor).await?),
            None => None,
        };
        let by_sku = match sku_no {
            Some(sku_no) => Some(self.sku_ids_by_sku_no(sku_no, executor).await?),
            None => None,
        };
        let mut ids = match (by_product, by_sku) {
            (None, None) => return Ok(None),
            (Some(ids), None) | (None, Some(ids)) => ids,
            (Some(left), Some(right)) => left.into_iter().filter(|id| right.contains(id)).collect(),
        };
        ids.sort_by(|left, right| left.as_ref().cmp(right.as_ref()));
        ids.dedup_by(|left, right| left.as_ref() == right.as_ref());
        Ok(Some(ids))
    }

    /// 按商品编号解析其下全部 SKU 主键。
    ///
    /// # 参数
    /// * `product_no` - 公司商品编号字面量关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中商品下的全部未删除 SKU 主键。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn sku_ids_by_product_no(
        &self,
        product_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuId>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "product_no", Some(product_no));
        let products = ProductRepository::new(self.db, <mongodb::Database as CatalogExt>::PRODUCTS)
            .find_many(filter, executor)
            .await?;
        if products.is_empty() {
            return Ok(Vec::new());
        }
        let product_ids =
            products.into_iter().map(|product| ProductId::new(product.base.id)).collect::<Vec<_>>();
        Ok(SkuRepository::new(self.db, SKUS)
            .find_by_product_ids(&product_ids, executor)
            .await?
            .into_iter()
            .map(|sku| SkuId::new(sku.base.id))
            .collect())
    }

    /// 按 SKU 编号解析 SKU 主键。
    ///
    /// # 参数
    /// * `sku_no` - 公司 SKU 编号字面量关键字
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回命中的未删除 SKU 主键。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    async fn sku_ids_by_sku_no(&self, sku_no: &str, executor: &mut dyn Executor) -> Result<Vec<SkuId>> {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "sku_no", Some(sku_no));
        Ok(SkuRepository::new(self.db, SKUS)
            .find_many(filter, executor)
            .await?
            .into_iter()
            .map(|sku| SkuId::new(sku.base.id))
            .collect())
    }

    /// 分页查询 SKU 并批量装配当前修订名称。
    ///
    /// # 参数
    /// * `keyword` - SKU 编号、当前名称、规格、商品编号或当前商品名称关键字；`None` 表示不筛选
    /// * `filter` - SKU 编号、归属、状态、分页与排序条件；`ids` 与关键词匹配身份取交集
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已带当前修订名称的 SKU 分页投影。
    ///
    /// # 错误
    /// MongoDB 查询、计数或当前修订批量读取失败时返回错误。
    pub async fn sku_page(
        &self,
        keyword: Option<&str>,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>> {
        let mut filter = filter.clone();
        if let Some(keyword) = keyword {
            let matched = self.list_keyword_sku_ids(keyword, executor).await?;
            filter.ids = Some(
                matched
                    .into_iter()
                    .map(|id| id.to_string())
                    .filter(|id| filter.ids.as_ref().is_none_or(|ids| ids.contains(id)))
                    .collect(),
            );
        }
        let mut result = self.db.skus().search_skus(&filter, executor).await?;
        self.attach_current_sku_names(&mut result.items, executor).await?;
        Ok(result)
    }

    /// 分页查询 SKU 修订投影。
    ///
    /// # 参数
    /// * `filter` - SKU 修订、名称、条码、状态、分页与排序条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 SKU 修订分页投影。
    ///
    /// # 错误
    /// MongoDB 查询、计数或结果反序列化失败时返回错误。
    pub async fn sku_revision_page(
        &self,
        filter: &SkuRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRevisionRow>> {
        self.db.sku_revisions().search_sku_revisions(filter, executor).await
    }

    /// 按稳定 ID 读取单个未删除 SKU。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配 SKU；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn sku(&self, sku_id: &str, executor: &mut dyn Executor) -> Result<Option<Sku>> {
        self.db.skus().find_by_id(sku_id, executor).await
    }

    /// 读取一个商品下的全部未删除 SKU。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该商品的全部 SKU，包含启用和历史停用身份。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn skus_for_product(
        &self,
        product_id: &ProductId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        self.db.skus().find_by_product_ids(std::slice::from_ref(product_id), executor).await
    }

    /// 读取指定 SKU 的历史最大修订序号。
    ///
    /// # 参数
    /// * `sku_id` - SKU 稳定 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回历史最大修订号；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn latest_sku_revision_no(
        &self,
        sku_id: &SkuId,
        executor: &mut dyn Executor,
    ) -> Result<Option<u32>> {
        max_revision_no(
            &self.db.sku_revisions().collection().clone_with_type(),
            doc! { "sku_id": sku_id.to_string() },
            executor,
        )
        .await
    }

    /// 返回当前启用修订中占用规范化条码的 SKU 身份。
    ///
    /// # 参数
    /// * `barcode` - 条码原值，Repository 按实体 trim 规则规范化
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重后的条码占用 SKU ID 集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn barcode_owner_sku_ids(
        &self,
        barcode: &str,
        executor: &mut dyn Executor,
    ) -> Result<HashSet<String>> {
        Ok(self
            .db
            .sku_revisions()
            .find_active_by_barcode(barcode, executor)
            .await?
            .into_iter()
            .map(|revision| revision.sku_id.to_string())
            .collect())
    }

    /// 批量解析一组 SKU 的当前修订。
    ///
    /// 优先使用稳定主表当前修订指针；指针缺失或失效时回退到该 SKU 最大修订号。
    ///
    /// # 参数
    /// * `skus` - 待解析的 SKU 稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回 `sku_id -> 当前 SKU 修订` 映射；没有修订的 SKU 不出现在映射中。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    pub async fn current_sku_revisions(
        &self,
        skus: &[Sku],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, SkuRevision>> {
        let sku_ids = skus.iter().map(|sku| SkuId::new(sku.base.id.clone())).collect::<Vec<_>>();
        let revisions = self.db.sku_revisions().find_by_sku_ids(&sku_ids, executor).await?;
        Ok(select_current_sku_revisions(skus, revisions))
    }

    /// 解析单个 SKU 的当前修订。
    ///
    /// # 参数
    /// * `sku` - 已加载的 SKU 稳定实体
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前指针命中或最大修订号对应的修订；无修订时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn current_sku_revision(
        &self,
        sku: &Sku,
        executor: &mut dyn Executor,
    ) -> Result<Option<SkuRevision>> {
        Ok(self.current_sku_revisions(std::slice::from_ref(sku), executor).await?.remove(&sku.base.id))
    }

    /// 批量装配 SKU 列表投影的当前修订名称。
    ///
    /// # 参数
    /// * `rows` - 当前页 SKU 投影
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 成功时原位填充每行 `name` 字段。
    ///
    /// # 错误
    /// MongoDB 批量查询或反序列化失败时返回错误。
    async fn attach_current_sku_names(&self, rows: &mut [SkuRow], executor: &mut dyn Executor) -> Result<()> {
        let revision_ids = rows.iter().filter_map(|row| row.current_revision_id.clone()).collect::<Vec<_>>();
        if revision_ids.is_empty() {
            return Ok(());
        }
        let revisions = self.db.sku_revisions().find_many(in_filter("id", revision_ids), executor).await?;
        let names = revisions
            .into_iter()
            .map(|revision| (revision.base.id, revision.name))
            .collect::<HashMap<_, _>>();
        for row in rows {
            row.name =
                row.current_revision_id.as_ref().and_then(|revision_id| names.get(revision_id).cloned());
        }
        Ok(())
    }

    /// 建立「稳定 SKU + 首个 SKU 修订 + 修订规格属性值」（跨集合多步骤写入）。
    ///
    /// 依次写入 `skus`、`sku_revisions`、`sku_revision_attribute_values`，
    /// 保证「SKU 身份 + 修订快照 + 规格值」原子可见（数据模型 §6.3）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下有 SKU 没有修订的半成品；
    /// Service 必须通过 `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `sku` - 待写入的稳定 SKU
    /// * `revision` - 待写入的 SKU 首个修订
    /// * `attribute_values` - 待写入的修订规格属性值
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sku_with_revision(
        &self,
        sku: &Sku,
        revision: &SkuRevision,
        attribute_values: &[SkuRevisionAttributeValue],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Sku>(SKUS), sku, executor).await?;
        mongo_ops::insert_one(&self.db.collection::<SkuRevision>(SKU_REVISIONS), revision, executor).await?;
        mongo_ops::insert_many(
            &self.db.collection::<SkuRevisionAttributeValue>(SKU_REVISION_ATTRIBUTE_VALUES),
            attribute_values.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

impl CatalogRepository<'_> {
    /// 公司 SKU 列表匹配 SKU 编号、当前名称/规格及商品编号/当前名称。
    ///
    /// 保持库存专用搜索口径不变；只接受商品当前修订，数据库错误整次返回。
    async fn list_keyword_sku_ids(&self, keyword: &str, executor: &mut dyn Executor) -> Result<Vec<SkuId>> {
        let mut ids = self.keyword_sku_ids(keyword, true, executor).await?;
        let mut name_filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut name_filter, "name", Some(keyword));
        let revisions = self.db.collection::<Document>(<mongodb::Database as CatalogExt>::PRODUCT_REVISIONS);
        let mut query = revisions.distinct("id", name_filter);
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        let revision_ids = query.await?;
        let mut number = Document::new();
        insert_literal_regex_filter(&mut number, "product_no", Some(keyword));
        let products = self.db.collection::<Document>(<mongodb::Database as CatalogExt>::PRODUCTS);
        let mut query = products.distinct("id", doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "$or": [number, doc! { "current_revision_id": { "$in": revision_ids } }] });
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        let product_ids = query.await?;
        let skus = self.db.collection::<Document>(SKUS);
        let mut query = skus.distinct(
            "id",
            doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON, "product_id": { "$in": product_ids } },
        );
        if let Some(session) = executor.session() {
            query = query.session(session);
        }
        ids.extend(
            query.await?.into_iter().filter_map(|id| id.as_str().map(|value| SkuId::new(value.to_owned()))),
        );
        ids.sort_by_key(ToString::to_string);
        ids.dedup();
        Ok(ids)
    }
}

#[cfg(test)]
mod tests {
    use persistence_core::QueryFilter;

    use super::*;
    use crate::entity::catalog::{EnableStatus, ListingStatus};

    #[test]
    fn sku_revision_filter_normalizes_barcode_for_exact_query() {
        let filter = SkuRevisionFilter {
            sku_id: Some("sku-1".to_string()),
            barcode: Some(" 6901234567890 ".to_string()),
            status: Some(EnableStatus::Active),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("barcode").unwrap(), "6901234567890");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_filter_applies_listing_status() {
        let filter = SkuFilter {
            product_id: Some("product-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ListingStatus::Listed),
            ..Default::default()
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("product_id").unwrap(), "product-1");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let listing = document.get_document("listing_status").unwrap();
        assert_eq!(listing.get_array("$in").unwrap().len(), 2);
    }

    #[test]
    fn legacy_sku_row_without_listing_status_is_treated_as_listed() {
        let row: SkuRow = mongodb::bson::deserialize_from_document(doc! {
            "id": "sku-1",
            "sku_no": "SKU-001",
            "product_id": "product-1",
            "base_unit_id": "unit-1",
            "specification_signature": "",
            "status": "active",
            "current_revision_id": "sku-revision-1",
            "version": 1_i64,
            "created_at": 1_i64,
        })
        .unwrap();

        assert_eq!(row.listing_status, ListingStatus::Listed);
    }

    #[test]
    fn sku_revision_roundtrips_through_bson() {
        use std::str::FromStr;

        use erp_core::common::time::BusinessDate;
        use erp_core::ids::{FileAssetId, SkuId, SkuRevisionId};
        use erp_core::money::{Amount, Quantity};
        use mongodb::bson::{self, Bson};

        use crate::entity::catalog::EnableStatus;
        use crate::entity::catalog::sku_revision::{SkuRevision, SkuRevisionData};

        let revision = SkuRevision::new(
            SkuRevisionId::new("rev-1"),
            SkuRevisionData {
                sku_id: SkuId::new("sku-1"),
                revision_no: 1,
                name: "坚果礼盒 500g".to_string(),
                description: None,
                specification: None,
                barcode: None,
                source_main_image_asset_id: Some(FileAssetId::new("asset-main-1")),
                weight_kg: Some(Quantity::from_str("0.500000").unwrap()),
                volume_m3: None,
                sales_visible_price_gross: Some(Amount::from_str("99.90").unwrap()),
                market_price: Some(Amount::from_str("129.00").unwrap()),
                status: EnableStatus::Active,
                effective_from: BusinessDate::from_ymd(2026, 1, 1).unwrap(),
                effective_to: None,
            },
        )
        .unwrap();
        let bytes = bson::serialize_to_vec(&revision).unwrap();
        let wire_doc: bson::Document = bson::deserialize_from_slice(&bytes).unwrap();
        assert!(matches!(wire_doc.get("sales_visible_price_gross"), Some(Bson::Decimal128(_))));
        assert!(matches!(wire_doc.get("weight_kg"), Some(Bson::Decimal128(_))));
        let back: SkuRevision = bson::deserialize_from_slice(&bytes).unwrap();
        assert_eq!(back, revision);
    }
}

#[cfg(test)]
mod inventory_keyword_tests {
    use super::*;
    /// 库存额外支持字面量规格，既有商品编号与名称搜索保持原语义。
    #[test]
    fn inventory_search_includes_literal_specification_without_changing_other_consumers() {
        let regular = sku_revision_keyword_filter("500ml.[x]", false);
        assert!(regular.contains_key("name"));
        assert!(!regular.contains_key("$or"));
        let inventory = sku_revision_keyword_filter("500ml.[x]", true);
        assert!(inventory.contains_key("deleted_at"));
        let clauses = inventory.get_array("$or").unwrap();
        let spec = clauses[1].as_document().unwrap().get_document("specification").unwrap();
        assert_eq!(spec.get_str("$regex").unwrap(), r"500ml\.\[x\]");
    }
}
