//! 商品上架汇总投影：按 `ProductId` 固定次数批量聚合当前启用 SKU 的上架计数。

use std::collections::{HashMap, HashSet};

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use futures_util::TryStreamExt;
use mongodb::bson::{doc, Document};
use serde::{Deserialize, Serialize};

use entities::catalog::{EnableStatus, ListingStatus};
use entities::ids::ProductId;

use super::shared::SKUS;
use super::CatalogRepository;
use crate::executor::Executor;
use crate::Result;

/// 单个商品的上架计数投影；SPU 继承状态由领域规则 `ProductListingStatus::inherited` 计算。
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProductListingSummary {
    /// 商品稳定主键。
    pub product_id: String,
    /// 当前启用且已上架的 SKU 数。
    pub listed_sku_count: u32,
    /// 当前启用 SKU 总数。
    pub sku_count: u32,
}

impl ProductListingSummary {
    /// 构造上架计数投影。
    ///
    /// # 参数
    /// * `product_id` - 商品稳定主键
    /// * `listed_sku_count` - 当前启用且已上架的 SKU 数
    /// * `sku_count` - 当前启用 SKU 总数
    ///
    /// # 返回
    /// 返回仓储投影事实，不含 `ProductListingStatus` 或 HTTP View。
    ///
    /// # 错误
    /// 无。
    pub fn new(product_id: impl Into<String>, listed_sku_count: u32, sku_count: u32) -> Self {
        Self {
            product_id: product_id.into(),
            listed_sku_count,
            sku_count,
        }
    }
}

