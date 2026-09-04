//! 卡实例批量解析（INT-R06 仓储所有权）。
//!
//! CARD 来源的外部引用解析与 ID 重读只归属本模块；Service 复用单次返回
//! 映射，不再逐个查询。

use std::collections::{HashMap, HashSet};

use entities::card_instance::MallCardInstance;
use entities::ids::MallCardInstanceId;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;

use crate::executor::Executor;
use crate::repository::Repository;
use crate::{mongo_ops, Result};

/// 对字符串集合去重并保持首次出现顺序。
///
/// # 参数
/// * `values` - 待去重的字符串集合
///
/// # 返回
/// 返回去重后的字符串列表。
fn unique_strings(values: &[String]) -> Vec<String> {
    let mut seen = HashSet::new();
    let mut unique = Vec::new();
    for value in values {
        if seen.insert(value.clone()) {
            unique.push(value.clone());
        }
    }
    unique
}

/// 构造按商城与稳定引用集合批量解析卡实例的过滤文档（INT-R06）。
///
/// # 用途
/// 将稳定引用展开为 `$in` 精确匹配，供单次查询替代逐个解析；
/// 过滤文档纯构造，便于单元测试锁定软删除语义。
///
/// # 参数
/// * `mall_id` - 来源商城
/// * `refs` - 去重后的卡实例稳定引用集合
///
/// # 返回
/// 返回含商城限定、`$in` 匹配与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 只匹配未删除基线；引用不存在时无条目，由调用方保持缺失错误语义。
fn card_identity_batch_filter(mall_id: &str, refs: &[String]) -> Document {
    doc! {
        "mall_id": mall_id,
        "opaque_instance_ref": { "$in": refs },
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 构造按卡实例 ID 集合批量读取卡实例的过滤文档（INT-R06）。
///
/// # 用途
/// 将卡实例 ID 展开为 `$in` 精确匹配，供单次查询替代按 ID 重读；
/// 过滤文档纯构造，便于单元测试锁定软删除语义。
///
/// # 参数
/// * `ids` - 去重后的卡实例 ID 字符串集合
///
/// # 返回
/// 返回含 `$in` 匹配与软删除排除的过滤文档。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 已删除卡 ID 无条目（缺失语义），不向资金计划注入已删除卡的归属。
fn card_id_batch_filter(ids: &[String]) -> Document {
    doc! {
        "id": { "$in": ids },
        "deleted_at": entity_core::NOT_DELETED_TIMESTAMP_BSON,
    }
}

/// 按稳定引用为卡实例行建立映射（INT-R06 归组语义）。
///
/// # 用途
/// 将单次批量查询返回的行折叠为引用到卡实例的映射；引用不存在时无条目，
/// 由调用方保持缺失引用错误语义，本函数不伪造缺失项。
///
/// # 参数
/// * `rows` - 单次 `$in` 查询返回的卡实例行（任意顺序）
///
/// # 返回
/// 返回稳定引用到卡实例的映射；重复行按最后一次出现覆盖。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 纯函数，不访问 I/O；缺失引用无条目，多来源共用一卡时共享同一映射项。
fn index_by_opaque_ref(rows: Vec<MallCardInstance>) -> HashMap<String, MallCardInstance> {
    let mut mapped = HashMap::with_capacity(rows.len());
    for row in rows {
        mapped.insert(row.opaque_instance_ref.clone(), row);
    }
    mapped
}

/// 按卡实例 ID 为卡实例行建立映射（INT-R06 归组语义）。
///
/// # 用途
/// 将单次批量查询返回的行折叠为 ID 到卡实例的映射；缺失 ID 无条目。
///
/// # 参数
/// * `rows` - 单次 `$in` 查询返回的卡实例行（任意顺序）
///
/// # 返回
/// 返回卡实例 ID 字符串到卡实例的映射。
///
/// # 错误
/// 不返回错误。
///
/// # 关键约束
/// 纯函数，不访问 I/O；多个分摊共用一来源时复用同一映射项，不再重读。
fn index_by_base_id(rows: Vec<MallCardInstance>) -> HashMap<String, MallCardInstance> {
    let mut mapped = HashMap::with_capacity(rows.len());
    for row in rows {
        mapped.insert(row.base.id.clone(), row);
    }
    mapped
}

impl<'a> Repository<'a, MallCardInstance> {
    /// 按商城与稳定引用集合批量解析卡实例（INT-R06）。
    ///
    /// # 用途
    /// 以一次 `$in` 查询替代 CARD 来源逐个按外部引用解析。
    ///
    /// # 参数
    /// * `self` - 卡实例仓储
    /// * `mall_id` - 来源商城
    /// * `refs` - 卡实例稳定引用集合；为空时返回空映射，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回稳定引用到卡实例的映射；引用不存在时无条目，由调用方保持缺失错误语义。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 空输入、去重、缺项由本方法保证；软删除排除；不返回 Service DTO；不开事务。
    pub async fn list_by_identity_refs(
        &self,
        mall_id: &str,
        refs: &[String],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, MallCardInstance>> {
        if refs.is_empty() {
            return Ok(HashMap::new());
        }
        let unique = unique_strings(refs);
        let rows: Vec<MallCardInstance> = mongo_ops::find_many(
            &self.collection(),
            card_identity_batch_filter(mall_id, &unique),
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(index_by_opaque_ref(rows))
    }

    /// 按卡实例 ID 集合批量读取卡实例（INT-R06）。
    ///
    /// # 用途
    /// 以一次 `$in` 查询替代生成 allocation 时按 ID 重读卡实例。
    ///
    /// # 参数
    /// * `self` - 卡实例仓储
    /// * `card_ids` - 卡实例 ID 集合；为空时返回空映射，不访问数据库
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回卡实例 ID 字符串到卡实例的映射；缺失 ID 无条目。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    ///
    /// # 关键约束
    /// 空输入、去重、缺项由本方法保证；软删除排除；不开事务。
    pub async fn list_by_card_ids(
        &self,
        card_ids: &[MallCardInstanceId],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, MallCardInstance>> {
        if card_ids.is_empty() {
            return Ok(HashMap::new());
        }
        let mut seen = HashSet::new();
        let mut ids = Vec::new();
        for id in card_ids {
            if seen.insert(id.to_string()) {
                ids.push(id.to_string());
            }
        }
        let rows: Vec<MallCardInstance> = mongo_ops::find_many(
            &self.collection(),
            card_id_batch_filter(&ids),
            FindOptions::default(),
            executor,
        )
        .await?;
        Ok(index_by_base_id(rows))
    }
}

#[cfg(test)]
mod tests {
    use super::{
        card_id_batch_filter, card_identity_batch_filter, index_by_base_id, index_by_opaque_ref,
        unique_strings,
    };
    use entities::card_instance::{CardSourceType, MallCardInstance, MallCardInstanceData};
    use entities::common::time::Instant;
    use entities::ids::{ExternalIdentityMapId, MallCardInstanceId, SalesOrderId, SalesOrderRevisionId};
    use entities::money::Amount;
    use std::str::FromStr;

    /// 构造卡实例行夹具。
    ///
    /// # 参数
    /// * `id` - 卡实例主键
    /// * `opaque_ref` - 稳定引用
    ///
    /// # 返回
    /// 返回归属同一销售单的卡实例行。
    fn card_fixture(id: &str, opaque_ref: &str) -> MallCardInstance {
        MallCardInstance::new(
            MallCardInstanceId::new(id),
            MallCardInstanceData {
                mall_id: "mall-a".to_string(),
                opaque_instance_ref: opaque_ref.to_string(),
                origin_sales_order_source_identity_id: ExternalIdentityMapId::new(format!("eim-{id}")),
                origin_sales_order_id: SalesOrderId::new("so-1"),
                origin_sales_order_revision_id: SalesOrderRevisionId::new("sor-1"),
                source_baseline_version: None,
                initial_balance: Amount::from_str("100.00").unwrap(),
                baseline_at: Instant::from_unix_secs(1_700_000_000),
                source_type: CardSourceType::Realtime,
            },
        )
        .expect("卡实例夹具构造失败")
    }

    /// 重复引用折叠：去重后 `$in` 只含唯一元素，重复来源只查询一次。
    #[test]
    fn duplicate_refs_collapse_to_single_in_element() {
        let unique = unique_strings(&["ref-1".to_string(), "ref-1".to_string(), "ref-2".to_string()]);
        let filter = card_identity_batch_filter("mall-a", &unique);
        let matched = filter
            .get_document("opaque_instance_ref")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(matched.len(), 2);

        let single = unique_strings(&["ref-1".to_string(), "ref-1".to_string()]);
        let single_filter = card_identity_batch_filter("mall-a", &single);
        assert_eq!(
            single_filter
                .get_document("opaque_instance_ref")
                .unwrap()
                .get_array("$in")
                .unwrap()
                .len(),
            1
        );
    }

    /// 大批量去重：100 个引用（含重复）在查询前折叠为 50 个唯一引用并保序。
    #[test]
    fn large_batch_dedups_before_query() {
        let refs: Vec<String> = (0..100).map(|index| format!("ref-{}", index % 50)).collect();
        let unique = unique_strings(&refs);
        assert_eq!(unique.len(), 50);
        assert_eq!(unique[0], "ref-0".to_string());
        assert_eq!(unique[49], "ref-49".to_string());
        let filter = card_identity_batch_filter("mall-a", &unique);
        assert_eq!(
            filter
                .get_document("opaque_instance_ref")
                .unwrap()
                .get_array("$in")
                .unwrap()
                .len(),
            50
        );
    }

    /// 缺失引用无映射条目：归组不伪造缺失项，调用方保持缺失引用错误语义。
    #[test]
    fn missing_ref_yields_no_map_entry() {
        let mapped = index_by_opaque_ref(vec![card_fixture("card-1", "ref-1")]);
        assert_eq!(mapped.len(), 1);
        assert!(mapped.get("ref-1").is_some());
        assert!(
            mapped.get("ref-missing").is_none(),
            "缺失引用不得伪造映射项，Service 保持 PendingAttribution 与缺失错误语义"
        );
    }

    /// 共用来源复用：同一卡实例只占一个映射项，多次查找命中同一归属。
    #[test]
    fn shared_source_reuses_single_map_lookup() {
        let mapped = index_by_base_id(vec![card_fixture("card-1", "ref-1")]);
        assert_eq!(mapped.len(), 1);
        let first = mapped.get("card-1").expect("卡 ID 必须命中");
        let second = mapped.get("card-1").expect("重复查找必须命中同一项");
        assert_eq!(first.origin_sales_order_id, second.origin_sales_order_id);
        assert!(mapped.get("card-missing").is_none());
    }

    /// 引用去重：重复引用只查询一次，空输入直接返回且保序。
    #[test]
    fn identity_refs_deduplicate_before_query() {
        assert_eq!(
            unique_strings(&["ref-1".to_string(), "ref-1".to_string(), "ref-2".to_string()]),
            vec!["ref-1".to_string(), "ref-2".to_string()]
        );
        assert!(unique_strings(&[]).is_empty());
    }

    /// 引用批量过滤：限定商城、`$in` 匹配且排除软删除基线。
    #[test]
    fn identity_batch_filter_limits_mall_and_excludes_deleted() {
        let refs = vec!["ref-1".to_string()];
        let filter = card_identity_batch_filter("mall-a", &refs);
        assert_eq!(filter.get_str("mall_id").unwrap(), "mall-a");
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let matched = filter
            .get_document("opaque_instance_ref")
            .unwrap()
            .get_array("$in")
            .unwrap();
        assert_eq!(matched.len(), 1);
    }

    /// ID 批量过滤：`$in` 匹配且排除软删除，已删除卡 ID 无条目。
    #[test]
    fn id_batch_filter_excludes_deleted_cards() {
        let ids = vec!["card-1".to_string(), "card-2".to_string()];
        let filter = card_id_batch_filter(&ids);
        assert_eq!(filter.get_i64("deleted_at").unwrap(), 0);
        let matched = filter.get_document("id").unwrap().get_array("$in").unwrap();
        assert_eq!(matched.len(), 2);
    }

    /// 空输入直接返回空映射，不访问数据库（0 条验收维度，无 I/O 单测）。
    ///
    /// 懒构造的客户端句柄不建立连接；空集合在触及执行器前返回，
    /// 因此本用例无需 MongoDB 即可在 `--lib` 门禁内运行。
    #[tokio::test]
    async fn empty_inputs_return_empty_without_database_access() {
        use crate::executor::NoTransaction;
        use crate::repository::extensions::CardInstanceExt;

        let options = mongodb::options::ClientOptions::parse("mongodb://127.0.0.1:27017")
            .await
            .expect("测试客户端选项解析失败");
        let client = mongodb::Client::with_options(options).expect("测试客户端构造失败");
        let db = client.database("int_r06_unit");
        let repository = db.mall_card_instances();
        assert!(repository
            .list_by_identity_refs("mall-a", &[], &mut NoTransaction)
            .await
            .expect("空引用集合必须成功")
            .is_empty());
        assert!(repository
            .list_by_card_ids(&[], &mut NoTransaction)
            .await
            .expect("空 ID 集合必须成功")
            .is_empty());
    }
}
