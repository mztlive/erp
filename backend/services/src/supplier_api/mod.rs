//! 域 D25 `supplier_api` 服务编排。
//!
//! 事务边界只在 Service（conventions §6.1）：
//! - 创建连接 + 能力清单：跨集合原子写入（`create_connection_with_capabilities`）
//!   → `database::Transactional::with_transaction`；
//! - 更新连接、原子替换能力：跨集合（业务写 + 审计）→ 事务；
//! - 健康检查（二期专属，P3 §3/§7）：**外部 HTTP 调用在事务之外完成**——
//!   事务 1 先落 `inbox_message`（Received）+ 审计；事务外经 `SupplierApiGateway`
//!   尝试外部调用（超时/重试上限/错误分类）；事务 2 把结果经
//!   `inbox_message`（Processed/Failed）+ `integration_error_task` 承接
//!   （D34 仓储 `IntegrationOpsExt`），并把连接健康检查结果成对写入。
//!
//! 跨域只调对方 Repository（P3 §2）：D09 `supplier_accounts` 供应商存在性校验。
//! 业务规则（状态、成对字段、唯一键）都在 entities（已冻结只读），Service 只编排。

use std::sync::Arc;

use database::{
    AccessControlExt, IntegrationOpsExt, NoTransaction, SupplierApiExt, SupplierExt, Transactional,
};
use entities::ids::{
    InboxMessageId, IntegrationErrorTaskId, SourceSystemId, SupplierApiCapabilityId, SupplierApiConnectionId,
};
use entities::integration_ops::{
    ErrorClass, InboxMessage, InboxMessageData, InboxMessageStatus, InboxMessageUpdate, IntegrationErrorTask,
    IntegrationErrorTaskData, MessageType,
};
use entities::supplier_api::{
    HealthCheckResult, SupplierApiCapability, SupplierApiCapabilityData, SupplierApiConnection,
    SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierApiConnectionUpdate,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::supplier_api::dto::SortDir;

mod dto;

pub use self::dto::{
    CapabilityItemRequest, CreateSupplierApiConnectionRequest, HealthCheckRequest, HealthCheckView, PageView,
    RateLimitPolicyRequest, ReplaceCapabilitiesRequest, SupplierApiCapabilityListParams,
    SupplierApiCapabilityView, SupplierApiConnectionDetailView, SupplierApiConnectionListParams,
    SupplierApiConnectionView, UpdateSupplierApiConnectionRequest,
};

/// 连接列表筛选条件类型（经 `SupplierApiExt` 关联类型跨 crate 可达）。
type SupplierApiConnectionFilter = <mongodb::Database as SupplierApiExt>::SupplierApiConnectionFilter;
/// 能力列表筛选条件类型。
type SupplierApiCapabilityFilter = <mongodb::Database as SupplierApiExt>::SupplierApiCapabilityFilter;

/// 外部调用错误分类（错误分类：临时故障/限流可自动重试，其余转人工，§7.7）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClassifiedError {
    /// 错误分类。
    pub class: ErrorClass,
    /// 稳定错误码。
    pub code: String,
    /// 脱敏错误摘要。
    pub summary: String,
}

/// 供应商 API 网关（外部 HTTP 调用统一入口）。
///
/// 实现要求（P3 §7、AGENTS.md 外部依赖容错）：统一设置超时（5 秒）、重试上限
/// （2 次）与错误分类；依赖失败降级为可观测错误。默认实现
/// [`UnavailableSupplierApiGateway`] 在端点引用无法解析为可调用地址时以分类错误
/// 失败关闭（当前无地址配置注册表，`config://` 引用不可解析），测试注入 mock 验证
/// 成功与失败两条路径。
pub trait SupplierApiGateway: Send + Sync {
    /// 执行一次连接健康检查（生产检查不创建业务订单，phase-2 §14.1）。
    ///
    /// # 参数
    /// * `connection` - 目标连接（提供端点引用与限流策略上下文）
    ///
    /// # 返回
    /// 检查成功返回 `Ok(())`；失败返回分类错误（可自动重试或转人工）。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    >;
}

/// 默认网关：端点引用不可解析时失败关闭（可观测降级）。
pub struct UnavailableSupplierApiGateway;

