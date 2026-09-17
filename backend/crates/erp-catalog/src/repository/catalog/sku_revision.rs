use std::collections::HashMap;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::common::time::BusinessDate;
use erp_core::ids::{SkuId, SkuRevisionId};
use erp_core::money::{Amount, Quantity};
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, PageResult, Pagination, QueryFilter, Result, insert_literal_regex_filter};
use serde::{Deserialize, Serialize};

use super::shared::{batch_ids_filter, default_paging, select_current_revision, sort_doc, whitelisted_sort};
use crate::dto::catalog::SKU_REVISION_SORT_FIELDS;
use crate::entity::catalog::{EnableStatus, Sku, SkuRevision};
use crate::repository::CatalogExt;
use crate::repository::owned::SkuRevisionRepository;

/// `sku_revision_attribute_value` 集合名（单一来源：`CatalogExt` 关联常量）。
pub(super) const SKU_REVISION_ATTRIBUTE_VALUES: &str =
    <mongodb::Database as CatalogExt>::SKU_REVISION_ATTRIBUTE_VALUES;

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

impl Default for SkuRevisionFilter {
    /// 缺省分页从第一页、每页二十条开始，其余筛选保持空条件。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回第 1 页、每页 20 条的空筛选条件。
    ///
    /// # 错误
    /// 无。
    fn default() -> Self {
        let (page, page_size, sort_by, sort_ascending) = default_paging();
        Self {
            sku_id: None,
            name: None,
            barcode: None,
            status: None,
            page,
            page_size,
            sort_by,
            sort_ascending,
        }
    }
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

impl<'a> SkuRevisionRepository<'a> {
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
            .sort(sku_revision_sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sku_revision_projection())
            .build();
        let collection = self.collection().clone_with_type::<SkuRevisionRow>();
        super::shared::search_projected(&collection, &self.collection(), filter, options, executor).await
    }

    /// 按稳定 ID 读取发布修订引用的 SKU 修订。
    ///
    /// 发布修订通过 SKU 修订稳定 ID 引用不可变商品版本，本方法提供该跨域只读
    /// 事实的数据访问能力，不承载业务规则判断。
    ///
    /// # 参数
    /// * `id` - SKU 修订 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回未删除 SKU 修订；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 只查询本域 `sku_revisions` 集合，不触碰其他集合。
    pub async fn find_publication_sku_revision(
        &self,
        id: &SkuRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SkuRevision>> {
        self.find_by_id(id.as_ref(), executor).await
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
        let Some(filter) = batch_ids_filter("sku_id", sku_ids) else {
            return Ok(Vec::new());
        };
        self.find_many(filter, executor).await
    }

    /// 按稳定主键批量查询 SKU 修订。
    ///
    /// # 参数
    /// * `ids` - SKU 修订稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回匹配的未删除 SKU 修订实体；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_ids(
        &self,
        ids: &[SkuRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        let Some(filter) = batch_ids_filter("id", ids) else {
            return Ok(Vec::new());
        };
        self.find_many(filter, executor).await
    }

    /// 批量读取采购责任规则展示需要的 SKU 当前修订。
    ///
    /// 采购责任展示入口的历史名称，语义与 [`Self::find_by_ids`] 完全一致；
    /// 保留本方法以避免展示层调用方改名，仅委托属主批量查询，不重复实现查询。
    ///
    /// # 参数
    /// * `revision_ids` - SKU 修订 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定事务边界
    ///
    /// # 返回
    /// 返回全部匹配且未删除的 SKU 修订；输入为空时返回空集合。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_procurement_responsibility_sku_revisions(
        &self,
        revision_ids: &[SkuRevisionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuRevision>> {
        self.find_by_ids(revision_ids, executor).await
    }
}

/// 构建 SKU 修订排序文档（白名单：`created_at`/`revision_no`）。
pub(super) fn sku_revision_sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    sort_doc(whitelisted_sort(sort_by, SKU_REVISION_SORT_FIELDS), sort_ascending)
}

/// SKU 修订列表投影字段。
pub(super) fn sku_revision_projection() -> Document {
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

/// SKU 当前修订的名称匹配；仅库存搜索包含规格，保留既有调用方语义。
pub(super) fn sku_revision_keyword_filter(keyword: &str, include_specification: bool) -> Document {
    let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
    let mut name = Document::new();
    insert_literal_regex_filter(&mut name, "name", Some(keyword));
    if !include_specification {
        filter.extend(name);
        return filter;
    }
    let mut specification = Document::new();
    insert_literal_regex_filter(&mut specification, "specification", Some(keyword));
    filter.insert("$or", vec![name, specification]);
    filter
}

/// 从同一 SKU 的修订集合解析当前修订（erp-catalog-006）。
///
/// 优先当前修订指针，缺失回退最大修订号；判定内核见共享
/// [`select_current_revision`](super::shared::select_current_revision)。
pub(super) fn select_current_sku_revision<'a>(
    sku: &Sku,
    revisions: &'a [SkuRevision],
) -> Option<&'a SkuRevision> {
    select_current_revision(
        sku.stable.current_revision_id.as_deref(),
        revisions,
        |revision| revision.base.id.as_str(),
        |revision| revision.revision.revision_no,
    )
}

/// 批量解析 SKU 当前修订映射。
///
/// # 参数
/// * `skus` - SKU 稳定实体集合
/// * `revisions` - 这些 SKU 的全部修订
///
/// # 返回
/// 返回 `sku_id -> 当前修订` 映射，没有修订的 SKU 被忽略。
///
/// # 错误
/// 无。
pub(super) fn select_current_sku_revisions(
    skus: &[Sku],
    revisions: Vec<SkuRevision>,
) -> HashMap<String, SkuRevision> {
    let mut grouped: HashMap<String, Vec<SkuRevision>> = HashMap::new();
    for revision in revisions {
        grouped.entry(revision.sku_id.to_string()).or_default().push(revision);
    }
    skus.iter()
        .filter_map(|sku| {
            let revisions = grouped.get(&sku.base.id)?;
            select_current_sku_revision(sku, revisions)
                .cloned()
                .map(|revision| (sku.base.id.clone(), revision))
        })
        .collect()
}

/// 按实体构造时的 trim 规则规范化条码（与 `SkuRevision::new` 一致）。
///
/// # 参数
/// * `barcode` - 条码原值
///
/// # 返回
/// 返回去除首尾空白后的规范化值。
pub(super) fn normalized_barcode(barcode: &str) -> &str {
    barcode.trim()
}
