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

use database::{AccessControlExt, NoTransaction, SupplierApiExt, SupplierExt, Transactional};
use entities::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
use entities::integration_ops::ErrorClass;
use entities::supplier_api::{
    SupplierApiCapability, SupplierApiCapabilityData, SupplierApiConnection, SupplierApiConnectionData,
    SupplierApiConnectionStatus,
};
use id_generator::next_id;
use mongodb::Database;
use validator::Validate;

use crate::audit::AuditActor;
use crate::errors::{Error, Result};
use crate::iam::SharedRbacService;
use crate::supplier_api::dto::SortDir;

mod dto;
mod governance;

pub use self::dto::{
    CapabilityItemRequest, ConfirmBusinessCapabilityRequirementCommand,
    ConfirmBusinessCapabilityRequirementResult, CreateSupplierApiConnectionRequest, HealthCheckRequest,
    HealthCheckView, PageView, RateLimitPolicyRequest, RelatedImpactView, ReplaceCapabilitiesRequest,
    SafeReferenceView, SafeReferencesView, SupplierActionBlockerView, SupplierApiCapabilityListParams,
    SupplierApiCapabilitySummaryView, SupplierApiCapabilityView, SupplierApiConnectionDetailView,
    SupplierApiConnectionListItemView, SupplierApiConnectionListParams, SupplierApiConnectionView,
    SupplierCapabilityChange, SupplierConnectionCommand, SupplierConnectionCommandResult,
    SupplierConnectionJobView, SupplierHealthCheckRunView, UpdateSupplierApiConnectionRequest,
    UpdateSupplierCapabilitiesCommand, UpdateSupplierCapabilitiesResult,
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

    /// 执行一次目录同步；实现必须保持来源幂等，并且只能写入 W21 正式供给链路。
    fn catalog_sync<'a>(
        &'a self,
        connection: &'a SupplierApiConnection,
    ) -> std::pin::Pin<
        Box<dyn std::future::Future<Output = std::result::Result<(), ClassifiedError>> + Send + 'a>,
    > {
        Box::pin(async move {
            Err(ClassifiedError {
                class: ErrorClass::TransientFailure,
                code: "CATALOG_SYNC_ADAPTER_UNAVAILABLE".to_string(),
                summary: format!("连接 {} 未注入目录同步适配器", connection.connection_code),
            })
        })
    }
}

/// 不透明引用种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SupplierReferenceKind {
    BusinessProfile,
    Endpoint,
    Credential,
}

/// 权威引用注册表解析结果。
///
/// `internal_reference` 只能写入后端配置实体，不得进入列表、详情、审计消息或日志。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedSupplierReference {
    pub internal_reference: String,
}

/// 服务端不透明引用注册表端口。
pub trait SupplierReferenceRegistry: Send + Sync {
    /// 判断当前进程是否已注入权威注册表。
    fn is_available(&self) -> bool;

    /// 解析服务端签发的短时引用；实现必须校验种类、环境、用途和有效期。
    fn resolve<'a>(
        &'a self,
        kind: SupplierReferenceKind,
        payload_reference: &'a str,
        environment: entities::supplier_api::ConnectionEnvironment,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<ResolvedSupplierReference, ClassifiedError>>
                + Send
                + 'a,
        >,
    >;
}

/// 未注入引用注册表时的默认失败关闭实现。
pub struct UnavailableSupplierReferenceRegistry;

impl SupplierReferenceRegistry for UnavailableSupplierReferenceRegistry {
    fn is_available(&self) -> bool {
        false
    }

    fn resolve<'a>(
        &'a self,
        _kind: SupplierReferenceKind,
        _payload_reference: &'a str,
        _environment: entities::supplier_api::ConnectionEnvironment,
    ) -> std::pin::Pin<
        Box<
            dyn std::future::Future<Output = std::result::Result<ResolvedSupplierReference, ClassifiedError>>
                + Send
                + 'a,
        >,
    > {
        Box::pin(async {
            Err(ClassifiedError {
                class: ErrorClass::AuthSignature,
                code: "REFERENCE_REGISTRY_UNAVAILABLE".to_string(),
                summary: "未注入权威引用注册表，引用绑定已失败关闭".to_string(),
            })
        })
    }
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
    reference_registry: Arc<dyn SupplierReferenceRegistry>,
    rbac: Option<SharedRbacService>,
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
        Self {
            db,
            gateway,
            reference_registry: Arc::new(UnavailableSupplierReferenceRegistry),
            rbac: None,
        }
    }

    /// 注入当前应用的权威 RBAC，用于动作投影与命令内二次鉴权。
    pub fn with_rbac(mut self, rbac: SharedRbacService) -> Self {
        self.rbac = Some(rbac);
        self
    }

    /// 注入启动组合根持有的权威引用注册表。
    pub fn with_reference_registry(mut self, registry: Arc<dyn SupplierReferenceRegistry>) -> Self {
        self.reference_registry = registry;
        self
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
        if req.endpoint_reference.is_some() || req.credential_reference.is_some() {
            return Err(Error::ValidationError(
                "创建连接只建立身份；技术引用必须通过不透明引用绑定命令提交".to_string(),
            ));
        }
        if req
            .status
            .is_some_and(|status| status != SupplierApiConnectionStatus::Disabled)
        {
            return Err(Error::ValidationError("新连接必须从停用状态开始".to_string()));
        }
        if !req.capabilities.is_empty() {
            return Err(Error::ValidationError(
                "初始能力必须通过独立能力配置命令提交".to_string(),
            ));
        }
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
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: req
                    .rate_limit_policy
                    .map(RateLimitPolicyRequest::into_policy)
                    .transpose()?,
                status: SupplierApiConnectionStatus::Disabled,
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
                safe_references: SafeReferencesView {
                    endpoint: SafeReferenceView {
                        state: if row.endpoint_reference_bound {
                            "BOUND"
                        } else {
                            "MISSING"
                        },
                        alias: None,
                        version: None,
                        visible: false,
                    },
                    credential: SafeReferenceView {
                        state: if row.credential_reference_bound {
                            "BOUND"
                        } else {
                            "MISSING"
                        },
                        alias: None,
                        version: None,
                        visible: false,
                    },
                },
                technical_config_version: row.technical_config_version,
                allowed_actions: Vec::new(),
                action_blockers: Vec::new(),
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
                constraint_summary: None,
                business_requirement: None,
                business_confirmation_version: None,
                technically_verified: false,
                verified_at: None,
                allowed_actions: Vec::new(),
                action_blockers: Vec::new(),
            })
            .collect();

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
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