impl SupplierApiGateway for UnavailableSupplierApiGateway {
    /// 执行一次连接健康检查（默认实现恒失败关闭）。
    ///
    /// # 参数
    /// * `connection` - 目标连接
    ///
    /// # 返回
    /// 恒返回 `TransientFailure` 分类错误（端点引用为配置引用，未注册可调用地址）。
    fn health_check<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: ErrorClass::TransientFailure,
                code: "ENDPOINT_UNRESOLVED".to_string(),
                summary: format!(
                    "连接 {} 的端点引用未解析为可调用地址，健康检查失败关闭",
                    connection.connection_code
                ),
            })
        })
    }
}

/// 供应商 API 服务。
pub struct SupplierApiService {
    db: Database,
    gateway: Arc<dyn SupplierApiGateway>,
}

impl SupplierApiService {
    /// 创建供应商 API 服务实例。
    ///
    /// # 参数
    /// * `db` - 数据库实例
    /// * `gateway` - 外部调用网关
    ///
    /// # 返回
    /// 返回服务实例。
    pub fn new(db: Database, gateway: Arc<dyn SupplierApiGateway>) -> Self {
        Self { db, gateway }
    }

    /// 创建供应商 API 连接及其能力声明（跨集合事务写入）。
    ///
    /// 一个事务内写入 `supplier_api_connections`、`supplier_api_capabilities` 与
    /// 审计日志，保证「连接配置 + 能力清单」原子可见（数据模型 §6.14）。
    /// 唯一性（`connection_code`、`(connection_id, capability_code)`）由唯一索引
    /// 透出 `DuplicateKey` → 409，不做「先查后插」。
    ///
    /// # 参数
    /// * `req` - 创建请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建连接的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - API 供应商不存在
    /// * `ConflictError` - 连接代码重复
    /// * `ValidationError` - 请求体校验失败
    pub async fn create_connection(
        &self,
        req: CreateSupplierApiConnectionRequest,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionView> {
        req.validate()?;
        self.db
            .supplier_accounts()
            .find_by_id(&req.supplier_id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("API 供应商不存在".to_string()))?;

        let id = entities::ids::SupplierApiConnectionId::new(next_id());
        let connection = SupplierApiConnection::new(
            id,
            SupplierApiConnectionData {
                supplier_id: req.supplier_id,
                connection_code: req.connection_code,
                environment: req.environment,
                endpoint_reference: req.endpoint_reference,
                credential_reference: req.credential_reference,
                rate_limit_policy: req
                    .rate_limit_policy
                    .map(RateLimitPolicyRequest::into_policy)
                    .transpose()?,
                status: req.status.unwrap_or(SupplierApiConnectionStatus::Active),
            },
            actor.id(),
        )?;
        let capabilities = self.build_capabilities(&connection, req.capabilities)?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.create",
            "supplier_api_connection",
            connection.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let connection_tx = connection.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_api()
                        .create_connection_with_capabilities(&connection_tx, &capabilities, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(connection.into())
    }

    /// 分页查询连接列表。
    ///
    /// 排序字段白名单在 Service 层校验（api-contract §4）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn connection_list(
        &self,
        params: &SupplierApiConnectionListParams,
    ) -> Result<PageView<SupplierApiConnectionView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierApiConnectionFilter {
            supplier_id: query.supplier_id,
            connection_code: query.connection_code,
            environment: query.environment,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_api_connections()
            .search_supplier_api_connections(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierApiConnectionView {
                id: row.id,
                supplier_id: row.supplier_id,
                connection_code: row.connection_code,
                environment: row.environment,
                status: row.status,
                rate_limit_policy: None,
                last_health_at: row.last_health_at.map(|at| at as u64),
                last_health_result: row.last_health_result,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询连接详情（连接身份 + 能力清单）。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    ///
    /// # 返回
    /// 返回连接详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 连接不存在
    pub async fn connection_detail(&self, id: &str) -> Result<SupplierApiConnectionDetailView> {
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        let capabilities = self
            .db
            .supplier_api_capabilities()
            .find_capabilities_by_connection(
                &SupplierApiConnectionId::new(connection.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let connection_id = connection.base.id.clone();
        Ok(SupplierApiConnectionDetailView {
            capabilities: capabilities
                .into_iter()
                .map(|capability| SupplierApiCapabilityView {
                    id: capability.base.id,
                    connection_id: connection_id.clone(),
                    capability_code: capability.capability_code,
                    status: capability.status,
                    version: capability.base.version,
                    created_at: capability.base.created_at,
                })
                .collect(),
            connection: connection.into(),
        })
    }

    /// 更新连接（乐观锁语义，跨集合事务写入 + 审计）。
    ///
    /// 期望版本 `req.version` 与当前版本不一致时直接返回冲突（409）；
    /// 仓储层 `update` 同时以 `id + version` CAS 兜底并发竞争。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    /// * `req` - 更新请求（含期望版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回更新后连接的响应视图。
    ///
    /// # 错误
    /// * `NotFound` - 连接不存在
    /// * `ConflictError` - 期望版本与当前版本不一致
    /// * `ValidationError` - 请求体校验失败或健康检查信息不完整
    pub async fn update_connection(
        &self,
        id: &str,
        req: UpdateSupplierApiConnectionRequest,
        actor: &AuditActor,
    ) -> Result<SupplierApiConnectionView> {
        req.validate()?;
        let mut connection = self
            .db
            .supplier_api_connections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        if connection.base.version != req.version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        connection.update(
            SupplierApiConnectionUpdate {
                environment: req.environment,
                endpoint_reference: req.endpoint_reference,
                credential_reference: req.credential_reference,
                rate_limit_policy: req
                    .rate_limit_policy
                    .map(RateLimitPolicyRequest::into_policy)
                    .transpose()?,
                status: req.status,
                last_health_at: req
                    .last_health_at
                    .map(|secs| entities::common::time::Instant::from_unix_secs(secs as i64)),
                last_health_result: req.last_health_result,
            },
            actor.id(),
        )?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.update",
            "supplier_api_connection",
            connection.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_api_connections()
                        .update(&mut connection, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierApiConnection, crate::errors::Error>(connection)
                })
            })
            .await?;

        Ok(updated.into())
    }

    /// 原子替换连接能力声明（先删后写，跨集合事务写入 + 审计）。
    ///
    /// 以期望连接版本做并发控制；`(connection_id, capability_code)` 唯一索引
    /// 透出冲突 → 409。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    /// * `req` - 替换请求（含期望连接版本）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回替换后的能力清单视图。
    ///
    /// # 错误
    /// * `NotFound` - 连接不存在
    /// * `ConflictError` - 期望版本不一致或能力清单冲突
    /// * `ValidationError` - 请求体校验失败
    pub async fn replace_capabilities(
        &self,
        id: &str,
        req: ReplaceCapabilitiesRequest,
        actor: &AuditActor,
    ) -> Result<Vec<SupplierApiCapabilityView>> {
        req.validate()?;
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        if connection.base.version != req.expected_connection_version {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        let capabilities = self.build_capabilities(&connection, req.capabilities)?;
        let audit = actor.clone().resource_log(
            "supplier_api_capability.replace",
            "supplier_api_capability",
            connection.base.id.clone(),
        )?;

        let db = self.db.clone();
        let client = db.client().clone();
        let connection_id = SupplierApiConnectionId::new(connection.base.id.clone());
        let capabilities_tx = capabilities.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_api()
                        .replace_connection_capabilities(&connection_id, &capabilities_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        Ok(capabilities
            .into_iter()
            .map(|capability| SupplierApiCapabilityView {
                id: capability.base.id,
                connection_id: id.to_string(),
                capability_code: capability.capability_code,
                status: capability.status,
                version: capability.base.version,
                created_at: capability.base.created_at,
            })
            .collect())
    }

    /// 分页查询连接能力列表。
    ///
    /// # 参数
    /// * `params` - 查询参数（`connection_id`/`capability_code`/`status` 扁平筛选）
    ///
    /// # 返回
    /// 返回契约形状的分页视图（`items`/`total`/`page`/`page_size`）。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    pub async fn capability_list(
        &self,
        params: &SupplierApiCapabilityListParams,
    ) -> Result<PageView<SupplierApiCapabilityView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = SupplierApiCapabilityFilter {
            connection_id: query.connection_id,
            capability_code: query.capability_code,
            status: query.status,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .supplier_api_capabilities()
            .search_supplier_api_capabilities(&filter, &mut NoTransaction)
            .await?;
        let items = page
            .items
            .into_iter()
            .map(|row| SupplierApiCapabilityView {
                id: row.id,
                connection_id: row.connection_id,
                capability_code: row.capability_code,
                status: row.status,
                version: row.version,
                created_at: row.created_at,
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 执行连接健康检查（外部 HTTP 调用在事务之外完成）。
    ///
    /// 流程（二期专属，P3 §3/§7）：
    /// 1. 事务 1：落 `inbox_message`（`Received`）+ 审计；
    /// 2. 事务外：经 [`SupplierApiGateway`] 尝试外部调用（超时/重试上限/错误分类）；
    /// 3. 事务 2：成功 → 连接健康检查成对写入 + `inbox_message` 置 `Processed`；
    ///    失败 → 连接健康检查 `Failed` + `inbox_message` 置 `Failed` +
    ///    `integration_error_task` 承接（可自动重试分类 → `AutoRetrying`，
    ///    其余 → `ManualRequired`）+ 审计。
    ///
    /// 幂等：`(source_system_id, source_event_id)` 唯一索引承接，重复幂等键 → 409。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    /// * `req` - 健康检查请求（含幂等键）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回健康检查结果视图。
    ///
    /// # 错误
    /// * `NotFound` - 连接不存在
    /// * `ConflictError` - 幂等键重复提交
    /// * `ValidationError` - 请求体校验失败
    pub async fn run_health_check(
        &self,
        id: &str,
        req: HealthCheckRequest,
        actor: &AuditActor,
    ) -> Result<HealthCheckView> {
        req.validate()?;
        let connection = self
            .db
            .supplier_api_connections()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("连接不存在".to_string()))?;
        if connection.credential_reference.is_none() {
            return Err(Error::ValidationError(
                "密钥管理系统引用未绑定，无法执行健康检查".to_string(),
            ));
        }

        let message = InboxMessage::new(
            InboxMessageId::new(next_id()),
            InboxMessageData {
                source_system_id: SourceSystemId::new(connection.supplier_id.to_string()),
                source_event_id: format!("health_check:{}:{}", id, req.idempotency_key),
                message_type: MessageType::SupplierCallback,
                business_fact_key: format!("supplier_api_connection.health_check:{id}"),
                payload_schema_version: "v1".to_string(),
                payload_reference: Some(connection.endpoint_reference.clone()),
                status: InboxMessageStatus::Received,
                source_sent_at: None,
                received_at: entities::common::time::Instant::now(),
                processed_at: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.health_check",
            "supplier_api_connection",
            connection.base.id.clone(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let message_tx = message.clone();
        client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.inbox_messages().create(&message_tx, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<(), crate::errors::Error>(())
                })
            })
            .await?;

        let now = entities::common::time::Instant::now();
        match self.gateway.health_check(&connection).await {
            Ok(()) => {
                self.settle_health_success(id, connection, message, now, actor)
                    .await
            }
            Err(error) => {
                self.settle_health_failure(id, connection, message, now, error, actor)
                    .await
            }
        }
    }

    /// 把成功健康检查结果落库（事务 2）。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    /// * `connection` - 待更新连接实体
    /// * `message` - 待置 `Processed` 的消息
    /// * `at` - 检查时间
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回健康检查结果视图。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_health_success(
        &self,
        id: &str,
        mut connection: SupplierApiConnection,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        actor: &AuditActor,
    ) -> Result<HealthCheckView> {
        connection.record_health(HealthCheckResult::Healthy, at);
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Processed),
            processed_at: Some(at),
        })?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.health_check.settled",
            "supplier_api_connection",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let inbox_id = message.base.id.clone();
        let mut connection_tx = connection;
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.supplier_api_connections()
                        .update(&mut connection_tx, session)
                        .await?;
                    db.inbox_messages().update(&mut message, session).await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierApiConnection, crate::errors::Error>(connection_tx)
                })
            })
            .await?;

