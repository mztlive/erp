//! 域 D25 `supplier_api` 的 DTO（Handler 直接复用，禁止在 handler 内重复定义同构类型）。
//!
//! 字段名与 HTTP 契约一致（api-contract.md）：分页参数 `page`/`page_size`/
//! `sort_by`/`sort_dir` 扁平传递；时间一律秒级时间戳。
//!
//! 敏感边界（数据模型 §6.14「不保存明文密钥」、phase-2 §14.1）：连接密钥只保存
//! 密钥管理系统引用，且引用本身不进入任何列表/详情投影（写后不回显），
//! 响应中的 `credential_reference` 一律省略。

use entities::supplier_api::{
    BusinessCapabilityRequirement, ConnectionEnvironment, HealthCheckResult, RateLimitPolicy,
    SupplierApiCapabilityCode, SupplierApiCapabilityStatus, SupplierApiConnection,
    SupplierApiConnectionStatus, SupplierCommandOutcome, SupplierConnectionAction, SupplierHealthCheckStatus,
    SupplierHealthCheckType,
};
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use validator::Validate;

use crate::errors::Result;
use crate::query::{normalized_text, page_or_default, page_size_or_default};

/// 连接列表允许的排序字段白名单（api-contract §4：Service 层校验，禁止任意字段透传）。
pub(crate) const SUPPLIER_API_CONNECTION_SORT_FIELDS: &[&str] =
    &["created_at", "connection_code", "updated_at"];
/// 能力列表允许的排序字段白名单。
pub(crate) const SUPPLIER_API_CAPABILITY_SORT_FIELDS: &[&str] = &["created_at", "updated_at"];

/// 排序方向。
pub use crate::query::SortDir;

/// 归一化后的分页查询 DTO（Service → Repository 共用）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PageParams {
    /// 页码（1 起）。
    pub page: u64,
    /// 单页条数（已 clamp 到 1–100）。
    pub page_size: u32,
    /// 排序字段（已过白名单校验，`&'static str` 保证来源只可能是白名单）。
    pub sort_by: &'static str,
    /// 排序方向。
    pub sort_dir: SortDir,
}

/// 校验排序参数（白名单 + 方向），返回归一化排序字段与方向。
///
/// # 参数
/// * `sort_by` - 可选排序字段；空白视为未提供
/// * `sort_dir` - 可选排序方向；空白视为未提供
/// * `allowed_fields` - 白名单
///
/// # 返回
/// 返回 `(排序字段, 方向)`；未提供时默认 `("created_at", Desc)`。
///
/// # 错误
/// 字段不在白名单或方向不是 `asc`/`desc` 时返回 `ValidationError`。
pub(crate) use crate::query::normalize_sort;

/// 契约目标形状的分页响应（api-contract §3）：`items` + `total` + `page` + `page_size`。
///
/// `services::Page` 只序列化 `items`/`total`（冻结），列表接口按契约在此补齐
/// `page`/`page_size`，不静默沿用 `{items,total}` 直出。
pub use crate::query::PageView;

/// 校验文本去除首尾空白后非空（validator 的 `length(min=1)` 对纯空白字符串
/// 不生效，空 code/name 需要按「空白视为空」拒绝，落入 HTTP 400）。
use crate::query::non_blank;

/// 限流策略请求值对象（对应实体 `RateLimitPolicy`）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, Validate)]
pub struct RateLimitPolicyRequest {
    /// 窗口内最大请求数。
    #[validate(range(min = 1, message = "限流最大请求数必须大于零"))]
    pub max_requests: u32,
    /// 窗口时长（秒）。
    #[validate(range(min = 1, message = "限流窗口时长必须大于零"))]
    pub window_secs: u32,
}

/// 能力声明行请求（创建连接时随连接一并写入；能力启停缺省启用）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CapabilityItemRequest {
    /// 能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 能力启停状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<SupplierApiCapabilityStatus>,
    /// 供应商能力限制快照。
    pub constraint_snapshot: Option<String>,
}

