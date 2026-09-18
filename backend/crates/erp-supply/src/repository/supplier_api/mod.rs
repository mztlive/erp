//! 域 D25 `supplier_api` 仓储：supplier_api_connection、supplier_api_capability。
//!
//! 单一集合 CRUD 与乐观锁直接复用 [`Repository`] 基类（base.rs：
//! `update`/`soft_delete`/`restore` 比较 `id + version` 做 CAS，版本不匹配返回
//! [`persistence_core::Error::OptimisticLockingError`]）；本模块只补充域特有查询与
//! 跨集合多步骤写入入口。集合名常量统一从 `SupplierApiExt` 关联常量导入。
//!
//! 筛选/行类型定义在 `query` 子模块，经 `SupplierApiExt` 的关联类型对外暴露
//! （`extensions/mod.rs` 已冻结，无法在 `repository/mod.rs` 增加 re-export）。

#![allow(async_fn_in_trait)]

use entity_core::NOT_DELETED_TIMESTAMP_BSON;
use mongodb::Database;
use mongodb::bson::doc;
use mongodb::options::FindOptions;
use persistence_core::{Executor, Result, mongo_ops};
use serde::Deserialize;

pub use super::SupplierApiExt;
use super::{SupplierFulfillmentExt, SupplierOfferingExt};
use crate::entity::supplier_api::{
    BusinessCapabilityConfirmation, SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiConnection,
    SupplierApiConnectionId, SupplierConnectionAction, SupplierConnectionBusinessImpact,
    SupplierConnectionCommandReceipt, SupplierHealthCheckRun,
};
use crate::repository::owned::{
    BusinessCapabilityConfirmationRepository, SupplierApiCapabilityRepository,
    SupplierApiConnectionRepository, SupplierConnectionCommandReceiptRepository,
    SupplierHealthCheckRunRepository,
};

mod capability_change_batch;
mod query;

pub use query::{
    BusinessCapabilityConfirmationRepositoryExt, SupplierApiCapabilityFilter,
    SupplierApiCapabilityRepositoryExt, SupplierApiCapabilityRow, SupplierApiConnectionFilter,
    SupplierApiConnectionRepositoryExt, SupplierApiConnectionRow,
    SupplierConnectionCommandReceiptRepositoryExt, SupplierHealthCheckRunRepositoryExt,
};

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

