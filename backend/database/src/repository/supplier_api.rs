//! 域 D25 `supplier_api` 仓储：supplier_api_connection、supplier_api_capability。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `SupplierApiExt` 关联常量导入。
//!
//! 筛选/行类型定义在本文件，经 `SupplierApiExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::supplier_api::{
    ConnectionEnvironment, HealthCheckResult, SupplierApiCapability, SupplierApiCapabilityCode,
    SupplierApiCapabilityStatus, SupplierApiConnection, SupplierApiConnectionId, SupplierApiConnectionStatus,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::SupplierApiExt;
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `supplier_api_connection` 集合名（单一来源：`SupplierApiExt` 关联常量）。
const SUPPLIER_API_CONNECTIONS: &str = <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS;
/// `supplier_api_capability` 集合名（单一来源：`SupplierApiExt` 关联常量）。
const SUPPLIER_API_CAPABILITIES: &str = <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CAPABILITIES;

/// 连接列表投影行（列表接口只取必要字段，禁止返回整文档；密钥引用
/// `credential_reference` 属于敏感字段，不进入任何列表投影）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierApiConnectionRow {
    /// 实体主键。
    pub id: String,
    /// API 供应商。
    pub supplier_id: String,
    /// ERP 内稳定连接代码。
    pub connection_code: String,
    /// 连接环境。
    pub environment: ConnectionEnvironment,
    /// 启停/故障状态。
    pub status: SupplierApiConnectionStatus,
    /// 最近健康检查时间。
    pub last_health_at: Option<i64>,
    /// 最近健康检查结果。
    pub last_health_result: Option<HealthCheckResult>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 连接列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierApiConnectionFilter {
    /// API 供应商；`None` 表示不筛选。
    pub supplier_id: Option<String>,
    /// 连接代码（字面量、忽略大小写的子串匹配）；`None` 表示不筛选。
    pub connection_code: Option<String>,
    /// 连接环境；`None` 表示不筛选。
    pub environment: Option<ConnectionEnvironment>,
    /// 启停/故障状态；`None` 表示不筛选。
    pub status: Option<SupplierApiConnectionStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierApiConnectionFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(supplier_id) = &self.supplier_id {
            filter.insert("supplier_id", supplier_id);
        }
        insert_literal_regex_filter(&mut filter, "connection_code", self.connection_code.as_deref());
        if let Some(environment) = self.environment {
            filter.insert("environment", environment.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierApiConnectionFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierApiConnection> {
    /// 分页检索供应商 API 连接列表（投影查询）。
    ///
    /// 只返回 [`SupplierApiConnectionRow`] 所需的列表字段，不加载整文档，
    /// 敏感字段（密钥管理系统引用）不进入投影；排序字段白名单在
    /// [`sort_doc`] 内收敛，禁止透传任意字段名。
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
    pub async fn search_supplier_api_connections(
        &self,
        filter: &SupplierApiConnectionFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierApiConnectionRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_api_connection_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierApiConnectionRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 按稳定连接代码查找未删除连接。
    ///
    /// 唯一性由 `uk_supplier_api_connections_connection_code` 唯一索引保证
    /// （数据模型 §6.14）；本方法用于连接查询与幂等判定。
    ///
    /// # 参数
    /// * `connection_code` - ERP 内稳定连接代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配的未删除连接；无匹配时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询失败时返回错误。
    pub async fn find_by_connection_code(
        &self,
        connection_code: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierApiConnection>> {
        self.find_one(doc! { "connection_code": connection_code }, executor)
            .await
    }
}

/// 连接能力列表投影行。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierApiCapabilityRow {
    /// 实体主键。
    pub id: String,
    /// 所属连接。
    pub connection_id: String,
    /// 能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 能力启停状态。
    pub status: SupplierApiCapabilityStatus,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 连接能力列表筛选条件。
#[derive(Debug, Clone)]
pub struct SupplierApiCapabilityFilter {
    /// 所属连接；`None` 表示不筛选。
    pub connection_id: Option<SupplierApiConnectionId>,
    /// 能力代码；`None` 表示不筛选。
    pub capability_code: Option<SupplierApiCapabilityCode>,
    /// 能力启停状态；`None` 表示不筛选。
    pub status: Option<SupplierApiCapabilityStatus>,
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数。
    pub page_size: u32,
    /// 排序字段（白名单在 `sort_doc` 内收敛，默认 `created_at`）。
    pub sort_by: Option<String>,
    /// 是否升序；`false` 表示降序（默认）。
    pub sort_ascending: bool,
}

impl QueryFilter for SupplierApiCapabilityFilter {
    /// 转换为 MongoDB 查询条件（自动追加未删除过滤）。
    ///
    /// # 返回
    /// 返回查询条件文档。
    fn to_doc(&self) -> Document {
        let mut filter = doc! { "deleted_at": NOT_DELETED_TIMESTAMP_BSON };
        if let Some(connection_id) = &self.connection_id {
            filter.insert("connection_id", connection_id.to_string());
        }
        if let Some(capability_code) = self.capability_code {
            filter.insert("capability_code", capability_code.as_str());
        }
        if let Some(status) = self.status {
            filter.insert("status", status.as_str());
        }
        filter
    }
}

impl Pagination for SupplierApiCapabilityFilter {
    /// 返回页码与单页条数。
    ///
    /// # 返回
    /// 返回 `(page, page_size)` 元组。
    fn page_and_size(&self) -> (u64, u64) {
        (self.page, u64::from(self.page_size))
    }
}

impl<'a> Repository<'a, SupplierApiCapability> {
    /// 分页检索连接能力声明列表（投影查询）。
    ///
    /// 只返回 [`SupplierApiCapabilityRow`] 所需的列表字段，不加载整文档
    /// （`constraint_snapshot` 长文本不进入列表投影）。
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
    pub async fn search_supplier_api_capabilities(
        &self,
        filter: &SupplierApiCapabilityFilter,
        executor: &mut dyn Executor,
    ) -> Result<PageResult<SupplierApiCapabilityRow>> {
        let options = FindOptions::builder()
            .sort(sort_doc(filter.sort_by.as_deref(), filter.sort_ascending))
            .skip(filter.skip())
            .limit(filter.limit())
            .projection(supplier_api_capability_projection())
            .build();
        let collection = self.collection().clone_with_type::<SupplierApiCapabilityRow>();
        let items = mongo_ops::find_many(&collection, filter.to_doc(), options, executor).await?;
        let total = mongo_ops::count_documents(&self.collection(), filter.to_doc(), executor).await?;

        Ok(PageResult {
            items,
            total: total as i64,
        })
    }

    /// 查找指定连接的全部能力声明（按能力代码升序）。
    ///
    /// 单次查询取回整组能力，避免逐条 N+1；返回完整实体供 Service 做
    /// 能力判定与替换计算。
    ///
    /// # 参数
    /// * `connection_id` - 所属连接
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回该连接下按 `capability_code` 升序的能力声明。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_capabilities_by_connection(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierApiCapability>> {
        self.find_many_sorted(
            doc! { "connection_id": connection_id.to_string() },
            doc! { "capability_code": 1 },
            executor,
        )
        .await
    }

    /// 批量查找多个连接的能力声明（`$in` 一次取回，禁止 N+1）。
    ///
    /// # 参数
    /// * `connection_ids` - 连接 ID 列表；为空时直接返回空列表
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回全部匹配连接的能力声明集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或游标读取失败时返回错误。
    pub async fn find_capabilities_by_connections(
        &self,
        connection_ids: &[SupplierApiConnectionId],
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierApiCapability>> {
        if connection_ids.is_empty() {
            return Ok(Vec::new());
        }
        let ids: Vec<String> = connection_ids.iter().map(ToString::to_string).collect();
        self.find_many(doc! { "connection_id": { "$in": ids } }, executor)
            .await
    }
}

/// D25 域专用仓储：跨集合、多步骤且必须位于事务内的聚合写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；本类型只承载依赖事务的
/// 跨集合原子写入入口，由 `SupplierApiExt::supplier_api()` 访问。
pub struct SupplierApiRepository<'a> {
    db: &'a Database,
}

impl<'a> SupplierApiRepository<'a> {
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

    /// 建立连接及其能力声明（跨集合多步骤写入）。
    ///
    /// 依次写入 `supplier_api_connections` 与 `supplier_api_capabilities`，
    /// 保证「连接配置 + 能力清单」原子可见（数据模型 §6.14）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有连接没有能力（或只有部分
    /// 能力）的半成品；Service 必须通过
    /// `database::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `connection` - 待写入的连接配置
    /// * `capabilities` - 待写入的能力声明清单
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`crate::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_connection_with_capabilities(
        &self,
        connection: &SupplierApiConnection,
        capabilities: &[SupplierApiCapability],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self
                .db
                .collection::<SupplierApiConnection>(SUPPLIER_API_CONNECTIONS),
            connection,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierApiCapability>(SUPPLIER_API_CAPABILITIES),
            capabilities.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 原子替换连接的能力声明（先删后写）。
    ///
    /// 先删除该连接的全部能力声明，再写入新清单（数据模型 §6.14
    /// `(connection_id, capability_code)` 唯一约束由唯一索引保证）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时删除与写入各自自动提交，中途失败会留下能力清单被清空或新旧混杂的
    /// 中间态；Service 必须通过事务传入执行器。
    ///
    /// # 参数
    /// * `connection_id` - 所属连接
    /// * `capabilities` - 替换后的能力声明清单
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当新清单与残留数据产生唯一索引冲突（透出
    /// [`crate::Error::DuplicateKey`]）或 MongoDB 写入失败时返回错误。
    pub async fn replace_connection_capabilities(
        &self,
        connection_id: &SupplierApiConnectionId,
        capabilities: &[SupplierApiCapability],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::delete_many(
            &self
                .db
                .collection::<SupplierApiCapability>(SUPPLIER_API_CAPABILITIES),
            doc! { "connection_id": connection_id.to_string() },
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self
                .db
                .collection::<SupplierApiCapability>(SUPPLIER_API_CAPABILITIES),
            capabilities.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }
}

/// 构建排序文档（排序字段白名单收敛）。
///
/// # 参数
/// * `sort_by` - 排序字段；仅允许 `connection_code`/`updated_at`/`created_at`，
///   其余一律回退 `created_at` 降序
/// * `sort_ascending` - 升序为 `true`，降序为 `false`
///
/// # 返回
/// 返回排序条件文档。
fn sort_doc(sort_by: Option<&str>, sort_ascending: bool) -> Document {
    let direction = if sort_ascending { 1 } else { -1 };
    match sort_by {
        Some("connection_code") => doc! { "connection_code": direction },
        Some("updated_at") => doc! { "updated_at": direction },
        _ => doc! { "created_at": direction },
    }
}

/// 连接列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_api_connection_projection() -> Document {
    doc! {
        "id": 1,
        "supplier_id": 1,
        "connection_code": 1,
        "environment": 1,
        "status": 1,
        "last_health_at": 1,
        "last_health_result": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 连接能力列表投影字段。
///
/// # 返回
/// 返回投影条件文档。
fn supplier_api_capability_projection() -> Document {
    doc! {
        "id": 1,
        "connection_id": 1,
        "capability_code": 1,
        "status": 1,
        "version": 1,
        "created_at": 1,
    }
}

#[cfg(test)]
mod tests {
    use super::{sort_doc, QueryFilter, SupplierApiCapabilityFilter, SupplierApiConnectionFilter};
    use entities::supplier_api::{
        ConnectionEnvironment, SupplierApiCapabilityCode, SupplierApiCapabilityStatus,
        SupplierApiConnectionStatus,
    };
    use mongodb::bson::doc;

    #[test]
    fn connection_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SupplierApiConnectionFilter {
            supplier_id: Some("sup-1".to_string()),
            connection_code: Some("CN-1".to_string()),
            environment: Some(ConnectionEnvironment::Production),
            status: Some(SupplierApiConnectionStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("supplier_id").unwrap(), "sup-1");
        assert_eq!(document.get_str("environment").unwrap(), "production");
        assert_eq!(document.get_str("status").unwrap(), "active");
        let regex = document.get_document("connection_code").unwrap();
        assert_eq!(regex.get_str("$regex").unwrap(), "CN\\-1");
        assert_eq!(regex.get_str("$options").unwrap(), "i");
    }

    #[test]
    fn capability_filter_applies_optional_fields_and_deleted_filter() {
        let filter = SupplierApiCapabilityFilter {
            connection_id: Some(entities::ids::SupplierApiConnectionId::new("conn-1")),
            capability_code: Some(SupplierApiCapabilityCode::Order),
            status: Some(SupplierApiCapabilityStatus::Active),
            page: 1,
            page_size: 20,
            sort_by: None,
            sort_ascending: false,
        };

        let document = filter.to_doc();
        assert_eq!(document.get_i64("deleted_at").unwrap(), 0);
        assert_eq!(document.get_str("connection_id").unwrap(), "conn-1");
        assert_eq!(document.get_str("capability_code").unwrap(), "order");
        assert_eq!(document.get_str("status").unwrap(), "active");
    }

    #[test]
    fn sort_doc_defaults_to_created_at_and_whitelists_fields() {
        assert_eq!(sort_doc(None, false), doc! { "created_at": -1 });
        assert_eq!(
            sort_doc(Some("connection_code"), true),
            doc! { "connection_code": 1 }
        );
        assert_eq!(
            sort_doc(Some("任意字段"), false),
            doc! { "created_at": -1 },
            "白名单外字段一律回退默认排序"
        );
    }
}
