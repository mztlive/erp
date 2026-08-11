use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, ListingStatus, Sku, SkuRevision, SkuRevisionAttributeValue};
use entities::common::time::BusinessDate;
use entities::ids::{ProductId, SkuId};
use entities::money::{Amount, Quantity};

use super::super::extensions::CatalogExt;
use super::super::regex_filter::insert_literal_regex_filter;
use super::super::{PageResult, Pagination, QueryFilter, Repository};
use super::shared::{in_filter, sort_doc, SKUS, SKU_REVISIONS};
use super::CatalogRepository;
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sku_revision_attribute_value` 集合名（单一来源：`CatalogExt` 关联常量）。
const SKU_REVISION_ATTRIBUTE_VALUES: &str = <mongodb::Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES;

/// SKU 列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRow {
    /// 实体主键。
    pub id: String,
    /// SKU 编号。
    pub sku_no: String,
    /// 所属 SPU。
    pub product_id: String,
    /// 唯一基础单位。
    pub base_unit_id: String,
    /// 规范化规格签名。
    pub specification_signature: String,
    /// 启停状态。
    pub status: EnableStatus,
    /// 上架状态；旧文档缺失值按原有可售行为兼容为已上架。
    #[serde(default = "legacy_sku_listing_status")]
    pub listing_status: ListingStatus,
    /// 当前 SKU 修订 ID。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 上架概念引入前的 SKU 投影行保持原有可售行为。
fn legacy_sku_listing_status() -> ListingStatus {
    ListingStatus::Listed
}

/// SKU 列表筛选条件。
#[derive(Debug, Clone)]
pub struct SkuFilter {
    /// SKU 编号字面量正则（忽略大小写）；`None` 表示不筛选。
    pub sku_no: Option<String>,
    /// 所属 SPU；`None` 表示不筛选。
    pub product_id: Option<String>,
    /// 启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 上架状态；`None` 表示不筛选。
    pub listing_status: Option<ListingStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`sku_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "sku_no", self.sku_no.as_deref());
        if let Some(product_id) = &self.product_id {
            filter.insert("product_id", product_id);
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(listing_status) = self.listing_status {
            let value = match listing_status {
                ListingStatus::Listed => doc! { "$in": [ListingStatus::Listed.as_str(), null] },
                ListingStatus::Unlisted => doc! { "$eq": ListingStatus::Unlisted.as_str() },
            };
            filter.insert("listing_status", value);
        }
        filter
    }
}

