//! 域 D26 `publication` 仓储：product_publication(+_revision、_revision_media)、
//! product_publication_delivery。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `PublicationExt` 关联常量导入。
//!
//! 本域全部实体为正式对象（§4.5 不设业务软删除）；修订集合为不可变版本
//! （§4.4），只追加不覆盖，**不提供任何软删除方法**。
//!
//! 筛选/行类型定义在本文件，经 `PublicationExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::ids::{SkuId, SourceSystemId};
use entities::money::Amount;
use entities::publication::{
    ProductPublication, ProductPublicationDelivery, ProductPublicationRevision, ProductPublicationRevisionId,
    ProductPublicationRevisionMedia, ProductPublicationStatus, PublicationDeliveryStatus, SaleStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::PublicationExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `product_publication` 集合名（单一来源：`PublicationExt` 关联常量）。
const PRODUCT_PUBLICATIONS: &str = <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATIONS;
/// `product_publication_revision` 集合名（单一来源：`PublicationExt` 关联常量）。
const PRODUCT_PUBLICATION_REVISIONS: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISIONS;
/// `product_publication_revision_media` 集合名（单一来源：`PublicationExt` 关联常量）。
const PRODUCT_PUBLICATION_REVISION_MEDIA: &str =
    <mongodb::Database as PublicationExt>::PRODUCT_PUBLICATION_REVISION_MEDIA;

/// 发布列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRow {
    /// 实体主键。
    pub id: String,
    /// ERP SKU。
    pub sku_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 发布状态。
    pub status: ProductPublicationStatus,
    /// 当前商城生效版本。
    pub current_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductPublicationFilter {
    /// ERP SKU；`None` 表示不筛选。
    pub sku_id: Option<SkuId>,
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 发布状态；`None` 表示不筛选。
    pub status: Option<ProductPublicationStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductPublicationFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sku_id) = &self.sku_id {
            filter.insert("sku_id", sku_id.to_string());
        }
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for ProductPublicationFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductPublication> {
    /// 分页检索商品发布列表（投影查询）。
    ///
    /// 只返回 [`ProductPublicationRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛，禁止透传任意字段名。
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
    pub async fn search_product_publications(
        &self,
        filter: &ProductPublicationFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductPublicationRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_publication_projection())
            .build();
        let collection = self.collection().clone_with_type::<ProductPublicationRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「SKU + 目标商城」查找唯一稳定发布。
    ///
    /// 唯一性由 `uk_product_publications_sku_mall` 唯一索引保证
    /// （数据模型 §6.15）；本方法用于发布查询与幂等判定。
    ///
    /// # 参数
    /// * `sku_id` - ERP SKU
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的发布；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_sku_and_mall(
        &self,
        sku_id: &SkuId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublication>> {
        self.find_one(
            doc! {
                "sku_id": sku_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }
}

/// 发布修订列表投影行（Decimal128 金额原样投影，不做舍入换算）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属稳定发布。
    pub product_publication_id: String,
    /// 修订序号（同一发布内从 1 递增）。
    pub revision_no: u32,
    /// 商城展示名称快照。
    pub name: String,
    /// 上架状态。
    pub sale_status: SaleStatus,
    /// 含税销售价。
    pub sales_price_gross: Amount,
    /// 生效区间开始。
    pub valid_from: i64,
    /// 生效区间结束。
    pub valid_to: Option<i64>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl<'a> Repository<'a, ProductPublicationRevision> {
    /// 按「发布 + 修订序号」查找唯一发布修订。
    ///
    /// 唯一性由 `uk_product_publication_revisions_publication_revision` 唯一索引
    /// 保证（数据模型 §6.15）；修订不可变，只读入口。
    ///
    /// # 参数
    /// * `product_publication_id` - 所属稳定发布
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的发布修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_revision_by_no(
        &self,
        product_publication_id: &entities::ids::ProductPublicationId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationRevision>> {
        self.find_one(
            doc! {
                "product_publication_id": product_publication_id.to_string(),
                "revision_no": revision_no,
            },
            executor,
        )
        .await
    }

    /// 列出指定发布的全部分区版本（投影查询，修订号降序）。
    ///
    /// 只返回 [`ProductPublicationRevisionRow`] 所需的列表字段，不加载整文档；
    /// 修订为不可变版本（§4.4），本方法只读，不提供任何删除入口。
    ///
    /// # 参数
    /// * `product_publication_id` - 所属稳定发布
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `revision_no` 降序的投影行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revisions_by_publication(
        &self,
        product_publication_id: &entities::ids::ProductPublicationId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductPublicationRevisionRow>> {
        let options = FindOptions::builder()
            .sort(doc! { "revision_no": -1 })
            .projection(product_publication_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<ProductPublicationRevisionRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "product_publication_id": product_publication_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, ProductPublicationRevisionMedia> {
    /// 列出指定发布修订的受控媒体（按角色、展示顺序排序）。
    ///
    /// 媒体为发布版本的不可变受控行，本方法只读，不提供任何删除入口；
    /// `(product_publication_revision_id, media_role, sort_no)` 唯一由
    /// `uk_product_publication_revision_media_revision_role_sort` 唯一索引保证
    /// （数据模型 §6.15）。
    ///
    /// # 参数
    /// * `revision_id` - 所属发布修订
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `media_role`、`sort_no` 升序的媒体实体列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_media_by_revision(
        &self,
        revision_id: &ProductPublicationRevisionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<ProductPublicationRevisionMedia>> {
        self.find_many_sorted(
            doc! { "product_publication_revision_id": revision_id.to_string() },
            doc! { "media_role": 1, "sort_no": 1 },
            executor,
        )
        .await
    }
}

/// 发布投递列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ProductPublicationDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 所属发布修订。
    pub publication_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 投递状态。
    pub delivery_status: PublicationDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 商城确认版本。
    pub mall_version: Option<String>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 发布投递列表筛选条件。
#[derive(Debug, Clone)]
pub struct ProductPublicationDeliveryFilter {
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 投递状态；`None` 表示不筛选。
    pub delivery_status: Option<PublicationDeliveryStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for ProductPublicationDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(delivery_status) = self.delivery_status {
            filter.insert("delivery_status", delivery_status.as_str());
        }
        filter
    }
}

impl Pagination for ProductPublicationDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, ProductPublicationDelivery> {
    /// 分页检索发布投递记录（投影查询）。
    ///
    /// 只返回 [`ProductPublicationDeliveryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛。投递状态查询由
    /// `idx_product_publication_deliveries_status` 索引支撑（§6.15）。
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
    pub async fn search_product_publication_deliveries(
        &self,
        filter: &ProductPublicationDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<ProductPublicationDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(product_publication_delivery_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<ProductPublicationDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「发布修订 + 目标商城」查找唯一投递记录。
    ///
    /// # 参数
    /// * `revision_id` - 所属发布修订
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的投递记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_delivery_by_revision_and_mall(
        &self,
        revision_id: &ProductPublicationRevisionId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<ProductPublicationDelivery>> {
        self.find_one(
            doc! {
                "publication_revision_id": revision_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }
}

/// D26 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `PublicationExt::publication()` 访问。
pub struct PublicationRepository<'a> {
    db: &'a Database,
}

impl<'a> PublicationRepository<'a> {
    /// 创建域专用仓储。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 建立发布主表及其首个发布修订（跨集合多步骤写入）。
    ///
    /// 依次写入 `product_publications` 与 `product_publication_revisions`，
    /// 保证「稳定发布 + 首个不可变版本」原子可见（数据模型 §6.15）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有发布没有版本的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `publication` - 待写入的稳定发布
    /// * `revision` - 待写入的首个发布修订
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_publication_revision(
        &self,
        publication: &ProductPublication,
        revision: &ProductPublicationRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<ProductPublication>(PRODUCT_PUBLICATIONS),
            publication,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ProductPublicationRevision>(PRODUCT_PUBLICATION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立发布修订及其受控媒体（跨集合多步骤写入）。
    ///
    /// 依次写入 `product_publication_revisions` 与
    /// `product_publication_revision_media`，保证「不可变版本 + 媒体行」
    /// 原子可见（数据模型 §6.15：提交发布必须有至少一张主图）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下没有媒体（或只有部分媒体）的
    /// 发布版本；Service 必须通过事务传入执行器。
    ///
    /// # 参数
    /// * `revision` - 待写入的发布修订
    /// * `media` - 待写入的受控媒体行清单
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_revision_with_media(
        &self,
        revision: &ProductPublicationRevision,
        media: &[ProductPublicationRevisionMedia],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<ProductPublicationRevision>(PRODUCT_PUBLICATION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<ProductPublicationRevisionMedia>(PRODUCT_PUBLICATION_REVISION_MEDIA),
            media.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单收敛）。
///
/// # 参数
/// * `sort_by` - 排序字段；仅允许 `sku_id`/`updated_at`/`created_at`，
///   其余一律回退 `created_at` 降序
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    match sort_by {
        Some("sku_id") => doc! { "sku_id": direction },
        Some("updated_at") => doc! { "updated_at": direction },
        _ => doc! { "created_at": direction },
    }
}

/// 发布列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_projection() -> Document {
    doc! {
        "id": 1,
        "sku_id": 1,
        "target_mall_id": 1,
        "status": 1,
        "current_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发布修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_revision_projection() -> Document {
    doc! {
        "id": 1,
        "product_publication_id": 1,
        "revision_no": 1,
        "name": 1,
        "sale_status": 1,
        "sales_price_gross": 1,
        "valid_from": 1,
        "valid_to": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 发布投递列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn product_publication_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "publication_revision_id": 1,
        "target_mall_id": 1,
        "delivery_status": 1,
        "attempt_count": 1,
        "mall_version": 1,
        "error_code": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, ProductPublicationDeliveryFilter, ProductPublicationFilter, QueryFilter};
    use entities::ids::{SkuId, SourceSystemId};
    use entities::publication::{ProductPublicationStatus, PublicationDeliveryStatus};
    use mongodb::bson::doc;

    #[test]
    fn publication_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ProductPublicationFilter {
            sku_id: Some(SkuId::new("sku-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProductPublicationStatus::MallEffective),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sku_id").unwrap(), "sku-1");
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("status").unwrap(), "mall_effective");
    }

    #[test]
    fn delivery_filter_applies_optional_fields_and_deleted_filter() {
        let filter = ProductPublicationDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            delivery_status: Some(PublicationDeliveryStatus::PendingSend),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("delivery_status").unwrap(), "pending_send");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(sort_doc(Some("sku_id"), true), doc! { "sku_id": 1 });
        assert_eq!(
            sort_doc(Some("任意字段"), false),
            doc! { "created_at": -1 },
            "白名单外字段一律回退默认排序"
        );
    }
}
