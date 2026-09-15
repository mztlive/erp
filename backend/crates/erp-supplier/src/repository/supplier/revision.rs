use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SupplierAccountId;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Error, Executor, Pagination, QueryFilter, Result, mongo_ops};
use serde::Deserialize;

use super::{
    SUPPLIER_CAPABILITY_REVISIONS, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS, SUPPLIER_QUALIFICATION_REVISIONS,
    SUPPLIER_RATING_REVISIONS, SupplierRepository,
};
use crate::entity::supplier::{
    CapabilityCode, QualificationType, SupplierCommercialProfileRevision, SupplierRatingRevision,
};
use crate::repository::owned::SupplierCommercialProfileRevisionRepository;

/// 商务结算版本列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierCommercialProfileFilter {
    /// 供应商角色 ID；`None` 表示不筛选。
    pub supplier_id: Option<SupplierAccountId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（仓储白名单，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierCommercialProfileFilter {
    /// 转换为 MongoDB 查询条件（修订集合无软删除，过滤条件为空时仍显式
    /// 追加未删除过滤，与基类语义保持一致）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id.to_string());
        }
        filter
    }
}

impl Pagination for SupplierCommercialProfileFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> SupplierCommercialProfileRevisionRepository<'a> {
    /// 检索某供应商的商务版本历史（按 `revision_no` 升序，§6.2 历史查询）。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该供应商的全部商务版本（修订集合追加式写入，无软删除）。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revision_history(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        self.find_many_sorted(
            doc! { "supplier_id": supplier_id.to_string() },
            doc! { "revision_no": 1 },
            executor,
        )
        .await
    }
}

/// 修订号最小投影行。
#[derive(Debug, Deserialize)]
struct RevisionNoRow {
    revision_no: u32,
}

/// 查询当前最大修订号并返回下一号。
///
/// # 参数
/// * `db` - 目标 MongoDB 数据库
/// * `collection_name` - 追加式修订集合名称
/// * `filter` - 修订序列的稳定业务身份条件
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 无历史时返回 `1`，否则返回当前最大修订号加一。
///
/// # 错误
/// 当 MongoDB 查询失败、反序列化失败或修订号达到 `u32::MAX` 时返回错误。
async fn next_revision_no(
    db: &Database,
    collection_name: &str,
    mut filter: Document,
    executor: &mut dyn Executor,
) -> Result<u32> {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder()
        .sort(doc! { "revision_no": -1 })
        .limit(1)
        .projection(doc! { "revision_no": 1, "_id": 0 })
        .build();
    let rows =
        mongo_ops::find_many(&db.collection::<RevisionNoRow>(collection_name), filter, options, executor)
            .await?;
    rows.into_iter()
        .next()
        .map(|row| row.revision_no)
        .unwrap_or(0)
        .checked_add(1)
        .ok_or(Error::EntityMetadataOutOfRange("supplier revision number"))
}

impl<'a> SupplierRepository<'a> {
    /// 按修订号倒序读取供应商商务资料历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新版本优先的商务资料修订。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_commercial_profiles_latest_first(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        SupplierCommercialProfileRevisionRepository::new(self.db, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": -1 },
                executor,
            )
            .await
    }

    /// 按修订 ID 集合批量读取商务资料版本。
    ///
    /// # 参数
    /// * `revision_ids` - 商务资料修订 ID 集合；为空时直接返回空集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配的商务资料版本。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_commercial_profiles_by_ids(
        &self,
        revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierCommercialProfileRevision>> {
        if revision_ids.is_empty() {
            return Ok(Vec::new());
        }
        SupplierCommercialProfileRevisionRepository::new(self.db, SUPPLIER_COMMERCIAL_PROFILE_REVISIONS)
            .find_many(doc! { "id": { "$in": revision_ids } }, executor)
            .await
    }

    /// 按修订号倒序读取供应商评级历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新评级优先的修订历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_ratings_latest_first(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRatingRevision>> {
        persistence_core::Repository::new(self.db, SUPPLIER_RATING_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": -1 },
                executor,
            )
            .await
    }

    /// 按修订号升序读取供应商评级历史。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最早评级优先的修订历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn list_rating_history(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierRatingRevision>> {
        persistence_core::Repository::new(self.db, SUPPLIER_RATING_REVISIONS)
            .find_many_sorted(
                doc! { "supplier_id": supplier_id.to_string() },
                doc! { "revision_no": 1 },
                executor,
            )
            .await
    }

    /// 返回下一商务资料修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_commercial_profile_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_COMMERCIAL_PROFILE_REVISIONS,
            doc! { "supplier_id": supplier_id.to_string() },
            executor,
        )
        .await
    }

    /// 返回下一能力修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `capability_code` - 稳定能力代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_capability_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        capability_code: CapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_CAPABILITY_REVISIONS,
            doc! {
                "supplier_id": supplier_id.to_string(),
                "capability_code": capability_code.as_str(),
            },
            executor,
        )
        .await
    }

    /// 返回下一资质修订序号。
    ///
    /// # 参数
    /// * `supplier_id` - 供应商角色 ID
    /// * `qualification_type` - 稳定资质类型
    /// * `certificate_no` - 稳定证书编号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 无历史时返回 `1`，否则返回当前最大修订号加一。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn next_qualification_revision_no(
        &self,
        supplier_id: &SupplierAccountId,
        qualification_type: QualificationType,
        certificate_no: &str,
        executor: &mut dyn Executor,
    ) -> Result<u32> {
        next_revision_no(
            self.db,
            SUPPLIER_QUALIFICATION_REVISIONS,
            doc! {
                "supplier_id": supplier_id.to_string(),
                "qualification_type": qualification_type.as_str(),
                "certificate_no": certificate_no,
            },
            executor,
        )
        .await
    }
}
