//! SKU 配送覆盖投影的持久化：变更扫描、待重算标记、保存与地区匹配。

use std::collections::BTreeSet;

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::change_scan::ChangeWindow;
use erp_core::common::time::BusinessDate;
use erp_core::delivery_coverage::{DeliveryCoverageQuery, DeliveryExtent};
use mongodb::bson::{Bson, Document, doc, serialize_to_bson};
use mongodb::options::FindOptions;
use mongodb::{Collection, Database};
use persistence_core::dirty_marks::{self, DirtyMark};
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use crate::entity::supplier_offering::{SkuDeliveryCoverage, SkuDeliveryCoverageState};
use crate::repository::SupplierOfferingExt;

const COVERAGES: &str = <Database as SupplierOfferingExt>::SKU_DELIVERY_COVERAGES;
const STATE: &str = <Database as SupplierOfferingExt>::SKU_DELIVERY_COVERAGE_STATE;
const OFFERINGS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
const AVAILABILITIES: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;
const TEMPLATES: &str = <Database as SupplierOfferingExt>::SUPPLIER_DELIVERY_TEMPLATES;

#[derive(Deserialize)]
struct IdRow {
    #[serde(rename = "_id")]
    id: String,
}

#[derive(Deserialize)]
struct SkuRow {
    sku_id: String,
}

#[derive(Deserialize)]
struct OfferingRow {
    supplier_offering_id: String,
}

#[derive(Deserialize)]
struct SupplierRow {
    supplier_id: String,
}

/// SKU 配送覆盖集合访问器。
pub struct SkuDeliveryCoverageStore<'a> {
    db: &'a Database,
}