/// 供应商 API 连接创建请求（HTTP 契约：连接身份 + 能力清单原子建立）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateSupplierApiConnectionRequest {
    /// API 供应商（D09 `supplier_account`）。
    pub supplier_id: entities::ids::SupplierAccountId,
    /// ERP 内稳定连接代码（全局唯一）。
    #[validate(custom(function = "non_blank", message = "连接代码不能为空"))]
    pub connection_code: String,
    /// 连接环境。
    pub environment: ConnectionEnvironment,
    /// 地址配置引用。
    pub endpoint_reference: Option<String>,
    /// 密钥管理系统引用（不保存明文密钥）。
    pub credential_reference: Option<String>,
    /// 限流策略。
    pub rate_limit_policy: Option<RateLimitPolicyRequest>,
    /// 启停/故障状态；缺省视为启用。
    #[serde(default)]
    pub status: Option<SupplierApiConnectionStatus>,
    /// 初始能力声明清单。
    pub capabilities: Vec<CapabilityItemRequest>,
}

/// 供应商 API 连接更新请求（携带乐观锁版本，`BaseModel.version` ≡ `lock_version`）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierApiConnectionRequest {
    /// 期望的乐观锁版本；与当前版本不一致时拒绝更新（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub version: u64,
    /// 连接环境；缺省表示不修改。
    pub environment: Option<ConnectionEnvironment>,
    /// 地址配置引用；缺省表示不修改。
    pub endpoint_reference: Option<String>,
    /// 密钥管理系统引用；缺省表示不修改。
    pub credential_reference: Option<String>,
    /// 限流策略；缺省表示不修改。
    pub rate_limit_policy: Option<RateLimitPolicyRequest>,
    /// 启停/故障状态；缺省表示不修改。
    pub status: Option<SupplierApiConnectionStatus>,
    /// 最近健康检查时间；与 `last_health_result` 必须成对出现。
    pub last_health_at: Option<u64>,
    /// 最近健康检查结果；与 `last_health_at` 必须成对出现。
    pub last_health_result: Option<HealthCheckResult>,
}

/// 原子替换连接能力请求（携带期望连接版本做并发控制）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReplaceCapabilitiesRequest {
    /// 期望的连接乐观锁版本；与当前版本不一致时拒绝（409）。
    #[validate(range(min = 1, message = "乐观锁版本必须大于 0"))]
    pub expected_connection_version: u64,
    /// 替换后的能力声明清单。
    pub capabilities: Vec<CapabilityItemRequest>,
}

