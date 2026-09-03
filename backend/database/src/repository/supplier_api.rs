//! 域 D25 `supplier_api` 仓储：supplier_api_connection、supplier_api_capability。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`crate::Error::OptimisticLockingError`]）；本文件只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `SupplierApiExt` 关联常量导入。
//!
//! 筛选/行类型定义在本文件，经 `SupplierApiExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

use entities::bulk_job::BackgroundJob;
use entities::supplier_api::{
    BusinessCapabilityConfirmation, ConnectionEnvironment, HealthCheckResult, SupplierApiCapability,
    SupplierApiCapabilityCode, SupplierApiCapabilityStatus, SupplierApiConnection, SupplierApiConnectionId,
    SupplierApiConnectionStatus, SupplierConnectionAction, SupplierConnectionBusinessImpact,
    SupplierConnectionCommandReceipt, SupplierHealthCheckRun,
};
use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::bson::{doc, Document};
use mongodb::options::FindOptions;
use mongodb::Database;
use serde::{Deserialize, Serialize};

use super::extensions::{
    AccessControlExt, BulkJobExt, PublicationExt, SupplierApiExt, SupplierFulfillmentExt, SupplierOfferingExt,
};
use super::regex_filter::insert_literal_regex_filter;
use super::{PageResult, Pagination, QueryFilter, Repository};
use crate::executor::Executor;
use crate::{mongo_ops, Result};

/// `supplier_api_connection` 集合名（单一来源：`SupplierApiExt` 关联常量）。
const SUPPLIER_API_CONNECTIONS: &str = <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CONNECTIONS;
/// `supplier_api_capability` 集合名（单一来源：`SupplierApiExt` 关联常量）。
const SUPPLIER_API_CAPABILITIES: &str = <mongodb::Database as SupplierApiExt>::SUPPLIER_API_CAPABILITIES;
/// 采购业务确认集合名。
const SUPPLIER_API_BUSINESS_CONFIRMATIONS: &str =
    <mongodb::Database as SupplierApiExt>::SUPPLIER_API_BUSINESS_CONFIRMATIONS;
/// 健康检查运行记录集合名。
const SUPPLIER_API_HEALTH_CHECK_RUNS: &str =
    <mongodb::Database as SupplierApiExt>::SUPPLIER_API_HEALTH_CHECK_RUNS;
/// 连接治理命令回执集合名。
const SUPPLIER_API_COMMAND_RECEIPTS: &str =
    <mongodb::Database as SupplierApiExt>::SUPPLIER_API_COMMAND_RECEIPTS;

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
    /// 按连接 ID 集合批量读取能力声明。
    ///
    /// 列表读模型使用一次查询补齐当前页能力摘要，禁止逐连接读取。
    ///
    /// # 参数
    /// * `connection_ids` - 当前页连接 ID；空集合直接返回空结果
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按连接 ID、能力代码稳定排序的完整能力实体。
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
        let ids = connection_ids.iter().map(ToString::to_string).collect::<Vec<_>>();
        self.find_many_sorted(
            doc! { "connection_id": { "$in": ids } },
            doc! { "connection_id": 1, "capability_code": 1 },
            executor,
        )
        .await
    }

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
pub type SupplierConnectionImpact = SupplierConnectionBusinessImpact;

/// 连接详情和状态命令共用的治理查询结果。
#[derive(Debug, Clone)]
pub struct SupplierApiGovernanceData {
    /// 当前连接能力。
    pub capabilities: Vec<SupplierApiCapability>,
    /// 最新优先的采购业务确认。
    pub confirmations: Vec<BusinessCapabilityConfirmation>,
    /// 最新优先的健康检查运行记录。
    pub health_runs: Vec<SupplierHealthCheckRun>,
    /// 停用前活动业务影响。
    pub impact: SupplierConnectionImpact,
}