impl<'a> SkuDeliveryCoverageStore<'a> {
    /// 绑定数据库。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    ///
    /// # 返回
    /// 返回未执行 I/O 的访问器。
    ///
    /// # 错误
    /// 无。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    fn coverages(&self) -> Collection<SkuDeliveryCoverage> {
        self.db.collection(COVERAGES)
    }

    fn documents(&self) -> Collection<Document> {
        self.db.collection(COVERAGES)
    }

    fn ids(&self) -> Collection<IdRow> {
        self.db.collection(COVERAGES)
    }

    /// 标记 SKU 待重算；尚无覆盖记录的 SKU 插入待重算占位。
    ///
    /// # 参数
    /// * `sku_ids` - 公司 SKU，允许重复
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 标记完成时无返回值；空输入不写入。
    ///
    /// # 错误
    /// MongoDB 读写失败时返回错误。
    pub async fn mark_skus(&self, sku_ids: &[String], executor: &mut dyn Executor) -> Result<()> {
        let pending = computed_fields(&SkuDeliveryCoverage::pending(String::new()))?;
        dirty_marks::mark_ids(&self.documents(), sku_ids, |_| pending.clone(), executor).await
    }

    /// 标记引用了任一供应商有效供给的 SKU 待重算。
    ///
    /// # 参数
    /// * `supplier_ids` - 模板发生变化的供应商
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回被标记的记录数；空输入不写入。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn mark_suppliers(&self, supplier_ids: &[String], executor: &mut dyn Executor) -> Result<u64> {
        if supplier_ids.is_empty() {
            return Ok(0);
        }
        let filter = doc! { "supplier_ids": { "$in": supplier_ids } };
        dirty_marks::mark_matching(&self.documents(), filter, executor).await
    }

    /// 标记名下供给已到按日期生效或失效日的 SKU 待重算。
    ///
    /// # 参数
    /// * `today` - 当前业务日期
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回被标记的记录数。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn mark_due(&self, today: BusinessDate, executor: &mut dyn Executor) -> Result<u64> {
        let filter = doc! { "dirty": false, "next_change_date": { "$lte": today.to_string() } };
        dirty_marks::mark_matching(&self.documents(), filter, executor).await
    }

    /// 读取一批待重算的 SKU。
    ///
    /// # 参数
    /// * `limit` - 本批上限
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回 SKU 与读取时的标记序号。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn dirty_batch(&self, limit: i64, executor: &mut dyn Executor) -> Result<Vec<DirtyMark>> {
        dirty_marks::dirty_batch(&self.documents(), limit, executor).await
    }

    /// 保存计算结果；计算期间又被标记时不清除待重算。
    ///
    /// # 参数
    /// * `record` - 计算结果
    /// * `dirty_token` - 开始计算前读取的标记序号
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 标记序号仍一致并已保存时为真；期间又被标记时为假，留待下一轮。
    ///
    /// # 错误
    /// 序列化或 MongoDB 写入失败时返回错误。
    pub async fn save(
        &self,
        record: &SkuDeliveryCoverage,
        dirty_token: i64,
        executor: &mut dyn Executor,
    ) -> Result<bool> {
        dirty_marks::save_clean(
            &self.documents(),
            &record.sku_id,
            dirty_token,
            computed_fields(record)?,
            executor,
        )
        .await
    }

    /// 批量读取覆盖记录。
    ///
    /// # 参数
    /// * `sku_ids` - 公司 SKU
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回存在的记录；空输入不查询。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SkuDeliveryCoverage>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        let filter = doc! { "_id": { "$in": sku_ids } };
        mongo_ops::find_many(&self.coverages(), filter, FindOptions::default(), executor).await
    }

    /// 返回满足地区筛选的全部 SKU。
    ///
    /// # 参数
    /// * `query` - 地区筛选条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回命中的 SKU，按多键索引匹配，不逐个计算。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn matching_sku_ids(
        &self,
        query: &DeliveryCoverageQuery,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let rows = mongo_ops::find_many(&self.ids(), match_filter(query), id_projection(), executor).await?;
        Ok(rows.into_iter().map(|row| row.id).collect())
    }

    /// 读取刷新进度。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 尚未初始化时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询或反序列化失败时返回错误。
    pub async fn state(&self, executor: &mut dyn Executor) -> Result<Option<SkuDeliveryCoverageState>> {
        let collection = self.db.collection::<SkuDeliveryCoverageState>(STATE);
        mongo_ops::find_one(&collection, doc! { "_id": SkuDeliveryCoverageState::ID }, executor).await
    }

    /// 推进刷新进度；进度只前进不后退，口径版本取较大值。
    ///
    /// # 参数
    /// * `state` - 新进度
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 写入成功时无返回值。
    ///
    /// # 错误
    /// MongoDB 写入失败时返回错误。
    pub async fn advance_state(
        &self,
        state: &SkuDeliveryCoverageState,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let update = doc! { "$max": {
            "version": state.version,
            "fresh_watermark": state.watermarks.fresh_watermark,
            "settled_watermark": state.watermarks.settled_watermark,
        } };
        let collection = self.db.collection::<SkuDeliveryCoverageState>(STATE);
        mongo_ops::update_one(&collection, doc! { "_id": &state.id }, update, true, executor).await?;
        Ok(())
    }

    /// 返回时间窗内供给或可供状态有变化的 SKU，含已删除供给。
    ///
    /// # 参数
    /// * `window` - 变更时间窗
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重后的 SKU。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn changed_sku_ids(
        &self,
        window: ChangeWindow,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let offerings = self.db.collection::<SkuRow>(OFFERINGS);
        let sku_projection = projection(doc! { "_id": 0, "sku_id": 1 });
        let mut sku_ids =
            mongo_ops::find_many(&offerings, window_filter(window), sku_projection.clone(), executor)
                .await?
                .into_iter()
                .map(|row| row.sku_id)
                .collect::<BTreeSet<_>>();
        let availabilities = self.db.collection::<OfferingRow>(AVAILABILITIES);
        let offering_projection = projection(doc! { "_id": 0, "supplier_offering_id": 1 });
        let offering_ids =
            mongo_ops::find_many(&availabilities, window_filter(window), offering_projection, executor)
                .await?
                .into_iter()
                .map(|row| row.supplier_offering_id)
                .collect::<BTreeSet<_>>();
        if !offering_ids.is_empty() {
            let filter = doc! { "id": { "$in": offering_ids.into_iter().collect::<Vec<_>>() } };
            let rows = mongo_ops::find_many(&offerings, filter, sku_projection, executor).await?;
            sku_ids.extend(rows.into_iter().map(|row| row.sku_id));
        }
        Ok(sku_ids.into_iter().collect())
    }

    /// 返回时间窗内配送模板有变化（含删除、默认切换）的供应商。
    ///
    /// # 参数
    /// * `window` - 变更时间窗
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重后的供应商。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn changed_template_supplier_ids(
        &self,
        window: ChangeWindow,
        executor: &mut dyn Executor,
    ) -> Result<Vec<String>> {
        let templates = self.db.collection::<SupplierRow>(TEMPLATES);
        let options = projection(doc! { "_id": 0, "supplier_id": 1 });
        let rows = mongo_ops::find_many(&templates, window_filter(window), options, executor).await?;
        Ok(rows.into_iter().map(|row| row.supplier_id).collect::<BTreeSet<_>>().into_iter().collect())
    }

    /// 返回存在未删除供给的全部 SKU，供全量重建。
    ///
    /// # 参数
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回去重后的 SKU。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn offered_sku_ids(&self, executor: &mut dyn Executor) -> Result<Vec<String>> {
        let offerings = self.db.collection::<SkuRow>(OFFERINGS);
        let filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        let options = projection(doc! { "_id": 0, "sku_id": 1 });
        let rows = mongo_ops::find_many(&offerings, filter, options, executor).await?;
        Ok(rows.into_iter().map(|row| row.sku_id).collect::<BTreeSet<_>>().into_iter().collect())
    }
}

