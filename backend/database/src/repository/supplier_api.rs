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
    BusinessCapabilityConfirmation, ConnectionEnvironment, HealthCheckResult, SupplierApiCapability,
    SupplierApiCapabilityCode, SupplierApiCapabilityStatus, SupplierApiConnection, SupplierApiConnectionId,
    SupplierApiConnectionStatus, SupplierConnectionAction, SupplierConnectionCommandReceipt,
    SupplierHealthCheckRun,
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
    /// 地址引用是否已由权威注册表确认绑定。
    pub endpoint_reference_bound: bool,
    /// 密钥引用是否已由权威注册表确认绑定。
    pub credential_reference_bound: bool,
    /// 技术配置版本。
    pub technical_config_version: u64,
    /// 最近成功健康证据对应的技术配置版本。
    pub last_healthy_technical_config_version: Option<u64>,
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
}

impl<'a> Repository<'a, BusinessCapabilityConfirmation> {
    /// 按连接、操作人与幂等摘要查找既有业务确认。
    pub async fn find_business_confirmation_receipt(
        &self,
        connection_id: &SupplierApiConnectionId,
        confirmed_by: &str,
        idempotency_key_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessCapabilityConfirmation>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "confirmed_by": confirmed_by,
                "idempotency_key_hash": idempotency_key_hash,
            },
            executor,
        )
        .await
    }

    /// 查询连接下全部追加式业务确认，按最新优先返回。
    pub async fn find_business_confirmations_by_connection(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BusinessCapabilityConfirmation>> {
        self.find_many_sorted(
            doc! { "connection_id": connection_id.to_string() },
            doc! { "confirmed_at": -1, "id": -1 },
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierHealthCheckRun> {
    /// 按后台任务 ID 查询健康检查运行记录。
    pub async fn find_health_run_by_job(
        &self,
        job_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierHealthCheckRun>> {
        self.find_one(doc! { "background_job_id": job_id }, executor)
            .await
    }

    /// 查询连接最近的健康运行记录。
    pub async fn find_health_runs_by_connection(
        &self,
        connection_id: &SupplierApiConnectionId,
        limit: i64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierHealthCheckRun>> {
        let options = FindOptions::builder()
            .sort(doc! { "created_at": -1, "id": -1 })
            .limit(limit.clamp(1, 100))
            .build();
        mongo_ops::find_many(
            &self.collection(),
            doc! {
                "connection_id": connection_id.to_string(),
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            options,
            executor,
        )
        .await
    }
}

impl<'a> Repository<'a, SupplierConnectionCommandReceipt> {
    /// 按连接、动作、操作人与幂等摘要查询命令回执。
    pub async fn find_command_receipt(
        &self,
        connection_id: &SupplierApiConnectionId,
        action: SupplierConnectionAction,
        actor_id: &str,
        idempotency_key_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierConnectionCommandReceipt>> {
        self.find_one(
            doc! {
                "connection_id": connection_id.to_string(),
                "action": action.as_str(),
                "actor_id": actor_id,
                "idempotency_key_hash": idempotency_key_hash,
            },
            executor,
        )
        .await
    }
}

/// 停用连接前由服务端重验的关联业务影响。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupplierConnectionImpact {
    pub active_offerings: u64,
    pub active_publications: u64,
    pub open_supplier_orders: u64,
    pub active_sync_jobs: u64,
}

impl SupplierConnectionImpact {
    /// 判断是否存在任何必须先处理的活动业务对象。
    pub fn has_blockers(self) -> bool {
        self.active_offerings > 0
            || self.active_publications > 0
            || self.open_supplier_orders > 0
            || self.active_sync_jobs > 0
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

    /// 汇总连接停用前必须重验的活动业务影响。
    ///
    /// 供给、发布、履约订单和目录同步任务都使用各自集合的正式状态；查询不读取
    /// 地址或密钥引用，也不返回业务对象明细。
    pub async fn connection_impact(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<SupplierConnectionImpact> {
        let (active_offerings, active_offering_revisions) =
            self.active_offering_revisions(connection_id, executor).await?;
        let active_publications = self
            .active_publication_count(&active_offering_revisions, executor)
            .await?;
        let open_supplier_orders = mongo_ops::count_documents(
            &self.db.collection::<Document>(
                <Database as super::extensions::SupplierFulfillmentExt>::SUPPLIER_FULFILLMENT_ORDERS,
            ),
            doc! {
                "connection_id": connection_id.to_string(),
                "fulfillment_status": { "$nin": ["COMPLETED", "REJECTED"] },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await?;
        let active_sync_jobs = mongo_ops::count_documents(
            &self
                .db
                .collection::<Document>(<Database as super::extensions::BulkJobExt>::BACKGROUND_JOBS),
            doc! {
                "domain_job_type": "SUPPLIER_CATALOG_SYNC",
                "domain_job_id": connection_id.to_string(),
                "status": { "$in": ["pending", "running", "partially_succeeded"] },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await?;
        Ok(SupplierConnectionImpact {
            active_offerings,
            active_publications,
            open_supplier_orders,
            active_sync_jobs,
        })
    }

    async fn active_offering_revisions(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<(u64, Vec<String>)> {
        #[derive(Deserialize)]
        struct OfferingRevisionRow {
            current_revision_id: Option<String>,
        }
        let collection = self.db.collection::<OfferingRevisionRow>(
            <Database as super::extensions::SupplierOfferingExt>::SUPPLIER_OFFERINGS,
        );
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "source_connection_id": connection_id.to_string(),
                "status": "ACTIVE",
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! { "current_revision_id": 1 })
                .build(),
            executor,
        )
        .await?;
        let count = rows.len() as u64;
        let revision_ids = rows
            .into_iter()
            .filter_map(|row| row.current_revision_id)
            .collect();
        Ok((count, revision_ids))
    }

    async fn active_publication_count(
        &self,
        offering_revision_ids: &[String],
        executor: &mut dyn Executor,
    ) -> Result<u64> {
        if offering_revision_ids.is_empty() {
            return Ok(0);
        }
        #[derive(Deserialize)]
        struct PublicationRevisionRow {
            product_publication_id: String,
        }
        let revisions = mongo_ops::find_many(
            &self.db.collection::<PublicationRevisionRow>(
                <Database as super::extensions::PublicationExt>::PRODUCT_PUBLICATION_REVISIONS,
            ),
            doc! {
                "supplier_offering_revision_id": { "$in": offering_revision_ids },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder()
                .projection(doc! { "product_publication_id": 1 })
                .build(),
            executor,
        )
        .await?;
        let publication_ids: Vec<String> = revisions
            .into_iter()
            .map(|row| row.product_publication_id)
            .collect();
        if publication_ids.is_empty() {
            return Ok(0);
        }
        mongo_ops::count_documents(
            &self.db.collection::<Document>(
                <Database as super::extensions::PublicationExt>::PRODUCT_PUBLICATIONS,
            ),
            doc! {
                "id": { "$in": publication_ids },
                "status": "mall_effective",
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await
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
        "endpoint_reference_bound": 1,
        "credential_reference_bound": 1,
        "technical_config_version": 1,
        "last_healthy_technical_config_version": 1,
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
