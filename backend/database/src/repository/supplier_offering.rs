//! 域 D24 供应商供给仓储。
//!
//! 本域只持久化供给稳定身份、不可变商业条款修订、实时可供投影和幂等命令。
//! 公司商品/SKU 由 D10 持有，不建立供应商商品主档或映射集合。

use entities::ids::{SkuId, SupplierAccountId, SupplierOfferingId};
use entities::supplier_offering::{
    AvailabilityStatus, OfferingSourceType, OfferingStatus, SupplierOffering, SupplierOfferingAvailability,
    SupplierOfferingCommand, SupplierOfferingRevision,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Bson, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SupplierOfferingExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

const OFFERINGS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERINGS;
const OFFERING_REVISIONS: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_REVISIONS;
const OFFERING_AVAILABILITIES: &str = <Database as SupplierOfferingExt>::SUPPLIER_OFFERING_AVAILABILITIES;
const OFFERING_SORT_FIELDS: &[&str] = &["created_at", "status", "supplier_sku_code"];

impl<'a> Repository<'a, SupplierOfferingCommand> {
    /// 按客户端幂等键查询已成功命令。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端幂等键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回已提交命令；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_idempotency_key(
        &self,
        idempotency_key: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingCommand>> {
        self.find_one(doc! { "idempotency_key": idempotency_key }, executor)
            .await
    }
}

/// 供给列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierOfferingRow {
    /// 供给主键。
    pub id: String,
    /// 公司 SKU。
    pub sku_id: SkuId,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 供应商侧商品编码。
    pub supplier_product_code: Option<String>,
    /// 供应商侧订货 SKU 编码。
    pub supplier_sku_code: String,
    /// 登记来源。
    pub source_type: OfferingSourceType,
    /// API 来源连接。
    pub source_connection_id: Option<entities::ids::SupplierApiConnectionId>,
    /// 供给关系状态。
    pub status: OfferingStatus,
    /// 当前商业条款修订。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间。
    pub created_at: u64,
}

/// 供给列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierOfferingFilter {
    /// 允许命中的供给主键；用于关联当前可供投影后的分页前筛选。
    pub offering_ids: Option<Vec<SupplierOfferingId>>,
    /// 公司 SKU。
    pub sku_id: Option<SkuId>,
    /// 供应商。
    pub supplier_id: Option<SupplierAccountId>,
    /// 供给关系状态。
    pub status: Option<OfferingStatus>,
    /// 登记来源。
    pub source_type: Option<OfferingSourceType>,
    /// 供应商 SKU 编码模糊查询。
    pub supplier_sku_code: Option<String>,
    /// 关键字命中的公司 SKU 主键；与 `supplier_sku_code` 做 OR（公司 SKU 编号/名称）。
    pub keyword_sku_ids: Option<Vec<SkuId>>,
    /// 按 SPU 编号 / SKU 编号命中的公司 SKU 主键（AND 条件；空集合表示无匹配）。
    pub sku_ids: Option<Vec<SkuId>>,
    /// 页码。
    pub page: u64,
    /// 每页数量。
    pub page_size: u32,
    /// 排序字段。
    pub sort_by: Option<String>,
    /// 是否升序。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierOfferingFilter {
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(offering_ids) = &self.offering_ids {
            filter.extend(in_filter("id", offering_ids.iter().map(ToString::to_string)));
        }
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(sku_ids) = &self.sku_ids {
            filter.extend(in_filter("sku_id", sku_ids.iter().map(ToString::to_string)));
        }
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        match (self.supplier_sku_code.as_deref(), self.keyword_sku_ids.as_ref()) {
            (Some(code), Some(sku_ids)) => {
                let mut code_filter = Document::new();
                insert_literal_regex_filter(&mut code_filter, "supplier_sku_code", Some(code));
                let mut or_branches = vec![code_filter];
                if !sku_ids.is_empty() {
                    or_branches.push(doc! {
                        "sku_id": {
                            "$in": sku_ids.iter().map(ToString::to_string).collect::<Vec<_>>()
                        }
                    });
                }
                filter.insert("$or", or_branches);
            }
            (Some(code), None) => {
                insert_literal_regex_filter(&mut filter, "supplier_sku_code", Some(code));
            }
            (None, Some(sku_ids)) => {
                filter.extend(in_filter("sku_id", sku_ids.iter().map(ToString::to_string)));
            }
            (None, None) => {}
        }
        filter
    }
}