/// 停用连接前由服务端重验的关联业务影响。
pub type SupplierConnectionImpact = SupplierConnectionBusinessImpact;
/// 供应链本域拥有的连接业务影响，后台任务计数由组合层追加。
#[derive(Debug, Clone, Copy)]
pub struct SupplierConnectionOwnedImpact {
    pub active_offerings: u64,
    pub open_supplier_orders: u64,
}
impl SupplierConnectionOwnedImpact {
    /// 与调用方取得的权威任务计数组成原治理规则输入。
    pub fn with_active_sync_jobs(self, active_sync_jobs: u64) -> SupplierConnectionImpact {
        SupplierConnectionImpact {
            active_offerings: self.active_offerings,
            open_supplier_orders: self.open_supplier_orders,
            active_sync_jobs,
        }
    }
}

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
    pub owned_impact: SupplierConnectionOwnedImpact,
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
        SupplierApiConnectionRepository::new(self.db, SUPPLIER_API_CONNECTIONS)
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
        SupplierApiCapabilityRepository::new(self.db, SUPPLIER_API_CAPABILITIES)
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
        SupplierApiCapabilityRepository::new(self.db, SUPPLIER_API_CAPABILITIES)
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
        BusinessCapabilityConfirmationRepository::new(self.db, SUPPLIER_API_BUSINESS_CONFIRMATIONS)
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
        BusinessCapabilityConfirmationRepository::new(self.db, SUPPLIER_API_BUSINESS_CONFIRMATIONS)
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
        SupplierHealthCheckRunRepository::new(self.db, SUPPLIER_API_HEALTH_CHECK_RUNS)
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
        SupplierHealthCheckRunRepository::new(self.db, SUPPLIER_API_HEALTH_CHECK_RUNS)
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
        SupplierConnectionCommandReceiptRepository::new(self.db, SUPPLIER_API_COMMAND_RECEIPTS)
            .find_command_receipt(connection_id, action, actor_id, idempotency_key_hash, executor)
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
        let health_runs = self.recent_health_runs(connection_id, health_limit, executor).await?;
        let owned_impact = self.owned_connection_impact(connection_id, executor).await?;
        Ok(SupplierApiGovernanceData { capabilities, confirmations, health_runs, owned_impact })
    }

    /// 建立连接及其能力声明（跨集合多步骤写入）。
    ///
    /// 依次写入 `supplier_api_connections` 与 `supplier_api_capabilities`，
    /// 保证「连接配置 + 能力清单」原子可见（数据模型 §6.14）。
    /// **必须收到事务执行器**：本方法不构成原子边界，传入 `NoTransaction`
    /// 时两笔写入各自自动提交，中途失败会留下只有连接没有能力（或只有部分
    /// 能力）的半成品；Service 必须通过
    /// `persistence_core::Transactional::with_transaction` 传入事务会话。
    ///
    /// # 参数
    /// * `connection` - 待写入的连接配置
    /// * `capabilities` - 待写入的能力声明清单
    /// * `executor` - 数据访问执行器，必须位于事务中
    ///
    /// # 错误
    /// 当唯一索引冲突（透出 [`persistence_core::Error::DuplicateKey`]，由 Service 映射
    /// 为冲突语义）或 MongoDB 写入失败时返回错误。
    pub async fn create_connection_with_capabilities(
        &self,
        connection: &SupplierApiConnection,
        capabilities: &[SupplierApiCapability],
        executor: &mut dyn Executor,
    ) -> Result<()> {
        mongo_ops::insert_one(
            &self.db.collection::<SupplierApiConnection>(SUPPLIER_API_CONNECTIONS),
            connection,
            executor,
        )
        .await?;
        mongo_ops::insert_many(
            &self.db.collection::<SupplierApiCapability>(SUPPLIER_API_CAPABILITIES),
            capabilities.to_vec(),
            executor,
        )
        .await?;
        Ok(())
    }

    /// 汇总连接停用前必须重验的活动业务影响。
    ///
    /// 读取同供给域的活动供给和未完成供应商订单，保留原顺序与状态。
    /// 后台目录任务计数由组合层在本方法之后读取；查询不读取地址或密钥引用。
    ///
    /// # 参数
    /// * `connection_id` - 待评估的供应商连接 ID
    /// * `executor` - 数据访问执行器，由 Service 决定是否位于事务中
    ///
    /// # 返回
    /// 返回活动供给和未完成订单计数。
    ///
    /// # 错误
    /// 任一 MongoDB 查询或反序列化失败时返回错误。
    pub async fn owned_connection_impact(
        &self,
        connection_id: &SupplierApiConnectionId,
        executor: &mut dyn Executor,
    ) -> Result<SupplierConnectionOwnedImpact> {
        let (active_offerings, _) = self.active_offering_revisions(connection_id, executor).await?;
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
        Ok(SupplierConnectionOwnedImpact { active_offerings, open_supplier_orders })
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
        let collection = self.db.supplier_offerings().collection().clone_with_type::<OfferingRevisionRow>();
        let rows = mongo_ops::find_many(
            &collection,
            doc! {
                "source_connection_id": connection_id.to_string(),
                "status": "ACTIVE",
                "deleted_at": NOT_DELETED_TIMESTAMP_BSON,
            },
            FindOptions::builder().projection(doc! { "current_revision_id": 1 }).build(),
            executor,
        )
        .await?;
        let count = rows.len() as u64;
        let revision_ids = rows.into_iter().filter_map(|row| row.current_revision_id).collect();
        Ok((count, revision_ids))
    }
}