/// 连接列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierApiConnectionListParams {
    /// API 供应商筛选。
    pub supplier_id: Option<String>,
    /// 连接代码子串筛选（忽略大小写）。
    pub connection_code: Option<String>,
    /// 连接环境筛选。
    pub environment: Option<ConnectionEnvironment>,
    /// 启停/故障状态筛选。
    pub status: Option<SupplierApiConnectionStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`connection_code`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的连接列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupplierApiConnectionListQuery {
    /// API 供应商筛选。
    pub supplier_id: Option<String>,
    /// 连接代码子串筛选。
    pub connection_code: Option<String>,
    /// 连接环境筛选。
    pub environment: Option<ConnectionEnvironment>,
    /// 启停/故障状态筛选。
    pub status: Option<SupplierApiConnectionStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierApiConnectionListParams {
    /// 归一化连接列表查询参数。
    ///
    /// 文本筛选去首尾空白、分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SupplierApiConnectionListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_API_CONNECTION_SORT_FIELDS)?;
        Ok(SupplierApiConnectionListQuery {
            supplier_id: normalized_text(self.supplier_id.as_deref()),
            connection_code: normalized_text(self.connection_code.as_deref()),
            environment: self.environment,
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 连接能力列表查询参数（分页参数与筛选字段扁平传递）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierApiCapabilityListParams {
    /// 所属连接筛选。
    pub connection_id: Option<entities::ids::SupplierApiConnectionId>,
    /// 能力代码筛选。
    pub capability_code: Option<SupplierApiCapabilityCode>,
    /// 能力启停状态筛选。
    pub status: Option<SupplierApiCapabilityStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的能力列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct SupplierApiCapabilityListQuery {
    /// 所属连接筛选。
    pub connection_id: Option<entities::ids::SupplierApiConnectionId>,
    /// 能力代码筛选。
    pub capability_code: Option<SupplierApiCapabilityCode>,
    /// 能力启停状态筛选。
    pub status: Option<SupplierApiCapabilityStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl SupplierApiCapabilityListParams {
    /// 归一化能力列表查询参数。
    ///
    /// 分页取默认值、排序字段过白名单校验。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<SupplierApiCapabilityListQuery> {
        let (sort_by, sort_dir) =
            normalize_sort(&self.sort_by, &self.sort_dir, SUPPLIER_API_CAPABILITY_SORT_FIELDS)?;
        Ok(SupplierApiCapabilityListQuery {
            connection_id: self.connection_id.clone(),
            capability_code: self.capability_code,
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 健康检查请求（携带幂等键；重复提交由 `inbox_message` 身份唯一索引承接）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct HealthCheckRequest {
    /// 调用方幂等键（与连接 ID 组合成消息身份；重复提交不产生第二笔外部调用）。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 固定连接治理命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierConnectionCommand {
    /// 固定动作注册表；未知动作在反序列化阶段拒绝。
    pub action: SupplierConnectionAction,
    /// 期望连接版本。
    #[validate(range(min = 1, message = "连接版本必须大于0"))]
    pub expected_version: u64,
    /// 服务端签发并由权威注册表解析的不透明引用。
    pub payload_reference: Option<String>,
    /// 固定业务原因代码。
    pub reason_code: Option<String>,
    /// 健康检查白名单类型；仅 `RUN_HEALTH_CHECK` 使用。
    pub check_type: Option<SupplierHealthCheckType>,
    /// 客户端幂等键；只保存不可逆摘要。
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 采购确认业务能力需求命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ConfirmBusinessCapabilityRequirementCommand {
    pub capability_code: SupplierApiCapabilityCode,
    pub requirement: BusinessCapabilityRequirement,
    pub applicability_reference: Option<String>,
    #[serde(default)]
    pub evidence_references: Vec<String>,
    #[validate(custom(function = "non_blank", message = "原因代码不能为空"))]
    pub reason_code: String,
    #[validate(range(min = 1, message = "连接版本必须大于0"))]
    pub expected_connection_version: u64,
    #[validate(range(min = 1, message = "能力版本必须大于0"))]
    pub expected_capability_version: u64,
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    pub operation_id: String,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 单条系统管理员能力配置变化。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SupplierCapabilityChange {
    pub code: SupplierApiCapabilityCode,
    pub enabled: bool,
    pub constraint_snapshot: Option<String>,
}

/// 系统管理员能力配置强命令。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct UpdateSupplierCapabilitiesCommand {
    #[validate(length(min = 1, max = 10, message = "能力变更必须为1到10条"))]
    pub capability_changes: Vec<SupplierCapabilityChange>,
    #[validate(range(min = 1, message = "连接版本必须大于0"))]
    pub expected_connection_version: u64,
    /// 既有能力必须携带当前版本；新能力使用版本 0。
    pub expected_capability_versions: BTreeMap<String, u64>,
    #[validate(custom(function = "non_blank", message = "原因代码不能为空"))]
    pub reason_code: String,
    #[validate(custom(function = "non_blank", message = "操作ID不能为空"))]
    pub operation_id: String,
    #[validate(custom(function = "non_blank", message = "幂等键不能为空"))]
    pub idempotency_key: String,
}

/// 服务端动作阻塞原因。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierActionBlockerView {
    pub action: String,
    pub code: String,
    pub message: String,
    pub destination_workspace_id: Option<String>,
}

/// 安全引用投影；永不包含底层引用正文或可访问秘密的 URL。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SafeReferenceView {
    pub state: &'static str,
    pub alias: Option<String>,
    pub version: Option<String>,
    pub visible: bool,
}

/// 地址与密钥引用的安全状态集合；不包含底层引用正文。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SafeReferencesView {
    pub endpoint: SafeReferenceView,
    pub credential: SafeReferenceView,
}

/// 连接关联业务影响。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq, Default)]
pub struct RelatedImpactView {
    pub active_offerings: u64,
    pub active_publications: u64,
    pub open_supplier_orders: u64,
    pub active_sync_jobs: u64,
}

/// 连接响应视图（列表与详情共用；`credential_reference` 永不回显）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierApiConnectionView {
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
    /// 限流策略。
    pub rate_limit_policy: Option<RateLimitPolicyView>,
    /// 最近健康检查时间（秒级时间戳）。
    pub last_health_at: Option<u64>,
    /// 最近健康检查结果。
    pub last_health_result: Option<HealthCheckResult>,
    /// 地址与密钥引用的安全状态；永不回显底层引用。
    pub safe_references: SafeReferencesView,
    /// 当前技术配置版本。
    pub technical_config_version: u64,
    /// 服务端允许的连接治理动作。
    pub allowed_actions: Vec<String>,
    /// 服务端动作阻塞原因。
    pub action_blockers: Vec<SupplierActionBlockerView>,
    /// 乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

/// 连接列表中的最小能力摘要。
///
/// 列表只展示能力代码和启停状态；确认、技术证据与动作阻塞属于详情读模型，
/// 不得为了列表复用详情装配。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierApiCapabilitySummaryView {
    /// 固定能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 当前能力状态。
    pub status: SupplierApiCapabilityStatus,
}

/// 连接列表项读模型。
///
/// 当前页的供应商名称和能力摘要由后端批量补齐，客户端不得再逐行请求详情或
/// 拉取独立能力大页后自行关联。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierApiConnectionListItemView {
    /// 连接列表基础字段。
    #[serde(flatten)]
    pub connection: SupplierApiConnectionView,
    /// 当前供应商主体名称；主数据不完整时为空，由界面明确回退到供应商 ID。
    pub supplier_name: Option<String>,
    /// 当前连接的能力摘要。
    pub capabilities: Vec<SupplierApiCapabilitySummaryView>,
}

/// 限流策略响应视图。
#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
pub struct RateLimitPolicyView {
    /// 窗口内最大请求数。
    pub max_requests: u32,
    /// 窗口时长（秒）。
    pub window_secs: u32,
}

/// 连接详情响应视图（连接身份 + 能力清单）。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierApiConnectionDetailView {
    /// 连接身份（列表同款字段）。
    #[serde(flatten)]
    pub connection: SupplierApiConnectionView,
    /// 连接下的能力声明清单。
    pub capabilities: Vec<SupplierApiCapabilityView>,
    /// 最近健康检查运行记录。
    pub health_records: Vec<SupplierHealthCheckRunView>,
    /// 服务端健康检查白名单。
    pub health_check_types: Vec<SupplierHealthCheckType>,
    /// 停用前关联业务影响。
    pub related_impact: RelatedImpactView,
}

/// 能力响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierApiCapabilityView {
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
    /// 服务端安全约束摘要。
    pub constraint_summary: Option<String>,
    /// 最新采购业务需求确认；没有确认时为 `None`。
    pub business_requirement: Option<BusinessCapabilityRequirement>,
    /// 最新确认记录版本。
    pub business_confirmation_version: Option<u64>,
    /// 当前能力版本是否已包含在最近成功技术健康证据中。
    pub technically_verified: bool,
    /// 最近技术验证时间。
    pub verified_at: Option<u64>,
    /// 能力对象允许动作。
    pub allowed_actions: Vec<String>,
    /// 能力对象动作阻塞原因。
    pub action_blockers: Vec<SupplierActionBlockerView>,
}

/// 后台健康检查运行记录视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierHealthCheckRunView {
    pub id: String,
    pub job_id: String,
    pub check_type: SupplierHealthCheckType,
    pub status: SupplierHealthCheckStatus,
    pub technical_config_version: u64,
    pub requested_by: String,
    pub started_at: Option<u64>,
    pub finished_at: Option<u64>,
    pub latency_ms: Option<u64>,
    pub error_code: Option<String>,
    pub error_summary: Option<String>,
}

/// 后台任务查询视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierConnectionJobView {
    pub job_id: String,
    pub job_no: String,
    pub action: String,
    pub status: entities::bulk_job::JobStatus,
    pub total: u64,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub error_summary: Option<String>,
    pub created_at: u64,
    pub finished_at: Option<u64>,
}

/// 正式连接治理命令结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierConnectionCommandResult {
    pub outcome: SupplierCommandOutcome,
    pub action: SupplierConnectionAction,
    pub operation_id: String,
    pub connection_version: u64,
    pub job_id: Option<String>,
    pub job_no: Option<String>,
    pub audit_event_id: String,
}