impl Pagination for SupplierOfferingFilter {
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierOffering> {
    /// 分页检索供给列表。
    ///
    /// # 参数
    /// * `filter` - 筛选与分页条件
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前页投影及总数。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn search_supplier_offerings(
        &self,
        filter: &SupplierOfferingFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierOfferingRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                OFFERING_SORT_FIELDS,
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_offering_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierOfferingRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;
        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按公司 SKU 批量取回供给。
    ///
    /// # 参数
    /// * `sku_ids` - 公司 SKU 集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配的供给身份。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_sku_ids(
        &self,
        sku_ids: &[SkuId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOffering>> {
        if sku_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter("sku_id", sku_ids.iter().map(ToString::to_string)),
            executor,
        )
        .await
    }

    /// 按供应商与供应商 SKU 编码查询唯一供给身份。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商
    /// * `supplier_sku_code` - 供应商订货编码
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回匹配供给；不存在时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_supplier_identity(
        &self,
        supplier_id: &SupplierAccountId,
        supplier_sku_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOffering>> {
        self.find_one(
            doc! {
                "supplier_id": supplier_id.to_string(),
                "supplier_sku_code": supplier_sku_code,
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierOfferingRevision> {
    /// 批量取回多个供给的全部商业条款修订。
    ///
    /// # 参数
    /// * `offering_ids` - 供给主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回全部匹配修订。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_revisions_by_offering_ids(
        &self,
        offering_ids: &[SupplierOfferingId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingRevision>> {
        if offering_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_offering_id",
                offering_ids.iter().map(ToString::to_string),
            ),
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierOfferingAvailability> {
    /// 按当前可供状态查询供给主键。
    ///
    /// # 参数
    /// * `status` - 当前可供状态
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回状态匹配的供给主键集合。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_offering_ids_by_status(
        &self,
        status: AvailabilityStatus,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingId>> {
        let rows = self
            .find_many(doc! { "availability_status": status.as_str() }, executor)
            .await?;
        Ok(rows.into_iter().map(|row| row.supplier_offering_id).collect())
    }

    /// 按供给主键查询实时可供投影。
    ///
    /// # 参数
    /// * `offering_id` - 供给主键
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回当前投影；尚未同步时返回 `None`。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_offering_id(
        &self,
        offering_id: &SupplierOfferingId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierOfferingAvailability>> {
        self.find_one(doc! { "supplier_offering_id": offering_id.to_string() }, executor)
            .await
    }

    /// 批量取回供给的实时可供投影。
    ///
    /// # 参数
    /// * `offering_ids` - 供给主键集合
    /// * `executor` - 数据访问执行器
    ///
    /// # 返回
    /// 返回已存在的可供投影。
    ///
    /// # 错误
    /// MongoDB 查询失败时返回错误。
    pub async fn find_by_offering_ids(
        &self,
        offering_ids: &[SupplierOfferingId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierOfferingAvailability>> {
        if offering_ids.is_empty() {
            return Ok(Vec::new());
        }
        self.find_many(
            in_filter(
                "supplier_offering_id",
                offering_ids.iter().map(ToString::to_string),
            ),
            executor,
        )
        .await
    }
}

/// 供给聚合的跨集合事务仓储。
pub struct SupplierOfferingRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierOfferingRepository<'a> {
    /// 创建供给聚合仓储。
    ///
    /// # 参数
    /// * `db` - 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 原子创建供给、首版商业条款与初始可供投影。
    ///
    /// # 参数
    /// * `offering` - 已指向首版修订的供给
    /// * `revision` - 首版商业条款
    /// * `availability` - 初始实时可供事实
    /// * `executor` - 事务执行器
    ///
    /// # 返回
    /// 三项写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一约束冲突或 MongoDB 写入失败时返回错误。
    pub async fn create_with_revision_and_availability(
        &self,
        offering: &SupplierOffering,
        revision: &SupplierOfferingRevision,
        availability: &SupplierOfferingAvailability,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOffering>(OFFERINGS),
            offering,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOfferingRevision>(OFFERING_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierOfferingAvailability>(OFFERING_AVAILABILITIES),
            availability,
            executor,
        )
        .await
    }

    /// 追加商业条款修订并乐观锁推进供给当前版本。
    ///
    /// # 参数
    /// * `offering` - 已更新当前修订指针和状态的供给
    /// * `revision` - 新商业条款修订
    /// * `executor` - 事务执行器
    ///
    /// # 返回
    /// 两项写入成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 修订号冲突、乐观锁冲突或 MongoDB 失败时返回错误。
    pub async fn append_revision(
        &self,
        offering: &mut SupplierOffering,
        revision: &SupplierOfferingRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierOfferingRevision>(OFFERING_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Repository::new(self.db, OFFERINGS)
            .update(offering, executor)
            .await
    }
}

fn sort_doc(sort_by: Option<&str>, whitelist: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| whitelist.contains(field))
        .unwrap_or("created_at");
    doc! { field: if sort_ascending { 1 } else { -1 } }
}

fn in_filter(field: &str, values: impl IntoIterator<Item = String>) -> Document {
    let values = values.into_iter().map(Bson::String).collect::<Vec<_>>();
    doc! { field: { "$in": values } }
}

fn supplier_offering_projection() -> Document {
    doc! {
        "_id": 0,
        "id": 1,
        "sku_id": 1,
        "supplier_id": 1,
        "supplier_product_code": 1,
        "supplier_sku_code": 1,
        "source_type": 1,
        "source_connection_id": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use mongodb::bson::doc;

    use super::{sort_doc, SupplierOfferingFilter};
    use crate::repository::QueryFilter;

    #[test]
    fn offering_filter_uses_direct_supplier_identity() {
        let filter = SupplierOfferingFilter {
            offering_ids: None,
            sku_id: None,
            supplier_id: None,
            status: None,
            source_type: None,
            supplier_sku_code: Some("SKU-1".to_string()),
            keyword_sku_ids: None,
            sku_ids: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
        .to_doc();
        assert!(filter.contains_key("supplier_sku_code"));
    }

    #[test]
    fn offering_filter_combines_source_and_candidate_ids() {
        let filter = SupplierOfferingFilter {
            offering_ids: Some(vec![entities::ids::SupplierOfferingId::new("offering-1")]),
            sku_id: None,
            supplier_id: None,
            status: None,
            source_type: Some(entities::supplier_offering::OfferingSourceType::Excel),
            supplier_sku_code: None,
            keyword_sku_ids: None,
            sku_ids: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
        .to_doc();
        assert_eq!(filter.get_str("source_type"), Ok("EXCEL"));
        assert_eq!(filter.get_document("id"), Ok(&doc! { "$in": ["offering-1"] }));
    }

    #[test]
    fn offering_filter_ors_supplier_code_with_keyword_sku_ids() {
        let filter = SupplierOfferingFilter {
            offering_ids: None,
            sku_id: None,
            supplier_id: None,
            status: None,
            source_type: None,
            supplier_sku_code: Some("SUP-1".to_string()),
            keyword_sku_ids: Some(vec![entities::ids::SkuId::new("sku-1")]),
            sku_ids: None,
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        }
        .to_doc();
        assert!(filter.contains_key("$or"));
    }

    #[test]
    fn sort_is_whitelisted() {
        assert_eq!(
            sort_doc(Some("supplier_sku_code"), &["supplier_sku_code"], true),
            doc! { "supplier_sku_code": 1 }
        );
        assert_eq!(
            sort_doc(Some("unsafe"), &["supplier_sku_code"], false),
            doc! { "created_at": -1 }
        );
    }
}
