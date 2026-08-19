//! 域 D28 `card_instance` 仓储：mall_consumption_cutover、mall_card_instance(+_correction)、
//! mall_balance_snapshot。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本文件只补充域特有
//! 查询与跨集合多步骤写入入口。集合名常量统一从 `CardInstanceExt` 关联常量导入。
//!
//! 事实/纠错/快照集合（`mall_balance_snapshot`、`mall_card_instance_correction`）是
//! 不可变追加事实（§4.5），**不提供软删除方法**：只暴露 `create` 与只读查询，
//! 调用方拿不到带软删除的通用 `Repository`。
//!
//! 筛选/行类型定义在本文件，经 `CardInstanceExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::card_instance::{
    CardSourceType, CutoverStatus, MallBalanceSnapshot, MallCardInstance, MallCardInstanceCorrection,
    MallConsumptionCutover,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::CardInstanceExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `mall_card_instance` 集合名（单一来源：`CardInstanceExt` 关联常量）。
const MALL_CARD_INSTANCES: &str = <mongodb::Database as CardInstanceExt>::MALL_CARD_INSTANCES;
/// `mall_card_instance_correction` 集合名（单一来源：`CardInstanceExt` 关联常量）。
const MALL_CARD_INSTANCE_CORRECTIONS: &str =
    <mongodb::Database as CardInstanceExt>::MALL_CARD_INSTANCE_CORRECTIONS;
/// `mall_balance_snapshot` 集合名（单一来源：`CardInstanceExt` 关联常量）。
const MALL_BALANCE_SNAPSHOTS: &str = <mongodb::Database as CardInstanceExt>::MALL_BALANCE_SNAPSHOTS;