/// 采购业务能力确认结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct ConfirmBusinessCapabilityRequirementResult {
    pub outcome: SupplierCommandOutcome,
    pub operation_id: String,
    pub confirmation_id: String,
    pub confirmation_version: u64,
    pub connection_version: u64,
    pub capability_version: u64,
    pub audit_event_id: String,
}

/// 系统管理员能力配置结果。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct UpdateSupplierCapabilitiesResult {
    pub outcome: SupplierCommandOutcome,
    pub operation_id: String,
    pub connection_version: u64,
    pub capabilities: Vec<SupplierApiCapabilityView>,
    pub audit_event_id: String,
}

/// 健康检查结果视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct HealthCheckView {
    /// 最近健康检查时间（秒级时间戳）。
    pub checked_at: u64,
    /// 最近健康检查结果。
    pub result: HealthCheckResult,
    /// 承接本次调用的消息信封 ID（`inbox_message`）。
    pub inbox_message_id: String,
    /// 失败时创建的集成错误任务 ID；成功路径为 `None`。
    pub error_task_id: Option<String>,
    /// 连接乐观锁版本（`BaseModel.version` ≡ 数据模型 `lock_version`）。
    pub version: u64,
}

impl From<SupplierApiConnection> for SupplierApiConnectionView {
    /// 从实体构造响应视图（省略密钥管理系统引用）。
    ///
    /// # 参数
    /// * `connection` - 连接实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(connection: SupplierApiConnection) -> Self {
        Self {
            id: connection.base.id,
            supplier_id: connection.supplier_id.to_string(),
            connection_code: connection.connection_code,
            environment: connection.environment,
            status: connection.stable.status,
            rate_limit_policy: connection.rate_limit_policy.map(|policy| RateLimitPolicyView {
                max_requests: policy.max_requests(),
                window_secs: policy.window_secs(),
            }),
            last_health_at: connection.last_health_at.map(|at| at.unix_secs() as u64),
            last_health_result: connection.last_health_result,
            safe_references: SafeReferencesView {
                endpoint: SafeReferenceView {
                    state: if connection.endpoint_reference_bound {
                        "BOUND"
                    } else {
                        "MISSING"
                    },
                    alias: None,
                    version: None,
                    visible: false,
                },
                credential: SafeReferenceView {
                    state: if connection.credential_reference_bound {
                        "BOUND"
                    } else {
                        "MISSING"
                    },
                    alias: None,
                    version: None,
                    visible: false,
                },
            },
            technical_config_version: connection.technical_config_version,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            version: connection.base.version,
            created_at: connection.base.created_at,
        }
    }
}

