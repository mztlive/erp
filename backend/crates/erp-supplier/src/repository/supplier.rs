//! 域 D09 `supplier` 仓储：supplier_account、supplier_commercial_profile_revision、
//! supplier_capability(+_revision)、supplier_qualification(+_revision)、
//! supplier_qualification_capability、supplier_rating_revision（数据模型 §6.2）。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS）；本模块只补充
//! 域特有查询与跨集合多步骤写入入口。`supplier_account`/`supplier_capability`/
//! `supplier_qualification` 是稳定基础资料（可软删除，身份字段全局唯一）；
//! 四个 `*_revision` 集合是不可变修订（追加式、**不提供**软删除）；
//! `supplier_qualification_capability` 是资质 ↔ 能力的纯关联行。
//!
//! 集合名常量统一从 `SupplierExt` 关联常量导入（唯一权威来源）；筛选/行类型
//! 定义在职责子模块，经本模块重新导出并由 `SupplierExt` 的关联类型对外暴露。

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use erp_core::ids::SupplierAccountId;
use mongodb::Database;
use mongodb::bson::{Document, doc};
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

use super::extensions::SupplierExt;

mod account;
mod bundle;
mod capability;
mod command;
mod qualification;
mod revision;

pub use account::{SupplierAccountFilter, SupplierAccountRow};
pub use bundle::{
    SupplierDetailBundle, SupplierListBundle, SupplierListSearchInput, SupplierQualificationHealthFilter,
};
pub use capability::SupplierCapabilityFilter;
pub use qualification::SupplierQualificationFilter;
pub use revision::SupplierCommercialProfileFilter;

/// `supplier_account` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_ACCOUNTS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_ACCOUNTS;
/// `supplier_commercial_profile_revision` 集合名（单一来源：`SupplierExt` 关联常量）。
const SUPPLIER_COMMERCIAL_PROFILE_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_COMMERCIAL_PROFILE_REVISIONS;
/// 供应商能力集合名。
const SUPPLIER_CAPABILITIES: &str = <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITIES;
/// 供应商能力修订集合名。
const SUPPLIER_CAPABILITY_REVISIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_CAPABILITY_REVISIONS;
/// 供应商资质集合名。
const SUPPLIER_QUALIFICATIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATIONS;
/// 供应商资质修订集合名。
const SUPPLIER_QUALIFICATION_REVISIONS: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_REVISIONS;
/// 资质适用能力关联集合名。
const SUPPLIER_QUALIFICATION_CAPABILITIES: &str =
    <mongodb::Database as SupplierExt>::SUPPLIER_QUALIFICATION_CAPABILITIES;
/// 供应商评级修订集合名。
const SUPPLIER_RATING_REVISIONS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_RATING_REVISIONS;
/// 供应商资料幂等命令集合名。
const SUPPLIER_PROFILE_COMMANDS: &str = <mongodb::Database as SupplierExt>::SUPPLIER_PROFILE_COMMANDS;

/// 仅承载筛选阶段所需的供应商角色 ID。
#[derive(Debug, Deserialize)]
struct SupplierIdRow {
    supplier_id: SupplierAccountId,
}

/// 按供应商子集合条件读取去重后的供应商角色 ID。
///
/// # 参数
/// * `collection` - 供应商能力或资质集合句柄
/// * `filter` - 已按业务语义构造的集合查询条件
/// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
///
/// # 返回
/// 返回按字符串 ID 稳定排序并去重的供应商角色 ID。
///
/// # 错误
/// 当 MongoDB 查询或反序列化失败时返回错误。
async fn find_supplier_ids(
    collection: mongodb::Collection<SupplierIdRow>,
    mut filter: Document,
    executor: &mut dyn Executor,
) -> Result<Vec<SupplierAccountId>> {
    filter.insert("deleted_at", NOT_DELETED_TIMESTAMP_BSON);
    let options = FindOptions::builder().projection(doc! { "supplier_id": 1, "_id": 0 }).build();
    let rows = mongo_ops::find_many(&collection, filter, options, executor).await?;
    let mut ids: Vec<SupplierAccountId> = rows.into_iter().map(|row| row.supplier_id).collect();
    ids.sort_by_key(ToString::to_string);
    ids.dedup();
    Ok(ids)
}

/// D09 域专用仓储：语义化聚合读取与跨集合事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；列表筛选、批量资料、当前修订号
/// 和跨集合原子写入由本类型收敛，通过 `SupplierExt::supplier()` 访问。
pub struct SupplierRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierRepository<'a> {
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

    /// 按能力负责人收窄供应商集合；空条件表示不筛选。
    ///
    /// # 参数
    /// * `owner_user_ids` - 能力负责人 ID
    /// * `executor` - 调用方执行器
    ///
    /// # 返回
    /// `None` 表示不筛选；`Some` 为命中的供应商 ID，空集合保持空结果。
    ///
    /// # 错误
    /// 数据库读取失败时拒绝。
    ///
    /// # 关键业务约束
    /// 能力负责人只收窄，不单独扩大可见供应商。
    pub async fn supplier_ids_by_capability_owners(
        &self,
        owner_user_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<Option<Vec<SupplierAccountId>>> {
        if owner_user_ids.is_empty() {
            return Ok(None);
        }
        let ids = find_supplier_ids(
            self.db.collection::<SupplierIdRow>(SUPPLIER_CAPABILITIES),
            doc! { "owner_user_id": { "$in": owner_user_ids } },
            executor,
        )
        .await?;
        Ok(Some(ids))
    }
}