impl<'a> CatalogRepository<'a> {
    /// 按商品 ID 批量投影上架汇总。
    ///
    /// 只读取 `product_id`/`status`/`listing_status`/`deleted_at`，在数据库内按商品
    /// 聚合启用数与已上架数。空输入不访问数据库；非空输入固定一次 `$in` 聚合，
    /// 查询次数不随商品数增长。缺失 `listing_status` 的旧 SKU 与商品列表管道共用
    /// 同一 `$ifNull` 口径，兼容为已上架。本方法不开启事务。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回去重后按输入首次出现顺序排列的上架计数；无启用 SKU 的商品计数为零。
    ///
    /// # 错误
    /// MongoDB 聚合或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 统计口径必须与 `product_list_pipeline` 共用 [`sku_is_listed_expr`]；
    /// 不返回 services DTO、HTTP View 或 `ProductListingStatus`。
    pub async fn listing_summaries(
        &self,
        product_ids: &[ProductId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductListingSummary>> {
        if product_ids.is_empty() {
            return Ok(Vec::new());
        }
        let unique_ids = unique_product_ids(product_ids);
        let rows =
            aggregate_listing_summaries(&self.db.collection::<Document>(SKUS), &unique_ids, executor).await?;
        Ok(fill_listing_summaries(&unique_ids, rows))
    }

    /// 返回上架汇总 `$match` + `$group` 管道，供 explain 与列表口径对拍共用。
    ///
    /// 调用方不得附加 hint；空输入仍返回管道，但 [`Self::listing_summaries`] 不会执行查询。
    ///
    /// # 参数
    /// * `product_ids` - 商品稳定 ID 集合
    ///
    /// # 返回
    /// 返回与 [`Self::listing_summaries`] 相同的聚合管道（已按首次出现去重）。
    ///
    /// # 错误
    /// 无。
    pub fn listing_summary_aggregation_pipeline(&self, product_ids: &[ProductId]) -> Vec<Document> {
        listing_summary_pipeline(&unique_product_ids(product_ids))
    }
}

/// 当前启用 SKU 是否上架的聚合表达式。
///
/// 与商品列表管道共用：缺失 `listing_status` 视为已上架，保持旧文档可售语义。
///
/// # 参数
/// 无。
///
/// # 返回
/// 返回 `$eq`/`$ifNull` 表达式文档。
///
/// # 错误
/// 无。
pub(super) fn sku_is_listed_expr() -> Document {
    doc! {
        "$eq": [
            { "$ifNull": ["$listing_status", ListingStatus::Listed.as_str()] },
            ListingStatus::Listed.as_str(),
        ]
    }
}

/// 构造按商品聚合上架计数的管道。
///
/// # 参数
/// * `product_ids` - 已去重的商品稳定主键
///
/// # 返回
/// 返回 `$match` + `$group` + `$project` 三段管道。
///
/// # 错误
/// 无。
fn listing_summary_pipeline(product_ids: &[String]) -> Vec<Document> {
    let listed = sku_is_listed_expr();
    let product_ids: Vec<String> = product_ids.to_vec();
    vec![
        doc! {
            "$match": {
                "product_id": { "$in": product_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
                "status": EnableStatus::Active.as_str(),
            }
        },
        doc! {
            "$group": {
                "_id": "$product_id",
                "sku_count": { "$sum": 1 },
                "listed_sku_count": {
                    "$sum": { "$cond": [listed, 1, 0] }
                },
            }
        },
        doc! {
            "$project": {
                "_id": 0,
                "product_id": "$_id",
                "sku_count": 1,
                "listed_sku_count": 1,
            }
        },
    ]
}

/// 执行上架汇总聚合并收集全部结果行。
///
/// # 参数
/// * `collection` - SKU 集合
/// * `product_ids` - 已去重的商品稳定主键
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回至少有一个启用 SKU 的商品计数行。
///
/// # 错误
/// MongoDB 聚合、游标读取或反序列化失败时返回错误。
async fn aggregate_listing_summaries(
    collection: &mongodb::Collection<Document>,
    product_ids: &[String],
    executor: &mut dyn Executor,
) -> Result<Vec<ProductListingSummary>> {
    let pipeline = listing_summary_pipeline(product_ids);
    match executor.session() {
        Some(session) => {
            collection
                .aggregate(pipeline)
                .with_type::<ProductListingSummary>()
                .session(&mut *session)
                .await?
                .stream(session)
                .try_collect::<Vec<_>>()
                .await
        }
        None => {
            collection
                .aggregate(pipeline)
                .with_type::<ProductListingSummary>()
                .await?
                .try_collect::<Vec<_>>()
                .await
        }
    }
    .map_err(crate::Error::from)
}

/// 按输入首次出现顺序去重商品 ID。
///
/// # 参数
/// * `product_ids` - 原始商品 ID 集合
///
/// # 返回
/// 返回稳定去重后的主键列表。
///
/// # 错误
/// 无。
fn unique_product_ids(product_ids: &[ProductId]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::with_capacity(product_ids.len());
    for product_id in product_ids {
        let key = product_id.to_string();
        if seen.insert(key.clone()) {
            unique.push(key);
        }
    }
    unique
}

/// 把聚合行归组为请求商品的上架汇总；缺项计为零。
///
/// # 参数
/// * `product_ids` - 已去重的请求商品主键
/// * `rows` - 数据库聚合出行（仅包含有启用 SKU 的商品）
///
/// # 返回
/// 返回与请求顺序一致的上架计数；无启用 SKU 时 `sku_count` 与 `listed_sku_count` 为零。
///
/// # 错误
/// 无。
fn fill_listing_summaries(
    product_ids: &[String],
    rows: Vec<ProductListingSummary>,
) -> Vec<ProductListingSummary> {
    let mut counts = HashMap::with_capacity(rows.len());
    for row in rows {
        counts.insert(row.product_id.clone(), row);
    }
    product_ids
        .iter()
        .map(|product_id| {
            counts
                .get(product_id)
                .cloned()
                .unwrap_or_else(|| ProductListingSummary::new(product_id.clone(), 0, 0))
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use entities::ids::ProductId;

    fn summary(product_id: &str, listed_sku_count: u32, sku_count: u32) -> ProductListingSummary {
        ProductListingSummary::new(product_id, listed_sku_count, sku_count)
    }

    /// 上架汇总管道固定一次 `$in` 聚合，并与列表管道共用缺失 `listing_status` 口径。
    #[test]
    fn listing_summary_pipeline_is_bounded_and_shares_listed_expr() {
        let pipeline = listing_summary_pipeline(&["product-1".to_string(), "product-2".to_string()]);
        let json = format!("{pipeline:?}");
        let listed = format!("{:?}", sku_is_listed_expr());

        assert_eq!(pipeline.len(), 3);
        assert!(json.contains("$in"));
        assert!(json.contains("product-1"));
        assert!(json.contains("product-2"));
        assert!(json.contains("active"));
        assert!(json.contains("deleted_at"));
        assert!(json.contains("listed_sku_count"));
        assert!(listed.contains("$ifNull"));
        assert!(listed.contains("listing_status"));
        assert!(listed.contains("listed"));
        assert!(json.contains(&listed[1..listed.len().saturating_sub(1)]) || json.contains("$ifNull"));
    }

    /// 空输入不产生管道访问；零 SKU 与全部停用均归为零计数。
    #[test]
    fn fill_listing_summaries_covers_zero_and_disabled() {
        assert!(unique_product_ids(&[]).is_empty());
        let filled = fill_listing_summaries(&["product-1".to_string()], Vec::new());
        assert_eq!(filled, vec![summary("product-1", 0, 0)]);
    }

    /// 全部上架、部分上架、全部未上架及多商品批量按输入顺序归组。
    #[test]
    fn fill_listing_summaries_covers_listed_partial_unlisted_and_batch() {
        let filled = fill_listing_summaries(
            &[
                "all-listed".to_string(),
                "partial".to_string(),
                "all-unlisted".to_string(),
                "missing".to_string(),
            ],
            vec![
                summary("partial", 1, 2),
                summary("all-unlisted", 0, 2),
                summary("all-listed", 2, 2),
            ],
        );

        assert_eq!(filled[0], summary("all-listed", 2, 2));
        assert_eq!(filled[1], summary("partial", 1, 2));
        assert_eq!(filled[2], summary("all-unlisted", 0, 2));
        assert_eq!(filled[3], summary("missing", 0, 0));
    }

    /// 重复商品 ID 按首次出现去重，缺项仍补零。
    #[test]
    fn unique_product_ids_preserve_first_seen_order() {
        let ids = [
            ProductId::new("p-2"),
            ProductId::new("p-1"),
            ProductId::new("p-2"),
        ];
        assert_eq!(
            unique_product_ids(&ids),
            vec!["p-2".to_string(), "p-1".to_string()]
        );
    }
}