fn id_projection() -> FindOptions {
    projection(doc! { "_id": 1 })
}

fn projection(fields: Document) -> FindOptions {
    FindOptions::builder().projection(fields).build()
}

fn window_filter(window: ChangeWindow) -> Document {
    doc! { "updated_at": { "$gte": window.from, "$lt": window.to } }
}

/// 地区筛选条件：整片模式按所选档位整片集合匹配地区自身、上级或全国标记，部分模式精确匹配。
fn match_filter(query: &DeliveryCoverageQuery) -> Document {
    let tier = query.tier_field();
    match query.extent {
        DeliveryExtent::Full => doc! { (format!("{tier}.full")): { "$in": query.full_match_codes() } },
        DeliveryExtent::Partial => doc! { (format!("{tier}.partial")): query.region.as_str() },
    }
}

/// 计算结果字段；不含主键与标记字段，占位插入与保存共用。
fn computed_fields(record: &SkuDeliveryCoverage) -> Result<Document> {
    Ok(doc! {
        "included": serialize_to_bson(&record.coverage.included)?,
        "reachable": serialize_to_bson(&record.coverage.reachable)?,
        "supplier_ids": &record.supplier_ids,
        "next_change_date": record.next_change_date.map(|date| Bson::String(date.to_string())),
        "computed_at": record.computed_at,
    })
}

#[cfg(test)]
mod tests {
    use erp_core::change_scan::ChangeWindow;
    use erp_core::delivery_coverage::{DeliveryCoverageQuery, DeliveryExtent, NATIONWIDE_REGION_CODE};
    use erp_core::region::RegionCode;
    use mongodb::bson::doc;

    use super::{computed_fields, match_filter, window_filter};
    use crate::entity::supplier_offering::SkuDeliveryCoverage;

    fn query(region: &str, extent: DeliveryExtent, include_freight_extra: bool) -> DeliveryCoverageQuery {
        DeliveryCoverageQuery { region: RegionCode::parse(region).unwrap(), extent, include_freight_extra }
    }

    #[test]
    fn full_match_uses_region_ancestors_and_nationwide_on_selected_tier() {
        let filter = match_filter(&query("440305", DeliveryExtent::Full, false));
        assert_eq!(
            filter,
            doc! { "included.full": { "$in": ["440305", "440300", "440000", NATIONWIDE_REGION_CODE] } }
        );
        let filter = match_filter(&query("440000", DeliveryExtent::Full, true));
        assert_eq!(filter, doc! { "reachable.full": { "$in": ["440000", NATIONWIDE_REGION_CODE] } });
    }

    #[test]
    fn partial_match_is_exact_region() {
        assert_eq!(
            match_filter(&query("440300", DeliveryExtent::Partial, true)),
            doc! { "reachable.partial": "440300" }
        );
    }

    #[test]
    fn change_window_includes_start_and_excludes_end() {
        assert_eq!(
            window_filter(ChangeWindow { from: 10, to: 20 }),
            doc! { "updated_at": { "$gte": 10_i64, "$lt": 20_i64 } }
        );
    }

    #[test]
    fn computed_fields_leave_identity_and_marks_to_the_marker() {
        let pending = SkuDeliveryCoverage::pending("sku-1".into());
        let fields = computed_fields(&pending).unwrap();
        for key in ["_id", "dirty", "dirty_token"] {
            assert!(!fields.contains_key(key), "{key}");
        }
        assert!(fields.get("next_change_date").unwrap().as_null().is_some());
        assert_eq!(fields.get_array("supplier_ids").unwrap().len(), 0);
    }
}