impl RateLimitPolicyRequest {
    /// 转换为实体限流策略值对象。
    ///
    /// # 参数
    /// * `request` - 请求值对象
    ///
    /// # 返回
    /// 返回实体值对象。
    pub(crate) fn into_policy(request: RateLimitPolicyRequest) -> Result<RateLimitPolicy> {
        RateLimitPolicy::new(request.max_requests, request.window_secs).map_err(Into::into)
    }
}

#[cfg(test)]
mod tests {
    use super::{normalize_sort, SortDir, SupplierApiConnectionListParams};
    use entities::supplier_api::{ConnectionEnvironment, SupplierApiConnectionStatus};
    use serde_json::json;
    use validator::Validate;

    #[test]
    fn sort_whitelist_rejects_unknown_fields_and_directions() {
        assert!(normalize_sort(
            &Some("status".to_string()),
            &None,
            &["created_at", "connection_code"]
        )
        .is_err());
        assert!(normalize_sort(&None, &Some("up".to_string()), &["created_at"]).is_err());

        let (field, direction) = normalize_sort(
            &Some(" connection_code ".to_string()),
            &Some(" asc ".to_string()),
            &["created_at", "connection_code"],
        )
        .unwrap();
        assert_eq!(field, "connection_code");
        assert_eq!(direction, SortDir::Asc);
    }

