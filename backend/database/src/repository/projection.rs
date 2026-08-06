//! 域 D27 `projection` 仓储：sales_order_projection(+_revision、_delivery)。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `ProjectionExt` 关联常量导入。
//!
//! 投影版本为不可变执行指令版本（§4.4），**只追加不覆盖，不提供任何软删除
//! 方法**；投影稳定身份按 `(sales_order_id, target_mall_id)` 唯一（§6.16）。
//!
//! 筛选/行类型定义在本文件，经 `ProjectionExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::ids::{SalesOrderId, SalesOrderProjectionRevisionId, SourceSystemId};
use entities::money::Amount;
use entities::projection::{
    CardForm, ProjectionDeliveryStatus, ProjectionSource, SalesOrderProjection, SalesOrderProjectionDelivery,
    SalesOrderProjectionRevision,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::ProjectionExt;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `sales_order_projection` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTIONS: &str = <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTIONS;
/// `sales_order_projection_revision` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTION_REVISIONS: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_REVISIONS;
/// `sales_order_projection_delivery` 集合名（单一来源：`ProjectionExt` 关联常量）。
const SALES_ORDER_PROJECTION_DELIVERIES: &str =
    <mongodb::Database as ProjectionExt>::SALES_ORDER_PROJECTION_DELIVERIES;

/// 投影列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRow {
    /// 实体主键。
    pub id: String,
    /// 卡券销售单。
    pub sales_order_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 商城最后确认版本。
    pub current_acked_revision_id: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderProjectionFilter {
    /// 卡券销售单；`None` 表示不筛选。
    pub sales_order_id: Option<SalesOrderId>,
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderProjectionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(sales_order_id) = &self.sales_order_id {
            filter.insert("sales_order_id", sales_order_id.to_string());
        }
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        filter
    }
}