impl Pagination for SkuFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, Sku> {
    /// 批量查询一组商品下的全部 SKU。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU 实体。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_product_ids(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<Sku>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("product_id", product_ids.iter().map(ToString::to_string)),
            executor,
        )
        .await
    }

    /// 分页检索 SKU 列表（投影查询）。
    ///
    /// 只返回 [`SkuRow`] 所需的列表字段；排序字段白名单化
    /// （`created_at`/`sku_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_skus(
        &self,
        filter: &SkuFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRow>> {
        let options = FindOptions::builder()
            .sort(sku_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按（商品，规范化规格签名）查找稳定 SKU 身份。
    ///
    /// 唯一性由 `uk_skus_product_spec` 唯一索引保证（数据模型 §6.3：签名在全
    /// 生命周期永久唯一，不随停用释放）。
    ///
    /// # 参数
    /// * `product_id` - 所属 SPU
    /// * `specification_signature` - 规范化规格签名
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_spec_signature(
        &self,
        product_id: &ProductId,
        specification_signature: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<Sku>> {
        self.find_one(
            doc! {
                "product_id": product_id.to_string(),
                "specification_signature": specification_signature,
            },
            executor,
        )
        .await
    }
}

/// SKU 修订列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SkuRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属稳定 SKU。
    pub sku_id: String,
    /// 修订序号。
    pub revision_no: u32,
    /// 公司审核后的 SKU 名称。
    pub name: String,
    /// 公司审核后的 SKU 描述。
    pub description: Option<String>,
    /// 公司审核后的规格或服务内容。
    pub specification: Option<String>,
    /// 条码原值（规范化精确查询字段）。
    pub barcode: Option<String>,
    /// 来源 SKU 主图（已归档受控文件，D05）。
    pub source_main_image_asset_id: Option<String>,
    /// 重量（千克）。
    pub weight_kg: Option<Quantity>,
    /// 体积（立方米）。
    pub volume_m3: Option<Quantity>,
    /// 修订启停状态。
    pub status: EnableStatus,
    /// 公司对销售可见的含税价格（Decimal128 定点金额）。
    pub sales_visible_price_gross: Option<Amount>,
    /// 市场参考价。
    pub market_price: Option<Amount>,
    /// 生效开始日。
    pub effective_from: BusinessDate,
    /// 生效结束日；空表示长期。
    pub effective_to: Option<BusinessDate>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// SKU 修订列表筛选条件（修订表追加写入，无软删除过滤）。
#[derive(Debug, Clone)]
pub struct SkuRevisionFilter {
    /// 所属稳定 SKU；`None` 表示不筛选。
    pub sku_id: Option<String>,
    /// 名称字面量正则（忽略大小写）；`None` 表示不筛选。
    pub name: Option<String>,
    /// 条码精确匹配（按实体构造时 trim 规范化后的持久化值）；`None` 表示不筛选。
    pub barcode: Option<String>,
    /// 修订启停状态；`None` 表示不筛选。
    pub status: Option<EnableStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`revision_no`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SkuRevisionFilter {
    /// 转换为 MongoDB 查询条件（修订表不参与软删除）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id);
        }
        insert_literal_regex_filter(&mut filter, "name", self.name.as_deref());
        if let Some(barcode) = &self.barcode {
            filter.insert("barcode", normalized_barcode(barcode));
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SkuRevisionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SkuRevision> {
    /// 分页检索 SKU 修订列表（投影查询）。
    ///
    /// 只返回 [`SkuRevisionRow`] 所需的列表字段（含 Decimal128 销售可见价，
    /// 不做舍入或换算）；排序字段白名单化（`created_at`/`revision_no`）。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回当前页投影行与满足筛选条件的总数。
    ///
    /// # 错误
    /// 当 MongoDB 查询、游标读取或计数失败时返回错误。
    pub async fn search_sku_revisions(
        &self,
        filter: &SkuRevisionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SkuRevisionRow>> {
        let options = FindOptions::builder()
            .sort(sku_revision_sort_doc(
                filter.sort_by.as_deref(),
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRevisionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按规范化条码精确查询全部「在用」SKU 修订。
    ///
    /// 条码走 `idx_sku_revisions_barcode` 精确查询索引；同一条码允许命中多个
    /// 在用 SKU 修订（数据模型 §6.3：同一条码出现多个在用 SKU 时由 Service
    /// 阻断正式启用并转人工，条码不是稳定身份）。
    ///
    /// # 参数
    /// * `barcode` - 条码原值（内部按 trim 规范化后精确匹配）
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回启用状态且条码精确匹配的 SKU 修订集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_active_by_barcode(
        &self,
        barcode: &str,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        self.find_many(
            doc! {
                "barcode": normalized_barcode(barcode),
                "status": EnableStatus::Active.as_str(),
            },
            executor,
        )
        .await
    }

    /// 批量查询一组 SKU 的修订（`$in`，一次取回）。
    ///
    /// 用于按 SKU 聚合修订明细，避免逐 SKU N+1。
    ///
    /// # 参数
    /// * `sku_ids` - SKU ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的修订实体集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = sku_ids.iter().map(|id| id.to_string()).collect();
        self.find_many(doc! { "sku_id": { "$in": ids } }, executor).await
    }
}

impl<'a> CatalogRepository<'a> {
    /// 建立「稳定 SKU + 首个 SKU 修订 + 修订规格属性值」（跨集合多步骤写入）。
    ///
    /// 依次写入 `skus`、`sku_revisions`、`sku_revision_attribute_values`，
    /// 保证「SKU 身份 + 修订快照 + 规格值」原子可见（数据模型 §6.3）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时各笔写入各自自动提交，中途失败会留下有 SKU 没有修订的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `sku` - 待写入的稳定 SKU
    /// * `revision` - 待写入的 SKU 首个修订
    /// * `attribute_values` - 待写入的修订规格属性值
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_sku_with_revision(
        &self,
        sku: &Sku,
        revision: &SkuRevision,
        attribute_values: &[SkuRevisionAttributeValue],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.db.collection::<Sku>(SKUS), sku, executor).await?;
        mongo_ops::insert_one(
            &self.db.collection::<SkuRevision>(SKU_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SkuRevisionAttributeValue>(SKU_REVISION_ATTRIBUTE_VALUES),
            attribute_values.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 按实体构造时的 trim 规则规范化条码（与 `SkuRevision::new` 一致）。
///
/// # 参数
/// * `barcode` - 条码原值
///
/// # 返回
/// 返回去除首尾空白后的规范化值。
fn normalized_barcode(barcode: &str) -> &str {
    barcode.trim()
}

/// 构建 SKU 排序文档（白名单：`created_at`/`sku_no`）。
fn sku_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("sku_no") => "sku_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// 构建 SKU 修订排序文档（白名单：`created_at`/`revision_no`）。
fn sku_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let field = match sort_by {
        Some("revision_no") => "revision_no",
        _ => "created_at",
    };
    sort_doc(field, sort_ascending)
}

/// SKU 列表投影字段。
fn sku_projection() -> Document {
    doc! {
        "id": 1,
        "sku_no": 1,
        "product_id": 1,
        "base_unit_id": 1,
        "specification_signature": 1,
        "status": 1,
        "listing_status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// SKU 修订列表投影字段。
fn sku_revision_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "revision_no": 1,
        "name": 1,
        "description": 1,
        "specification": 1,
        "barcode": 1,
        "source_main_image_asset_id": 1,
        "weight_kg": 1,
        "volume_m3": 1,
        "status": 1,
        "sales_visible_price_gross": 1,
        "market_price": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::catalog::{EnableStatus, ListingStatus};

    #[test]
    fn sku_revision_filter_normalizes_barcode_for_exact_query() {
        let filter = SkuRevisionFilter {
            sku_id: Some("sku-1".to_string()),
            name: None,
            barcode: Some(" 6901234567890 ".to_string()),
            status: Some(EnableStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("barcode").unwrap(), "6901234567890");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sku_filter_applies_listing_status() {
        let filter = SkuFilter {
            sku_no: None,
            product_id: Some("product-1".to_string()),
            status: Some(EnableStatus::Active),
            listing_status: Some(ListingStatus::Listed),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_str("product_id").unwrap(), "product-1");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let listing = document.get_document("listing_status").unwrap();
        assert_eq!(listing.get_array("$in").unwrap().len(), 2);
    }

    #[test]
    fn legacy_sku_row_without_listing_status_is_treated_as_listed() {
        let row: SkuRow = mongodb::bson::from_document(doc! {
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
}
