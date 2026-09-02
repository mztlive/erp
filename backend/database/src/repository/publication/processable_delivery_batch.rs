//! 待处理发布投递批次上下文（MASTER-R04）。
//!
//! 一次有界读取返回投递及其关联修订、稳定发布事实；查询次数固定为最多三次，
//! 不随批次条数增长。全部使用调用方 executor，不开事务；商城调用、逐项处理、
//! 结果统计与缺失关系失败策略仍由 Service 承担。

use std::collections::{HashMap, HashSet};

use entities::common::time::Instant;
use entities::publication::{
    ProductPublication, ProductPublicationDelivery, ProductPublicationRevision, PublicationDeliveryStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use super::super::extensions::PublicationExt;
use super::PublicationRepository;
use crate::executor::Executor;
use crate::Result;

/// 一条待处理投递及其关联修订、稳定发布事实。
///
/// 修订或发布缺失时对应字段为 `None`，由 Service 按既有 NotFound 语义失败关闭，
/// 仓储不得静默跳过坏关系，也不得把商城调用纳入本查询。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ProcessablePublicationDeliveryContext {
    /// 待处理投递。
    pub delivery: ProductPublicationDelivery,
    /// 投递引用的发布修订；缺失时为 `None`。
    pub revision: Option<ProductPublicationRevision>,
    /// 修订所属稳定发布；修订缺失或发布缺失时为 `None`。
    pub publication: Option<ProductPublication>,
}

impl<'a> PublicationRepository<'a> {
    /// 列出待发送与已到期重试投递，并批量附带关联修订与稳定发布。
    ///
    /// 固定最多三次数据库访问：先按到期条件有界列出投递；再按修订 ID `$in`
    /// 批量读取修订；再按发布 ID `$in` 批量读取稳定发布。空批次只访问投递集合
    /// 一次；无命中修订时跳过发布读取。处理顺序、`limit` 与到期重试筛选与
    /// `list_processable_publication_deliveries` 一致。
    /// 本方法不开启或提交事务，不调用外部商城，不把缺失关系折叠为跳过。
    ///
    /// # 参数
    /// * `at` - 到期判定时刻；`retrying` 且 `next_attempt_at <= at` 才入选
    /// * `limit` - 本批最多返回条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `created_at,id` 升序的投递上下文；关联修订或发布不存在时对应
    /// 字段为 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    ///
    /// # 约束
    /// 查询次数不随批次条数线性增长；索引依赖
    /// `idx_product_publication_deliveries_processable` 与修订/发布 `id` 唯一索引。
    pub async fn list_processable_publication_delivery_contexts(
        &self,
        at: Instant,
        limit: u32,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProcessablePublicationDeliveryContext>> {
        let deliveries = self
            .db
            .product_publication_deliveries()
            .list_processable_publication_deliveries(at, limit, executor)
            .await?;
        let revision_ids = unique_ids_in_first_seen_order(
            deliveries
                .iter()
                .map(|row| row.publication_revision_id.to_string()),
        );
        let Some(revision_filter) = related_id_in_filter(&revision_ids) else {
            return Ok(Vec::new());
        };
        let revisions = self
            .db
            .product_publication_revisions()
            .find_many(revision_filter, executor)
            .await?;

        let publication_ids = unique_ids_in_first_seen_order(
            revisions.iter().map(|row| row.product_publication_id.to_string()),
        );
        let publications = match related_id_in_filter(&publication_ids) {
            Some(publication_filter) => {
                self.db
                    .product_publications()
                    .find_many(publication_filter, executor)
                    .await?
            }
            None => Vec::new(),
        };

        Ok(assemble_processable_publication_delivery_contexts(
            deliveries,
            revisions,
            publications,
        ))
    }
}

/// 构建待处理投递查询条件。
///
/// 只接受未删除的 `pending_send`，或已到期的 `retrying`；发送中、结果未知、
/// 已确认、失败与转人工不得入选。
///
/// # 参数
/// * `at` - 到期判定时刻
///
/// # 返回
/// 返回含软删除过滤与 `$or` 状态筛选的查询文档。
///
/// # 错误
/// 无。
///
/// # 约束
/// 不得把未来 `next_attempt_at` 的重试记录纳入本批。
pub(crate) fn processable_publication_delivery_filter(at: Instant) -> Document {
    doc! {
        "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        "$or": [
            { "delivery_status": PublicationDeliveryStatus::PendingSend.as_str() },
            {
                "delivery_status": PublicationDeliveryStatus::Retrying.as_str(),
                "next_attempt_at": { "$lte": at.unix_secs() },
            },
        ],
    }
}