impl Pagination for SalesOrderProjectionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderProjection> {
    /// 分页检索销售单执行投影列表（投影查询）。
    ///
    /// 只返回 [`SalesOrderProjectionRow`] 所需的列表字段，不加载整文档；
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
    pub async fn search_sales_order_projections(
        &self,
        filter: &SalesOrderProjectionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderProjectionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_projection_projection())
            .build();
        let collection = self.collection().clone_with_type::<SalesOrderProjectionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「销售单 + 目标商城」查找唯一稳定投影。
    ///
    /// 唯一性由 `uk_sales_order_projections_order_mall` 唯一索引保证
    /// （数据模型 §6.16）；本方法用于投影查询与幂等判定。
    ///
    /// # 参数
    /// * `sales_order_id` - 卡券销售单
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的投影；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_sales_order_and_mall(
        &self,
        sales_order_id: &SalesOrderId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjection>> {
        self.find_one(
            doc! {
                "sales_order_id": sales_order_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }
}

/// 投影修订列表投影行（Decimal128 金额原样投影，不做舍入换算）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionRevisionRow {
    /// 实体主键。
    pub id: String,
    /// 所属投影稳定身份。
    pub projection_id: String,
    /// 修订序号（同一投影内从 1 递增）。
    pub revision_no: u32,
    /// 投影来源。
    pub projection_source: ProjectionSource,
    /// ERP 销售版本。
    pub sales_order_revision_id: String,
    /// 商城客户标识。
    pub customer_external_identity: String,
    /// 卡券面额。
    pub face_value: Amount,
    /// 卡张数。
    pub card_count: u32,
    /// 电子卡或实体卡。
    pub card_form: CardForm,
    /// ERP 生效时间。
    pub effective_at: i64,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl<'a> Repository<'a, SalesOrderProjectionRevision> {
    /// 按「投影 + 修订序号」查找唯一投影修订。
    ///
    /// 唯一性由 `uk_sales_order_projection_revisions_projection_revision` 唯一
    /// 索引保证（数据模型 §6.16）；修订不可变，只读入口。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `revision_no` - 修订序号
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的投影修订；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_revision_by_no(
        &self,
        projection_id: &entities::ids::SalesOrderProjectionId,
        revision_no: u32,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionRevision>> {
        self.find_one(
            doc! {
                "projection_id": projection_id.to_string(),
                "revision_no": revision_no,
            },
            executor,
        )
        .await
    }

    /// 列出指定投影的全部分区版本（投影查询，修订号降序）。
    ///
    /// 只返回 [`SalesOrderProjectionRevisionRow`] 所需的列表字段，不加载整文档；
    /// 修订为不可变版本（§4.4），本方法只读，不提供任何删除入口。
    ///
    /// # 参数
    /// * `projection_id` - 所属投影稳定身份
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按 `revision_no` 降序的投影行列表。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_revisions_by_projection(
        &self,
        projection_id: &entities::ids::SalesOrderProjectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SalesOrderProjectionRevisionRow>> {
        let options = FindOptions::builder()
            .sort(doc! { "revision_no": -1 })
            .projection(sales_order_projection_revision_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionRevisionRow>();
        mongo_ops::find_many(
            &collection,
            doc! {
                "projection_id": projection_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

/// 投影下发列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SalesOrderProjectionDeliveryRow {
    /// 实体主键。
    pub id: String,
    /// 待下发投影版本。
    pub projection_revision_id: String,
    /// 目标商城。
    pub target_mall_id: String,
    /// 下发状态。
    pub status: ProjectionDeliveryStatus,
    /// 发送次数。
    pub attempt_count: u32,
    /// 商城确认时间。
    pub mall_ack_at: Option<i64>,
    /// 错误码。
    pub error_code: Option<String>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 投影下发列表筛选条件。
#[derive(Debug, Clone)]
pub struct SalesOrderProjectionDeliveryFilter {
    /// 目标商城；`None` 表示不筛选。
    pub target_mall_id: Option<SourceSystemId>,
    /// 下发状态；`None` 表示不筛选。
    pub status: Option<ProjectionDeliveryStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SalesOrderProjectionDeliveryFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(target_mall_id) = &self.target_mall_id {
            filter.insert("target_mall_id", target_mall_id.to_string());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SalesOrderProjectionDeliveryFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SalesOrderProjectionDelivery> {
    /// 分页检索投影下发记录（投影查询）。
    ///
    /// 只返回 [`SalesOrderProjectionDeliveryRow`] 所需的列表字段，不加载整文档；
    /// 排序字段白名单在 [`sort_doc`] 内收敛。下发状态查询由
    /// `idx_sales_order_projection_deliveries_status` 索引支撑（§6.16）。
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
    pub async fn search_sales_order_projection_deliveries(
        &self,
        filter: &SalesOrderProjectionDeliveryFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SalesOrderProjectionDeliveryRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(sales_order_projection_delivery_projection())
            .build();
        let collection = self
            .collection()
            .clone_with_type::<SalesOrderProjectionDeliveryRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按「投影修订 + 目标商城」查找唯一下发记录。
    ///
    /// # 参数
    /// * `revision_id` - 待下发投影版本
    /// * `target_mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的下发记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_delivery_by_revision_and_mall(
        &self,
        revision_id: &SalesOrderProjectionRevisionId,
        target_mall_id: &SourceSystemId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SalesOrderProjectionDelivery>> {
        self.find_one(
            doc! {
                "projection_revision_id": revision_id.to_string(),
                "target_mall_id": target_mall_id.to_string(),
            },
            executor,
        )
        .await
    }
}

/// D27 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `ProjectionExt::projection()` 访问。
pub struct ProjectionRepository<'a> {
    db: &'a Database,
}

impl<'a> ProjectionRepository<'a> {
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

    /// 建立投影稳定身份及其首个投影版本（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_order_projections` 与 `sales_order_projection_revisions`，
    /// 保证「稳定投影 + 首个不可变版本」原子可见（数据模型 §6.16）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有投影没有版本的半成品；
    /// Service 必须通过 `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `projection` - 待写入的投影稳定身份
    /// * `revision` - 待写入的首个投影版本
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_projection_revision(
        &self,
        projection: &SalesOrderProjection,
        revision: &SalesOrderProjectionRevision,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjection>(SALES_ORDER_PROJECTIONS),
            projection,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionRevision>(SALES_ORDER_PROJECTION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        Ok(())
    }

    /// 建立投影版本及其下发记录（跨集合多步骤写入）。
    ///
    /// 依次写入 `sales_order_projection_revisions` 与
    /// `sales_order_projection_deliveries`，保证「不可变版本 + 下发记录」
    /// 原子可见（数据模型 §6.16）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下没有下发记录的投影版本；
    /// Service 必须通过事务传入执行器。
    ///
    /// # 参数
    /// * `revision` - 待写入的投影版本
    /// * `delivery` - 待写入的下发记录
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]）或 MongoDB 写入
    /// 失败时返回错误。
    pub async fn create_projection_revision_with_delivery(
        &self,
        revision: &SalesOrderProjectionRevision,
        delivery: &SalesOrderProjectionDelivery,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionRevision>(SALES_ORDER_PROJECTION_REVISIONS),
            revision,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SalesOrderProjectionDelivery>(SALES_ORDER_PROJECTION_DELIVERIES),
            delivery,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单收敛）。
///
/// # 参数
/// * `sort_by` - 排序字段；仅允许 `sales_order_id`/`updated_at`/`created_at`，
///   其余一律回退 `created_at` 降序
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    match sort_by {
        Some("sales_order_id") => doc! { "sales_order_id": direction },
        Some("updated_at") => doc! { "updated_at": direction },
        _ => doc! { "created_at": direction },
    }
}

/// 投影列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_projection() -> Document {
    doc! {
        "id": 1,
        "sales_order_id": 1,
        "target_mall_id": 1,
        "current_acked_revision_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 投影修订列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_revision_projection() -> Document {
    doc! {
        "id": 1,
        "projection_id": 1,
        "revision_no": 1,
        "projection_source": 1,
        "sales_order_revision_id": 1,
        "customer_external_identity": 1,
        "face_value": 1,
        "card_count": 1,
        "card_form": 1,
        "effective_at": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 投影下发列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn sales_order_projection_delivery_projection() -> Document {
    doc! {
        "id": 1,
        "projection_revision_id": 1,
        "target_mall_id": 1,
        "status": 1,
        "attempt_count": 1,
        "mall_ack_at": 1,
        "error_code": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, SalesOrderProjectionDeliveryFilter, SalesOrderProjectionFilter};
    use entities::ids::{SalesOrderId, SourceSystemId};
    use entities::projection::ProjectionDeliveryStatus;
    use mongodb::bson::doc;

    #[test]
    fn projection_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderProjectionFilter {
            sales_order_id: Some(SalesOrderId::new("so-1")),
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("sales_order_id").unwrap(), "so-1");
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
    }

    #[test]
    fn delivery_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SalesOrderProjectionDeliveryFilter {
            target_mall_id: Some(SourceSystemId::new("mall-1")),
            status: Some(ProjectionDeliveryStatus::Retrying),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("target_mall_id").unwrap(), "mall-1");
        assert_eq!(document.get_str("status").unwrap(), "retrying");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("sales_order_id"), true),
            doc! { "sales_order_id": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false),
            doc! { "created_at": -1 },
            "白名单外字段一律回退默认排序"
        );
    }
}