/// D25 域专用仓储：连接治理读取与跨集合事务写入。
///
/// 单一集合 CRUD 使用 [`Repository`] 基类；连接、能力、确认、健康记录、
/// 后台任务和影响汇总等治理查询由本类型收敛，通过 `SupplierApiExt::supplier_api()` 访问。
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

    /// 按 ID 读取未删除连接。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配连接；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn connection(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierApiConnection>> {
        Repository::new(self.db, SUPPLIER_API_CONNECTIONS)
            .find_by_id(connection_id.as_ref(), executor)
            .await
    }

    /// 按连接和能力代码读取未删除能力声明。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `capability_code` - 固定能力代码
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配能力；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn connection_capability(
        &self,
        connection_id: &SupplierApiConnectionId,
        capability_code: SupplierApiCapabilityCode,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierApiCapability>> {
        Repository::new(self.db, SUPPLIER_API_CAPABILITIES)
            .find_one(
                doc! {
                    "connection_id": connection_id.to_string(),
                    "capability_code": capability_code.as_str(),
                },
                executor,
            )
            .await
    }

    /// 按连接读取全部能力声明。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回按能力代码升序排列的完整实体。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn connection_capabilities(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierApiCapability>> {
        Repository::new(self.db, SUPPLIER_API_CAPABILITIES)
            .find_capabilities_by_connection(connection_id, executor)
            .await
    }

    /// 按幂等身份读取采购业务确认回执。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `confirmed_by` - 确认人账号 ID
    /// * `idempotency_key_hash` - 客户端幂等键摘要
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回既有确认；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn business_confirmation_receipt(
        &self,
        connection_id: &SupplierApiConnectionId,
        confirmed_by: &str,
        idempotency_key_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BusinessCapabilityConfirmation>> {
        Repository::new(self.db, SUPPLIER_API_BUSINESS_CONFIRMATIONS)
            .find_business_confirmation_receipt(connection_id, confirmed_by, idempotency_key_hash, executor)
            .await
    }

    /// 按连接读取最新优先的采购业务确认。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新确认优先的追加式历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn business_confirmations(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<Vec<BusinessCapabilityConfirmation>> {
        Repository::new(self.db, SUPPLIER_API_BUSINESS_CONFIRMATIONS)
            .find_business_confirmations_by_connection(connection_id, executor)
            .await
    }

    /// 按后台任务读取健康检查运行记录。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配运行记录；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn health_run_for_job(
        &self,
        job_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierHealthCheckRun>> {
        Repository::new(self.db, SUPPLIER_API_HEALTH_CHECK_RUNS)
            .find_health_run_by_job(job_id, executor)
            .await
    }

    /// 按连接读取最新优先的健康检查运行记录。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `limit` - 最大返回条数，仓储收敛到 `1..=100`
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回最新运行优先的健康检查历史。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn recent_health_runs(
        &self,
        connection_id: &SupplierApiConnectionId,
        limit: i64,
        executor: &mut dyn Executor,
    ) -> Result<Vec<SupplierHealthCheckRun>> {
        Repository::new(self.db, SUPPLIER_API_HEALTH_CHECK_RUNS)
            .find_health_runs_by_connection(connection_id, limit, executor)
            .await
    }

    /// 按幂等身份读取连接治理命令回执。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `action` - 固定治理动作
    /// * `actor_id` - 操作人账号 ID
    /// * `idempotency_key_hash` - 客户端幂等键摘要
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回既有命令回执；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn command_receipt(
        &self,
        connection_id: &SupplierApiConnectionId,
        action: SupplierConnectionAction,
        actor_id: &str,
        idempotency_key_hash: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<SupplierConnectionCommandReceipt>> {
        Repository::new(self.db, SUPPLIER_API_COMMAND_RECEIPTS)
            .find_command_receipt(connection_id, action, actor_id, idempotency_key_hash, executor)
            .await
    }

    /// 读取连接治理后台任务。
    ///
    /// # 参数
    /// * `job_id` - 后台任务 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配后台任务；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn governance_job(
        &self,
        job_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        self.db.background_jobs().find_by_id(job_id, executor).await
    }

    /// 按稳定审计 ID 读取连接治理审计记录。
    ///
    /// # 参数
    /// * `audit_id` - 审计记录 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回匹配审计记录；不存在时返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn governance_audit(
        &self,
        audit_id: &str,
        executor: &mut dyn Executor,
    ) -> Result<Option<entities::AuditLog>> {
        self.db.audit_logs().find_by_id(audit_id, executor).await
    }

    /// 读取属于指定连接的治理后台任务。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `job_id` - 后台任务 ID
    /// * `job_types` - 允许的治理任务类型
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 任务存在且归属、类型均匹配时返回任务，否则返回 `None`。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    pub async fn connection_job(
        &self,
        connection_id: &SupplierApiConnectionId,
        job_id: &str,
        job_types: &[&str],
        executor: &mut dyn Executor,
    ) -> Result<Option<BackgroundJob>> {
        if job_types.is_empty() {
            return Ok(None);
        }
        self.db
            .background_jobs()
            .find_one(
                doc! {
                    "id": job_id,
                    "domain_job_id": connection_id.to_string(),
                    "domain_job_type": { "$in": job_types },
                },
                executor,
            )
            .await
    }

    /// 批量读取连接治理上下文与停用影响。
    ///
    /// # 参数
    /// * `connection_id` - 连接 ID
    /// * `health_limit` - 最大健康检查历史条数
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回能力、采购确认、健康检查和活动业务影响。
    ///
    /// # 错误
    /// 任一 MongoDB 查询或反序列化失败时返回错误。
    pub async fn governance_data(
        &self,
        connection_id: &SupplierApiConnectionId,
        health_limit: i64,
        executor: &mut dyn Executor,
    ) -> Result<SupplierApiGovernanceData> {
        let capabilities = self.connection_capabilities(connection_id, executor).await?;
        let confirmations = self.business_confirmations(connection_id, executor).await?;
        let health_runs = self
            .recent_health_runs(connection_id, health_limit, executor)
            .await?;
        let impact = self.connection_impact(connection_id, executor).await?;
        Ok(SupplierApiGovernanceData {
            capabilities,
            confirmations,
            health_runs,
            impact,
        })
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
    /// 跨聚合事实包：外聚合读经由所属仓储访问器（`supplier_offerings`、
    /// `product_publications`、`supplier_fulfillment_orders`、`background_jobs`）
    /// 编排，本类型不直连外聚合集合。
    /// 供给、发布、履约订单和目录同步任务都使用各自集合的正式状态；查询不读取
    /// 地址或密钥引用，也不返回业务对象明细。
    ///
    /// # 参数
    /// * `connection_id` - 待评估的供应商连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回活动供给、发布、未完成订单和目录同步任务计数。
    ///
    /// # 错误
    /// 任一 MongoDB 查询或反序列化失败时返回错误。
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
            &self.db.supplier_fulfillment_orders().collection(),
            doc! {
                "connection_id": connection_id.to_string(),
                "fulfillment_status": { "$nin": ["COMPLETED", "REJECTED"] },
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            executor,
        )
        .await?;
        let active_sync_jobs = mongo_ops::count_documents(
            &self.db.background_jobs().collection(),
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

    /// 读取连接下活动供给数量及其当前修订 ID。
    ///
    /// # 参数
    /// * `connection_id` - 供应商连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回活动供给数量和非空当前修订 ID 集合。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
    async fn active_offering_revisions(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<(u64, Vec<String>)> {
        #[derive(Deserialize)]
        struct OfferingRevisionRow {
            current_revision_id: Option<String>,
        }
        let collection = self
            .db
            .supplier_offerings()
            .collection()
            .clone_with_type::<OfferingRevisionRow>();
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

    /// 统计指定供给修订关联的当前有效商品发布数量。
    ///
    /// # 参数
    /// * `offering_revision_ids` - 活动供给当前修订 ID 集合
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回状态为商城生效的商品发布数量；输入为空时返回零。
    ///
    /// # 错误
    /// 当 MongoDB 查询或反序列化失败时返回错误。
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
            &self
                .db
                .product_publication_revisions()
                .collection()
                .clone_with_type::<PublicationRevisionRow>(),
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
            &self.db.product_publications().collection(),
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