/// 构建待处理投递的稳定排序与有界条数。
///
/// # 参数
/// * `limit` - 本批最多返回条数
///
/// # 返回
/// 返回按 `created_at,id` 升序并限制条数的查询选项。
///
/// # 错误
/// 无。
///
/// # 约束
/// 同值 `created_at` 必须以 `id` 作为次排序，保证处理顺序稳定。
pub(crate) fn processable_publication_delivery_find_options(limit: u32) -> FindOptions {
    FindOptions::builder()
        .sort(doc! { "created_at": 1, "id": 1 })
        .limit(i64::from(limit))
        .build()
}

/// 构造关联文档的单次 `$in` 过滤；空 ID 表示跳过下一次读取。
///
/// 本函数是批次上下文查询次数的控制流：投递列出后若无修订 ID 则不再读取；
/// 修订批量命中后若无发布 ID 则跳过发布读取。非空 ID 无论 1 条还是 50 条都只
/// 形成一份 `$in` 过滤，禁止按 ID 逐条 `find_by_id`。
///
/// # 参数
/// * `ids` - 已去重的关联文档稳定 ID
///
/// # 返回
/// 空切片返回 `None`（跳过本次 `find_many`）；否则返回 `{ id: { $in: ids } }`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 查询次数只取决于是否继续下一次固定读取，不随 `ids.len()` 增长。
fn related_id_in_filter(ids: &[String]) -> Option<Document> {
    if ids.is_empty() {
        None
    } else {
        Some(doc! { "id": { "$in": ids } })
    }
}

/// 按首次出现顺序去重稳定 ID。
///
/// # 参数
/// * `ids` - 原始 ID 序列，可含重复
///
/// # 返回
/// 返回去重后的 ID 列表；空输入返回空列表。
///
/// # 错误
/// 无。
///
/// # 约束
/// `$in` 查询使用本列表，不得按哈希表迭代顺序生成。
fn unique_ids_in_first_seen_order(ids: impl IntoIterator<Item = String>) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for id in ids {
        if seen.insert(id.clone()) {
            unique.push(id);
        }
    }
    unique
}