        Ok(HealthCheckView {
            checked_at: at.unix_secs() as u64,
            result: HealthCheckResult::Healthy,
            inbox_message_id: inbox_id,
            error_task_id: None,
            version: updated.base.version,
        })
    }

    /// 把失败健康检查结果落库（事务 2：错误任务 + 消息失败 + 连接健康检查）。
    ///
    /// # 参数
    /// * `id` - 连接 ID
    /// * `connection` - 待更新连接实体
    /// * `message` - 待置 `Failed` 的消息
    /// * `at` - 检查时间
    /// * `error` - 分类错误
    /// * `actor` - 审计操作人
    ///
    /// # 返回
    /// 返回健康检查结果视图（含错误任务 ID）。
    ///
    /// # 错误
    /// 乐观锁冲突或 MongoDB 写入失败时返回错误。
    async fn settle_health_failure(
        &self,
        id: &str,
        mut connection: SupplierApiConnection,
        mut message: InboxMessage,
        at: entities::common::time::Instant,
        error: ClassifiedError,
        actor: &AuditActor,
    ) -> Result<HealthCheckView> {
        connection.record_health(HealthCheckResult::Failed, at);
        message.update(InboxMessageUpdate {
            status: Some(InboxMessageStatus::Failed),
            processed_at: Some(at),
        })?;
        let task = IntegrationErrorTask::new(
            IntegrationErrorTaskId::new(next_id()),
            IntegrationErrorTaskData {
                message_id: Some(message.base.id.clone().into()),
                business_object_id: None,
                error_class: error.class,
                owner_role: Some("integration_ops".to_string()),
                owner_user_id: None,
            },
        )?;
        let audit = actor.clone().resource_log(
            "supplier_api_connection.health_check.failed",
            "supplier_api_connection",
            id.to_string(),
        )?;
        let db = self.db.clone();
        let client = db.client().clone();
        let inbox_id = message.base.id.clone();
        let task_id = task.base.id.clone();
        let mut connection_tx = connection;
        let updated = client
            .with_transaction(move |session| {
                Box::pin(async move {
                    db.integration_ops()
                        .create_error_task_with_message_failure(&task, &mut message, session)
                        .await?;
                    db.supplier_api_connections()
                        .update(&mut connection_tx, session)
                        .await?;
                    db.audit_logs().create(&audit, session).await?;
                    Ok::<SupplierApiConnection, crate::errors::Error>(connection_tx)
                })
            })
            .await?;

        Ok(HealthCheckView {
            checked_at: at.unix_secs() as u64,
            result: HealthCheckResult::Failed,
            inbox_message_id: inbox_id,
            error_task_id: Some(task_id),
            version: updated.base.version,
        })
    }

    /// 由能力行请求构建能力实体清单（能力代码去重，保留首次出现顺序）。
    ///
    /// # 参数
    /// * `connection` - 所属连接
    /// * `items` - 能力行请求
    ///
    /// # 返回
    /// 返回能力实体清单。
    ///
    /// # 错误
    /// 能力约束快照超长时返回错误。
    fn build_capabilities(
        &self,
        connection: &SupplierApiConnection,
        items: Vec<CapabilityItemRequest>,
    ) -> Result<Vec<SupplierApiCapability>> {
        let mut seen = Vec::new();
        let mut capabilities = Vec::with_capacity(items.len());
        for item in items {
            if !seen.contains(&item.capability_code) {
                seen.push(item.capability_code);
                capabilities.push(SupplierApiCapability::new(
                    SupplierApiCapabilityId::new(next_id()),
                    SupplierApiCapabilityData {
                        connection_id: SupplierApiConnectionId::new(connection.base.id.clone()),
                        capability_code: item.capability_code,
                        status: item
                            .status
                            .unwrap_or(entities::supplier_api::SupplierApiCapabilityStatus::Active),
                        constraint_snapshot: item.constraint_snapshot,
                    },
                )?);
            }
        }
        Ok(capabilities)
    }
}

#[cfg(test)]
mod tests {

    use entities::ids::{SupplierAccountId, SupplierApiConnectionId};
    use entities::integration_ops::ErrorClass;
    use entities::supplier_api::{
        ConnectionEnvironment, SupplierApiConnection, SupplierApiConnectionData, SupplierApiConnectionStatus,
    };

    use super::{ClassifiedError, SupplierApiGateway, UnavailableSupplierApiGateway};

    fn sample_connection() -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            SupplierApiConnectionData {
                supplier_id: SupplierAccountId::new("sup-1"),
                connection_code: "CN-1".to_string(),
                environment: ConnectionEnvironment::Production,
                endpoint_reference: "config://supplier/001".to_string(),
                credential_reference: Some("kms://prod/sup-001".to_string()),
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Active,
            },
            "admin-1",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn default_gateway_fails_closed_with_classified_error() {
        let gateway = UnavailableSupplierApiGateway;
        let error: ClassifiedError = gateway
            .health_check(&sample_connection())
            .await
            .expect_err("默认网关必须失败关闭");
        assert_eq!(error.class, ErrorClass::TransientFailure);
        assert_eq!(error.code, "ENDPOINT_UNRESOLVED");
    }
}