    #[test]
    fn list_params_normalize_paging_filters_and_sort_defaults() {
        let params = SupplierApiConnectionListParams {
            supplier_id: Some(" sup-1 ".to_string()),
            connection_code: Some(" CN-1 ".to_string()),
            environment: Some(ConnectionEnvironment::Production),
            status: Some(SupplierApiConnectionStatus::Active),
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        };
        let query = params.normalized().unwrap();
        assert_eq!(query.supplier_id.as_deref(), Some("sup-1"));
        assert_eq!(query.connection_code.as_deref(), Some("CN-1"));
        assert_eq!(query.environment, Some(ConnectionEnvironment::Production));
        assert_eq!(query.paging.page, 1);
        assert_eq!(query.paging.page_size, 20);
        assert_eq!(query.paging.sort_by, "created_at");
        assert_eq!(query.paging.sort_dir, SortDir::Desc);
    }

    #[test]
    fn list_params_reject_unbounded_page_size() {
        let params = SupplierApiConnectionListParams {
            supplier_id: None,
            connection_code: None,
            environment: None,
            status: None,
            page: Some(0),
            page_size: Some(u32::MAX),
            sort_by: None,
            sort_dir: None,
        };
        assert!(params.validate().is_err());
    }

    #[test]
    fn rate_limit_policy_request_rejects_zero_bounds() {
        let invalid: super::RateLimitPolicyRequest =
            serde_json::from_value(json!({ "max_requests": 0, "window_secs": 60 })).unwrap();
        assert!(invalid.validate().is_err());
    }

    #[test]
    fn connection_view_omits_credential_reference() {
        let view = serde_json::to_value(super::SupplierApiConnectionView {
            id: "conn-1".to_string(),
            supplier_id: "sup-1".to_string(),
            connection_code: "CN-1".to_string(),
            environment: ConnectionEnvironment::Production,
            status: SupplierApiConnectionStatus::Active,
            rate_limit_policy: None,
            last_health_at: None,
            last_health_result: None,
            safe_references: super::SafeReferencesView {
                endpoint: super::SafeReferenceView {
                    state: "MISSING",
                    alias: None,
                    version: None,
                    visible: false,
                },
                credential: super::SafeReferenceView {
                    state: "BOUND",
                    alias: None,
                    version: None,
                    visible: false,
                },
            },
            technical_config_version: 1,
            allowed_actions: Vec::new(),
            action_blockers: Vec::new(),
            version: 1,
            created_at: 1_700_000_000,
        })
        .unwrap();
        assert!(view.get("credential_reference").is_none());
        assert_eq!(view["safe_references"]["credential"]["state"], "BOUND");
        assert_eq!(view["connection_code"], "CN-1");
    }
}