/// 切换记录列表投影行（列表接口只取必要字段，禁止返回整文档）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallConsumptionCutoverRow {
    /// 实体主键。
    pub id: String,
    /// 目标商城。
    pub mall_id: String,
    /// 切换状态。
    pub status: CutoverStatus,
    /// 启用时间 `T`，启用前为空。
    pub enabled_at: Option<entities::common::time::Instant>,
    /// 上线负责人。
    pub enabled_by: Option<String>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 切换记录列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallConsumptionCutoverFilter {
    /// 目标商城代码（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 切换状态；`None` 表示不筛选。
    pub status: Option<CutoverStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`created_at`/`enabled_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallConsumptionCutoverFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for MallConsumptionCutoverFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

/// 卡实例列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct MallCardInstanceRow {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub mall_id: String,
    /// 不可反推卡号、卡密的稳定引用。
    pub opaque_instance_ref: String,
    /// 映射后的 ERP 销售单。
    pub origin_sales_order_id: entities::ids::SalesOrderId,
    /// 初始余额（Decimal128 持久化）。
    pub initial_balance: entities::money::Amount,
    /// 基线形成时间。
    pub baseline_at: entities::common::time::Instant,
    /// 实时或历史基线。
    pub source_type: CardSourceType,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 卡实例列表筛选条件。
#[derive(Debug, Clone)]
pub struct MallCardInstanceFilter {
    /// 来源商城代码（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub mall_id: Option<String>,
    /// 稳定引用（字面量忽略大小写模糊匹配）；`None` 表示不筛选。
    pub opaque_instance_ref: Option<String>,
    /// 来源类型；`None` 表示不筛选。
    pub source_type: Option<CardSourceType>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单：`baseline_at`/`created_at`，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for MallCardInstanceFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        insert_literal_regex_filter(&mut filter, "mall_id", self.mall_id.as_deref());
        insert_literal_regex_filter(
            &mut filter,
            "opaque_instance_ref",
            self.opaque_instance_ref.as_deref(),
        );
        if let Some(source_type) = self.source_type {
            filter.insert("source_type", source_type.as_str());
        }
        filter
    }
}

impl Pagination for MallCardInstanceFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, MallConsumptionCutover> {
    /// 分页检索切换记录列表（投影查询）。
    ///
    /// 只返回 [`MallConsumptionCutoverRow`] 所需的列表字段，不加载整文档；
    /// 排序字段按白名单映射（非法字段回落到 `created_at`），禁止透传任意字段名。
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
    pub async fn search_cutovers(
        &self,
        filter: &MallConsumptionCutoverFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallConsumptionCutoverRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["created_at", "enabled_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(cutover_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallConsumptionCutoverRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按商城查找已启用切换记录。
    ///
    /// 「每个商城只能有一个已启用 `T`」由 `uk_mall_consumption_cutovers_mall` 唯一索引
    /// 保证（每商城至多一条切换记录），本方法用于登记 `T` 前的启用前置校验与
    /// 履约链归属比较。
    ///
    /// # 参数
    /// * `mall_id` - 目标商城
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回已启用的切换记录；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_enabled_cutover_by_mall_id(
        &self,
        mall_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallConsumptionCutover>> {
        self.find_one(
            doc! {
                "mall_id": mall_id,
                "status": CutoverStatus::Enabled.as_str(),
            },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, MallCardInstance> {
    /// 分页检索卡实例列表（投影查询）。
    ///
    /// 只返回 [`MallCardInstanceRow`] 所需的列表字段，不加载整文档；
    /// 排序字段按白名单映射（非法字段回落到 `created_at`）。
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
    pub async fn search_card_instances(
        &self,
        filter: &MallCardInstanceFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<MallCardInstanceRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(
                filter.sort_by.as_deref(),
                &["baseline_at", "created_at"],
                filter.sort_ascending,
            ))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(card_instance_projection())
            .build();
        let collection = self.collection().clone_with_type::<MallCardInstanceRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按商城与稳定引用查找卡实例基线。
    ///
    /// 唯一性由 `uk_mall_card_instances_identity` 唯一索引保证；本方法用于
    /// 基线幂等接收判定，服务层不得做「先查后插」的重复性判断。
    ///
    /// # 参数
    /// * `mall_id` - 来源商城
    /// * `opaque_instance_ref` - 不可反推卡号、卡密的稳定引用
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除基线；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_identity(
        &self,
        mall_id: &str,
        opaque_instance_ref: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallCardInstance>> {
        self.find_one(
            doc! {
                "mall_id": mall_id,
                "opaque_instance_ref": opaque_instance_ref,
            },
            executor,
        )
        .await
    }
}

/// `mall_balance_snapshot` 只读追加仓储（余额快照是不可变事实，§4.5 不设软删除）。
pub struct BalanceSnapshotRepository<'a> {
    db: &'a Database,
}

impl<'a> BalanceSnapshotRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加余额快照。
    ///
    /// 快照不可变（只提供 `new()`）；`(mall_card_instance_id, snapshot_at)` 与
    /// 非空 `source_snapshot_version` 的唯一性由唯一索引保证（§6.17），
    /// 冲突时透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `snapshot` - 待追加的快照
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(&self, snapshot: &MallBalanceSnapshot, executor: &mut dyn Executor) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), snapshot, executor).await
    }

    /// 按 ID 查找快照。
    ///
    /// # 参数
    /// * `id` - 快照主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的快照；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallBalanceSnapshot>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按卡实例与时间范围取余额快照序列（按 `snapshot_at` 升序）。
    ///
    /// # 参数
    /// * `mall_card_instance_id` - 卡实例
    /// * `from` - 起始快照时间（含）；`None` 表示不设下界
    /// * `to` - 结束快照时间（含）；`None` 表示不设上界
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按时间升序的快照序列。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_card_and_range(
        &self,
        mall_card_instance_id: &entities::ids::MallCardInstanceId,
        from: Option<entities::common::time::Instant>,
        to: Option<entities::common::time::Instant>,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallBalanceSnapshot>> {
        let mut filter = doc! {
            "mall_card_instance_id": mall_card_instance_id.to_string(),
            "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
        };
        let mut snapshot_at = Document::new();
        if let Some(from) = from {
            snapshot_at.insert("$gte", from.unix_secs());
        }
        if let Some(to) = to {
            snapshot_at.insert("$lte", to.unix_secs());
        }
        if !snapshot_at.is_empty() {
            filter.insert("snapshot_at", snapshot_at);
        }

        mongo_ops::find_many(
            &self.collection(),
            filter,
            FindOptions::builder().sort(doc! { "snapshot_at": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallBalanceSnapshot> {
        self.db.collection::<MallBalanceSnapshot>(MALL_BALANCE_SNAPSHOTS)
    }
}

/// `mall_card_instance_correction` 只读追加仓储（纠错是不可变追加事实，§4.5 不设软删除）。
pub struct CardInstanceCorrectionRepository<'a> {
    db: &'a Database,
}

impl<'a> CardInstanceCorrectionRepository<'a> {
    /// 创建仓储实例。
    ///
    /// # 参数
    /// * `db` - 目标 MongoDB 数据库
    ///
    /// # 返回
    /// 返回仓储实例。
    pub fn new(db: &'a Database) -> Self {
        Self { db }
    }

    /// 追加卡实例纠错。
    ///
    /// 纠错不可变（只提供 `new()`）；`(mall_card_instance_id, correction_no)` 唯一
    /// 与「非空 `supersedes_correction_id` 唯一」由唯一索引保证（§6.17），
    /// 冲突时透出 [`crate::Error::DuplicateKey`]。
    ///
    /// # 参数
    /// * `correction` - 待追加的纠错
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 追加成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 唯一索引冲突或 MongoDB 写入失败时返回错误。
    pub async fn create(
        &self,
        correction: &MallCardInstanceCorrection,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(&self.collection(), correction, executor).await
    }

    /// 按 ID 查找纠错。
    ///
    /// # 参数
    /// * `id` - 纠错主键
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的纠错；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_id(
        &self,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<MallCardInstanceCorrection>> {
        mongo_ops::find_one(
            &self.collection(),
            doc! { "id": id, "deleted_at": NOT_DELETED_TIMESTAMP_BSON },
            executor,
        )
        .await
    }

    /// 按卡实例取纠错链（按 `correction_no` 升序）。
    ///
    /// 纠错链只追加不覆盖（§6.17）；当前归属/余额纠错值由链中该类型最后一条
    /// 记录派生，链尾锁定由 P3 在追加前校验。
    ///
    /// # 参数
    /// * `mall_card_instance_id` - 卡实例
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按纠错号升序的纠错链。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn list_by_card(
        &self,
        mall_card_instance_id: &entities::ids::MallCardInstanceId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<MallCardInstanceCorrection>> {
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "mall_card_instance_id": mall_card_instance_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().sort(doc! { "correction_no": 1 }).build(),
            executor,
        )
        .await
    }

    /// 返回当前实体对应的 MongoDB 集合（内部使用）。
    fn collection(&self) -> mongodb::Collection<MallCardInstanceCorrection> {
        self.db
            .collection::<MallCardInstanceCorrection>(MALL_CARD_INSTANCE_CORRECTIONS)
    }
}

/// D28 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的跨集合原子
/// 写入入口，由 `CardInstanceExt::card_instance()` 访问。
pub struct CardInstanceRepository<'a> {
    db: &'a Database,
}

impl<'a> CardInstanceRepository<'a> {
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

    /// 建立卡实例基线并记录初始余额快照（跨集合多步骤写入）。
    ///
    /// 实时基线到达时，基线与初始余额快照必须原子可见（§6.17：基线形成即落
    /// 首份余额快照）。**必须收到事务执行器**：本方法不构成原子边界，传入
    /// `NoTransaction` 时两笔写入各自自动提交，中途失败会留下只有基线没有快照
    /// 的半成品；Service 必须通过 `database::Transactional::with_transaction`
    /// 传入事务会话。
    ///
    /// # 参数
    /// * `instance` - 待写入的卡实例基线
    /// * `snapshot` - 待写入的初始余额快照
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射为
    /// 冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_card_instance_with_initial_snapshot(
        &self,
        instance: &MallCardInstance,
        snapshot: &MallBalanceSnapshot,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<MallCardInstance>(MALL_CARD_INSTANCES),
            instance,
            executor,
        )
        .await?;
        mongo_ops::insert_one(
            &self.db.collection::<MallBalanceSnapshot>(MALL_BALANCE_SNAPSHOTS),
            snapshot,
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（白名单映射）。
///
/// # 参数
/// * `sort_by` - 排序字段；不在白名单或为 `None` 时默认 `created_at`
/// * `allowed` - 允许的排序字段白名单
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, allowed: &[&str], sort_ascending: bool) -> Document {
    let field = sort_by
        .filter(|field| allowed.contains(field))
        .unwrap_or("created_at");
    let direction = if sort_ascending { 1 } else { -1 };
    doc! { field: direction }
}

/// 切换记录列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn cutover_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "status": 1,
        "enabled_at": 1,
        "enabled_by": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 卡实例列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn card_instance_projection() -> Document {
    doc! {
        "id": 1,
        "mall_id": 1,
        "opaque_instance_ref": 1,
        "origin_sales_order_id": 1,
        "initial_balance": 1,
        "baseline_at": 1,
        "source_type": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use entities::card_instance::CutoverStatus;
    use mongodb::bson::doc;

    use super::{sort_doc, MallCardInstanceFilter, MallConsumptionCutoverFilter, Pagination, QueryFilter};

    #[test]
    fn cutover_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallConsumptionCutoverFilter {
            mall_id: Some("mall-a".to_string()),
            status: Some(CutoverStatus::Enabled),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert!(document.contains_key("mall_id"));
        assert_eq!(document.get_str("status").unwrap(), "enabled");
    }

    #[test]
    fn card_instance_filter_applies_optional_fields_and_deleted_filter() {
        let filter = MallCardInstanceFilter {
            mall_id: Some("mall-a".to_string()),
            opaque_instance_ref: None,
            source_type: Some(entities::card_instance::CardSourceType::Realtime),
            page: 2,
            page_size: 10,
            sort_by: Some("baseline_at".to_string()),
            sort_ascending: true,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("source_type").unwrap(), "realtime");
        assert_eq!(filter.skip(), 10);
        assert_eq!(filter.limit(), 10);
    }

    #[test]
    fn sort_doc_maps_only_whitelisted_fields_and_defaults_to_created_at() {
        assert_eq!(
            sort_doc(None, &["created_at", "enabled_at"], false),
            doc! { "created_at": -1 }
        );
        assert_eq!(
            sort_doc(Some("enabled_at"), &["created_at", "enabled_at"], true),
            doc! { "enabled_at": 1 }
        );
        assert_eq!(
            sort_doc(Some("malicious_field"), &["created_at", "enabled_at"], false),
            doc! { "created_at": -1 },
            "白名单外字段必须回落到默认排序"
        );
    }
}