/// 按投递顺序装配修订与发布事实，缺失关系保留为 `None`。
///
/// # 参数
/// * `deliveries` - 已按处理顺序排列的投递
/// * `revisions` - 批量读取的修订，可缺项、可含未引用项
/// * `publications` - 批量读取的稳定发布，可缺项、可含未引用项
///
/// # 返回
/// 返回与 `deliveries` 等长、同序的上下文；修订或发布不存在时为 `None`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 不得丢弃坏关系投递，也不得按修订/发布集合重排处理顺序。
fn assemble_processable_publication_delivery_contexts(
    deliveries: Vec<ProductPublicationDelivery>,
    revisions: Vec<ProductPublicationRevision>,
    publications: Vec<ProductPublication>,
) -> Vec<ProcessablePublicationDeliveryContext> {
    let revisions_by_id: HashMap<String, ProductPublicationRevision> = revisions
        .into_iter()
        .map(|revision| (revision.base.id.clone(), revision))
        .collect();
    let publications_by_id: HashMap<String, ProductPublication> = publications
        .into_iter()
        .map(|publication| (publication.base.id.clone(), publication))
        .collect();

    deliveries
        .into_iter()
        .map(|delivery| {
            let revision = revisions_by_id
                .get(delivery.publication_revision_id.as_ref())
                .cloned();
            let publication = revision.as_ref().and_then(|revision| {
                publications_by_id
                    .get(revision.product_publication_id.as_ref())
                    .cloned()
            });
            ProcessablePublicationDeliveryContext {
                delivery,
                revision,
                publication,
            }
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::{
        assemble_processable_publication_delivery_contexts, processable_publication_delivery_filter,
        processable_publication_delivery_find_options, related_id_in_filter, unique_ids_in_first_seen_order,
    };
    use entities::common::time::Instant;
    use entities::ids::{
        ProductCategoryId, ProductPublicationDeliveryId, ProductPublicationId, ProductPublicationRevisionId,
        SkuId, SkuRevisionId, SourceSystemId, SupplierOfferingRevisionId,
    };
    use entities::money::{Amount, Quantity, Rate};
    use entities::publication::{
        ProductCapability, ProductPublication, ProductPublicationData, ProductPublicationDelivery,
        ProductPublicationDeliveryData, ProductPublicationRevision, ProductPublicationRevisionData,
        ProductPublicationStatus, PublicationDeliveryStatus, SaleStatus,
    };
    use mongodb::bson::doc;
    use std::str::FromStr;

    /// 解析测试金额。
    ///
    /// # 参数
    /// * `value` - 定点金额字符串
    ///
    /// # 返回
    /// 返回金额。
    ///
    /// # 错误
    /// 非法金额时 panic。
    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    /// 解析测试数量。
    ///
    /// # 参数
    /// * `value` - 定点数量字符串
    ///
    /// # 返回
    /// 返回数量。
    ///
    /// # 错误
    /// 非法数量时 panic。
    fn quantity(value: &str) -> Quantity {
        Quantity::from_str(value).unwrap()
    }

    /// 解析测试税率。
    ///
    /// # 参数
    /// * `value` - 定点税率字符串
    ///
    /// # 返回
    /// 返回税率。
    ///
    /// # 错误
    /// 非法税率时 panic。
    fn rate(value: &str) -> Rate {
        Rate::from_str(value).unwrap()
    }

    /// 构造稳定发布夹具。
    ///
    /// # 参数
    /// * `id` - 发布主键
    /// * `mall_id` - 目标商城
    ///
    /// # 返回
    /// 返回合法稳定发布。
    ///
    /// # 错误
    /// 夹具非法时 panic。
    fn publication(id: &str, mall_id: &str) -> ProductPublication {
        ProductPublication::new(
            ProductPublicationId::new(id),
            ProductPublicationData {
                sku_id: SkuId::new(format!("sku-{id}")),
                target_mall_id: SourceSystemId::new(mall_id),
                status: ProductPublicationStatus::PendingPublish,
            },
            "tester",
        )
        .expect("测试发布必须合法")
    }

    /// 构造发布修订夹具。
    ///
    /// # 参数
    /// * `id` - 修订主键
    /// * `publication_id` - 所属稳定发布
    ///
    /// # 返回
    /// 返回合法发布修订。
    ///
    /// # 错误
    /// 夹具非法时 panic。
    fn revision(id: &str, publication_id: &str) -> ProductPublicationRevision {
        ProductPublicationRevision::new(
            ProductPublicationRevisionId::new(id),
            1,
            ProductPublicationRevisionData {
                product_publication_id: ProductPublicationId::new(publication_id),
                sku_revision_id: SkuRevisionId::new("sku-rev-1"),
                supplier_offering_revision_id: SupplierOfferingRevisionId::new("off-rev-1"),
                category_id: ProductCategoryId::new("cat-1"),
                name: "福利商城卡".to_string(),
                specification: None,
                sales_description: "员工福利采购".to_string(),
                minimum_purchase_quantity: quantity("1.000000"),
                sales_price_gross: amount("100.00"),
                sales_tax_rate: rate("0.130000"),
                base_unit_code: "张".to_string(),
                sales_region: None,
                sale_status: SaleStatus::OnSale,
                product_capabilities: vec![ProductCapability::Cancel],
                valid_from: Instant::from_unix_secs(1_700_000_000),
                valid_to: Some(Instant::from_unix_secs(1_800_000_000)),
            },
        )
        .expect("测试修订必须合法")
    }

    /// 构造待发送投递夹具。
    ///
    /// # 参数
    /// * `id` - 投递主键
    /// * `revision_id` - 关联修订
    /// * `mall_id` - 目标商城
    ///
    /// # 返回
    /// 返回合法待发送投递。
    ///
    /// # 错误
    /// 夹具非法时 panic。
    fn pending_delivery(id: &str, revision_id: &str, mall_id: &str) -> ProductPublicationDelivery {
        ProductPublicationDelivery::new(
            ProductPublicationDeliveryId::new(id),
            ProductPublicationDeliveryData {
                publication_revision_id: ProductPublicationRevisionId::new(revision_id),
                target_mall_id: SourceSystemId::new(mall_id),
                delivery_status: PublicationDeliveryStatus::PendingSend,
                attempt_count: 0,
                last_attempt_at: None,
                next_attempt_at: None,
                mall_ack_at: None,
                mall_version: None,
                error_class: None,
                error_code: None,
                error_summary: None,
            },
        )
        .expect("测试投递必须合法")
    }

    #[test]
    fn processable_filter_keeps_pending_and_due_retry_only() {
        let filter = processable_publication_delivery_filter(Instant::from_unix_secs(1_700_000_100));
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let choices = filter.get_array("$or").unwrap();
        assert_eq!(choices.len(), 2);
        assert_eq!(
            choices[0]
                .as_document()
                .unwrap()
                .get_str("delivery_status")
                .unwrap(),
            "pending_send"
        );
        let retry = choices[1].as_document().unwrap();
        assert_eq!(retry.get_str("delivery_status").unwrap(), "retrying");
        assert_eq!(
            retry
                .get_document("next_attempt_at")
                .unwrap()
                .get_i64("$lte")
                .unwrap(),
            1_700_000_100
        );
    }

    #[test]
    fn processable_options_are_stable_sorted_and_bounded() {
        let options = processable_publication_delivery_find_options(2);
        assert_eq!(options.sort, Some(doc! { "created_at": 1, "id": 1 }));
        assert_eq!(options.limit, Some(2));
    }

    #[test]
    fn unique_ids_preserve_first_seen_order_and_empty() {
        assert!(unique_ids_in_first_seen_order(Vec::<String>::new()).is_empty());
        assert_eq!(
            unique_ids_in_first_seen_order(["rev-b".into(), "rev-a".into(), "rev-b".into()]),
            vec!["rev-b".to_string(), "rev-a".to_string()]
        );
    }

    #[test]
    fn related_id_reads_skip_empty_and_stay_one_in_filter_for_any_batch_size() {
        assert_eq!(related_id_in_filter(&[]), None, "空批次不得再发关联读取");

        assert_eq!(
            related_id_in_filter(&["rev-1".to_string()]),
            Some(doc! { "id": { "$in": ["rev-1"] } }),
            "单条必须是一次 $in 而不是 find_by_id"
        );

        let many: Vec<String> = (0..50).map(|index| format!("rev-{index}")).collect();
        let filter = related_id_in_filter(&many).expect("非空批次必须发出一次 $in");
        let ids = filter
            .get_document("id")
            .expect("必须按 id 过滤")
            .get_array("$in")
            .expect("必须是单次 $in");
        assert_eq!(ids.len(), 50, "50 条仍是一份 $in，查询次数不得随批次数变成 1+2N");
        assert_eq!(
            related_id_in_filter(&["rev-a".to_string(), "rev-b".to_string()]),
            Some(doc! { "id": { "$in": ["rev-a", "rev-b"] } })
        );
    }

    #[test]
    fn assemble_covers_empty_single_many_and_missing_relations() {
        assert!(
            assemble_processable_publication_delivery_contexts(Vec::new(), Vec::new(), Vec::new()).is_empty()
        );

        let first = pending_delivery("del-1", "rev-1", "mall-1");
        let second = pending_delivery("del-2", "rev-2", "mall-2");
        let missing_revision = pending_delivery("del-3", "rev-missing", "mall-3");
        let missing_publication = pending_delivery("del-4", "rev-orphan", "mall-4");
        let shared = pending_delivery("del-5", "rev-1", "mall-5");

        let rev_1 = revision("rev-1", "pub-1");
        let rev_2 = revision("rev-2", "pub-2");
        let rev_orphan = revision("rev-orphan", "pub-missing");
        let pub_1 = publication("pub-1", "mall-1");
        let pub_2 = publication("pub-2", "mall-2");

        let single = assemble_processable_publication_delivery_contexts(
            vec![first.clone()],
            vec![rev_1.clone()],
            vec![pub_1.clone()],
        );
        assert_eq!(single.len(), 1);
        assert_eq!(single[0].delivery.base.id, "del-1");
        assert_eq!(
            single[0].revision.as_ref().map(|row| row.base.id.as_str()),
            Some("rev-1")
        );
        assert_eq!(
            single[0].publication.as_ref().map(|row| row.base.id.as_str()),
            Some("pub-1")
        );

        let rows = assemble_processable_publication_delivery_contexts(
            vec![first, second, missing_revision, missing_publication, shared],
            vec![rev_1, rev_2, rev_orphan],
            vec![pub_1, pub_2],
        );
        let ids: Vec<_> = rows.iter().map(|row| row.delivery.base.id.as_str()).collect();
        assert_eq!(ids, vec!["del-1", "del-2", "del-3", "del-4", "del-5"]);
        assert!(rows[2].revision.is_none());
        assert!(rows[2].publication.is_none());
        assert_eq!(
            rows[3].revision.as_ref().map(|row| row.base.id.as_str()),
            Some("rev-orphan")
        );
        assert!(rows[3].publication.is_none());
        assert_eq!(
            rows[4].revision.as_ref().map(|row| row.base.id.as_str()),
            Some("rev-1")
        );
        assert_eq!(
            rows[4].publication.as_ref().map(|row| row.base.id.as_str()),
            Some("pub-1")
        );
    }
}
