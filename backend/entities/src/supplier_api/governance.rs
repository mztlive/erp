//! W20 供应商连接治理的追加式事实与幂等回执。
//!
//! 采购业务确认、技术健康检查和治理命令是三类独立事实。业务确认不得修改
//! 能力启停；健康检查任务不得在 HTTP 请求内伪装完成；命令回执只保存不可逆
//! 幂等摘要，不保存密钥、地址正文或客户端原始幂等键。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierApiCapabilityId, SupplierApiConnectionId};
use crate::supplier_api::{
    ConnectionEnvironment, SupplierApiCapability, SupplierApiCapabilityCode, SupplierApiConnection,
    SupplierApiConnectionStatus,
};
use crate::validation::{normalize_optional_text, normalize_required_text};

const REFERENCE_MAX_LEN: usize = 512;
const REASON_MAX_LEN: usize = 128;
const OPERATION_ID_MAX_LEN: usize = 128;
const HASH_MAX_LEN: usize = 128;
const ACTOR_ID_MAX_LEN: usize = 128;
const ERROR_CODE_MAX_LEN: usize = 128;
const ERROR_SUMMARY_MAX_LEN: usize = 1024;
const MAX_EVIDENCE_REFERENCES: usize = 20;

/// 采购确认的业务能力需求结论。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum BusinessCapabilityRequirement {
    /// 该连接业务上必须具备此能力。
    Required,
    /// 当前业务范围不需要此能力。
    NotRequired,
}

/// 追加式采购业务能力确认创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct BusinessCapabilityConfirmationData {
    /// 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 被确认的能力实体。
    pub capability_id: SupplierApiCapabilityId,
    /// 被确认的固定能力代码。
    pub capability_code: SupplierApiCapabilityCode,
    /// 需求结论。
    pub requirement: BusinessCapabilityRequirement,
    /// 适用范围的不透明业务引用。
    pub applicability_reference: Option<String>,
    /// 追加证据引用；只保存引用，不保存证据正文。
    pub evidence_references: Vec<String>,
    /// 固定原因代码。
    pub reason_code: String,
    /// 提交时连接版本。
    pub connection_version: u64,
    /// 提交时能力版本。
    pub capability_version: u64,
    /// 稳定操作 ID。
    pub operation_id: String,
    /// 客户端幂等键的不可逆摘要。
    pub idempotency_key_hash: String,
    /// 完整请求的不可逆摘要，用于拒绝同键异参。
    pub request_fingerprint: String,
    /// 采购确认人。
    pub confirmed_by: String,
    /// 确认时间。
    pub confirmed_at: Instant,
}

/// 采购业务能力确认事实。
///
/// 同一连接/能力可持续追加确认；最新确认由时间和主键稳定排序决定。该实体没有
/// 更新入口，采购确认不能借此修改 [`crate::supplier_api::SupplierApiCapability`]。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct BusinessCapabilityConfirmation {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    pub capability_id: SupplierApiCapabilityId,
    pub capability_code: SupplierApiCapabilityCode,
    pub requirement: BusinessCapabilityRequirement,
    pub applicability_reference: Option<String>,
    pub evidence_references: Vec<String>,
    pub reason_code: String,
    pub connection_version: u64,
    pub capability_version: u64,
    pub operation_id: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
    pub confirmed_by: String,
    pub confirmed_at: Instant,
}

impl BusinessCapabilityConfirmation {
    /// 构造不可变业务确认事实。
    ///
    /// # Errors
    /// 必填字段为空、引用过长、证据超限或对象版本为零时返回错误。
    pub fn new(id: impl Into<String>, data: BusinessCapabilityConfirmationData) -> Result<Self> {
        if data.connection_version == 0 || data.capability_version == 0 {
            return Err(Error::from("业务确认的对象版本必须大于零"));
        }
        if data.evidence_references.len() > MAX_EVIDENCE_REFERENCES {
            return Err(Error::from("业务确认的证据引用不能超过20条"));
        }
        let applicability_reference =
            normalize_optional_text(data.applicability_reference, "适用范围引用", REFERENCE_MAX_LEN)?;
        let mut evidence_references = Vec::with_capacity(data.evidence_references.len());
        for reference in data.evidence_references {
            let normalized =
                normalize_required_text(reference, "证据引用不能为空", REFERENCE_MAX_LEN, "证据引用过长")?;
            if !evidence_references.contains(&normalized) {
                evidence_references.push(normalized);
            }
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id: data.connection_id,
            capability_id: data.capability_id,
            capability_code: data.capability_code,
            requirement: data.requirement,
            applicability_reference,
            evidence_references,
            reason_code: required(data.reason_code, "原因代码", REASON_MAX_LEN)?,
            connection_version: data.connection_version,
            capability_version: data.capability_version,
            operation_id: required(data.operation_id, "操作ID", OPERATION_ID_MAX_LEN)?,
            idempotency_key_hash: required(data.idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(data.request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            confirmed_by: required(data.confirmed_by, "确认人", ACTOR_ID_MAX_LEN)?,
            confirmed_at: data.confirmed_at,
        })
    }

    /// 判断该确认是否仍覆盖当前能力配置。
    ///
    /// 能力必须被确认为业务必需，且确认版本等于当前能力版本或仅落后一版
    /// （同一命令启用能力会使版本递增一次）。
    ///
    /// # 参数
    /// * `capability` - 当前连接能力实体
    ///
    /// # 返回
    /// 采购确认仍可用于当前能力配置时返回 `true`。
    pub fn covers(&self, capability: &SupplierApiCapability) -> bool {
        if self.capability_code != capability.capability_code
            || self.requirement != BusinessCapabilityRequirement::Required
        {
            return false;
        }
        self.capability_version == capability.base.version
            || self.capability_version.checked_add(1) == Some(capability.base.version)
    }

    /// 从最新优先历史中返回指定能力代码的最近确认。
    ///
    /// # 参数
    /// * `confirmations` - 最新确认优先的追加式历史
    /// * `capability_code` - 固定能力代码
    ///
    /// # 返回
    /// 返回首个匹配确认；没有时返回 `None`。
    pub fn latest_for(confirmations: &[Self], capability_code: SupplierApiCapabilityCode) -> Option<&Self> {
        confirmations
            .iter()
            .find(|confirmation| confirmation.capability_code == capability_code)
    }
}

/// W20 健康检查固定白名单。
///
/// 所有检查都只能读取技术元数据，不允许创建真实订单、取消或退款。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierHealthCheckType {
    /// 检查端点可达性与协议握手。
    Connectivity,
    /// 检查认证引用能否完成只读鉴权。
    Authentication,
    /// 检查连接声明的只读能力元数据。
    CapabilityMetadata,
}

/// 健康检查任务状态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierHealthCheckStatus {
    Pending,
    Running,
    Succeeded,
    Failed,
    Unknown,
}

impl SupplierHealthCheckStatus {
    /// 判断检查是否已经形成不可再推进的终态。
    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Unknown)
    }
}

/// 健康检查创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierHealthCheckRunData {
    pub connection_id: SupplierApiConnectionId,
    pub background_job_id: String,
    pub check_type: SupplierHealthCheckType,
    pub technical_config_version: u64,
    pub capability_versions: Vec<CapabilityVersionSnapshot>,
    pub requested_by: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
}

/// 检查开始时冻结的能力版本。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CapabilityVersionSnapshot {
    pub capability_code: SupplierApiCapabilityCode,
    pub version: u64,
}

/// 后台健康检查运行记录与技术健康证据。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierHealthCheckRun {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    pub background_job_id: String,
    pub check_type: SupplierHealthCheckType,
    pub status: SupplierHealthCheckStatus,
    pub technical_config_version: u64,
    pub capability_versions: Vec<CapabilityVersionSnapshot>,
    pub requested_by: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
    pub started_at: Option<Instant>,
    pub finished_at: Option<Instant>,
    pub latency_ms: Option<u64>,
    pub error_code: Option<String>,
    pub error_summary: Option<String>,
}

impl SupplierHealthCheckRun {
    /// 创建等待执行的健康检查记录。
    ///
    /// # Errors
    /// 任务/操作人/摘要为空或技术配置版本为零时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierHealthCheckRunData) -> Result<Self> {
        if data.technical_config_version == 0 {
            return Err(Error::from("技术配置版本必须大于零"));
        }
        if data.capability_versions.iter().any(|item| item.version == 0) {
            return Err(Error::from("能力版本必须大于零"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id: data.connection_id,
            background_job_id: required(data.background_job_id, "后台任务ID", OPERATION_ID_MAX_LEN)?,
            check_type: data.check_type,
            status: SupplierHealthCheckStatus::Pending,
            technical_config_version: data.technical_config_version,
            capability_versions: data.capability_versions,
            requested_by: required(data.requested_by, "检查发起人", ACTOR_ID_MAX_LEN)?,
            idempotency_key_hash: required(data.idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(data.request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            started_at: None,
            finished_at: None,
            latency_ms: None,
            error_code: None,
            error_summary: None,
        })
    }

    /// 标记后台任务开始执行。
    ///
    /// # Errors
    /// 非等待状态不允许重复开始。
    pub fn start(&mut self, at: Instant) -> Result<()> {
        if self.status != SupplierHealthCheckStatus::Pending {
            return Err(Error::from("只有等待执行的健康检查可以开始"));
        }
        self.status = SupplierHealthCheckStatus::Running;
        self.started_at = Some(at);
        Ok(())
    }

    /// 形成成功技术健康证据。
    ///
    /// # Errors
    /// 非执行中状态不允许形成成功结果。
    pub fn succeed(&mut self, at: Instant, latency_ms: u64) -> Result<()> {
        self.finish(SupplierHealthCheckStatus::Succeeded, at, latency_ms, None, None)
    }

    /// 形成明确失败技术健康证据。
    ///
    /// # Errors
    /// 非执行中状态或错误字段非法时返回错误。
    pub fn fail(
        &mut self,
        at: Instant,
        latency_ms: u64,
        error_code: String,
        error_summary: String,
    ) -> Result<()> {
        self.finish(
            SupplierHealthCheckStatus::Failed,
            at,
            latency_ms,
            Some(error_code),
            Some(error_summary),
        )
    }

    /// 形成结果未知证据；调用方不得把它视为成功或自动重试依据。
    ///
    /// # Errors
    /// 非执行中状态或错误字段非法时返回错误。
    pub fn mark_unknown(
        &mut self,
        at: Instant,
        latency_ms: u64,
        error_code: String,
        error_summary: String,
    ) -> Result<()> {
        self.finish(
            SupplierHealthCheckStatus::Unknown,
            at,
            latency_ms,
            Some(error_code),
            Some(error_summary),
        )
    }

    /// 判断成功运行是否验证了当前能力版本。
    ///
    /// # 参数
    /// * `capability` - 当前连接能力实体
    ///
    /// # 返回
    /// 运行成功且冻结快照包含完全一致的能力代码和版本时返回 `true`。
    pub fn verifies(&self, capability: &SupplierApiCapability) -> bool {
        self.status == SupplierHealthCheckStatus::Succeeded
            && self.capability_versions.iter().any(|snapshot| {
                snapshot.capability_code == capability.capability_code
                    && snapshot.version == capability.base.version
            })
    }

    /// 将执行中的健康检查收敛到指定终态。
    ///
    /// # 参数
    /// * `status` - 成功、失败或结果未知终态
    /// * `at` - 完成时间
    /// * `latency_ms` - 执行耗时毫秒数
    /// * `error_code` - 可选稳定错误代码
    /// * `error_summary` - 可选错误摘要
    ///
    /// # 返回
    /// 状态和错误字段合法时返回 `Ok(())`。
    ///
    /// # 错误
    /// 当前记录非执行中、目标非终态或错误字段非法时返回错误。
    fn finish(
        &mut self,
        status: SupplierHealthCheckStatus,
        at: Instant,
        latency_ms: u64,
        error_code: Option<String>,
        error_summary: Option<String>,
    ) -> Result<()> {
        if self.status != SupplierHealthCheckStatus::Running || !status.is_terminal() {
            return Err(Error::from("只有执行中的健康检查可以写入终态"));
        }
        self.error_code = error_code
            .map(|value| required(value, "错误代码", ERROR_CODE_MAX_LEN))
            .transpose()?;
        self.error_summary = normalize_optional_text(error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        self.status = status;
        self.finished_at = Some(at);
        self.latency_ms = Some(latency_ms);
        Ok(())
    }
}

/// 供应商连接固定治理动作注册表。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierConnectionAction {
    UpdateBusinessProfile,
    BindEndpointReference,
    BindCredentialReference,
    RunHealthCheck,
    Enable,
    Disable,
    StartCatalogSync,
}

impl SupplierConnectionAction {
    /// 返回稳定动作代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::UpdateBusinessProfile => "UPDATE_BUSINESS_PROFILE",
            Self::BindEndpointReference => "BIND_ENDPOINT_REFERENCE",
            Self::BindCredentialReference => "BIND_CREDENTIAL_REFERENCE",
            Self::RunHealthCheck => "RUN_HEALTH_CHECK",
            Self::Enable => "ENABLE",
            Self::Disable => "DISABLE",
            Self::StartCatalogSync => "START_CATALOG_SYNC",
        }
    }

    /// 返回固定注册表全部动作。
    pub const fn all() -> [Self; 7] {
        [
            Self::UpdateBusinessProfile,
            Self::BindEndpointReference,
            Self::BindCredentialReference,
            Self::RunHealthCheck,
            Self::Enable,
            Self::Disable,
            Self::StartCatalogSync,
        ]
    }
}

/// 停用连接前需要重验的活动业务影响计数。
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct SupplierConnectionBusinessImpact {
    /// 活动供给数量。
    pub active_offerings: u64,
    /// 活动发布数量。
    pub active_publications: u64,
    /// 未完成供应商订单数量。
    pub open_supplier_orders: u64,
    /// 活动目录同步任务数量。
    pub active_sync_jobs: u64,
}

impl SupplierConnectionBusinessImpact {
    /// 判断是否存在任何必须先处理的活动业务对象。
    ///
    /// # 返回
    /// 任一计数大于零时返回 `true`。
    pub fn has_blockers(self) -> bool {
        self.active_offerings > 0
            || self.active_publications > 0
            || self.open_supplier_orders > 0
            || self.active_sync_jobs > 0
    }
}

/// 连接治理动作的稳定阻塞原因。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierGovernanceBlocker {
    /// 被阻塞动作。
    pub action: SupplierConnectionAction,
    /// 稳定原因代码。
    pub code: &'static str,
    /// 可展示说明。
    pub message: String,
    /// 可跳转工作区。
    pub destination_workspace_id: Option<&'static str>,
}

/// 连接、能力、采购确认和技术健康证据组成的纯治理上下文。
pub struct SupplierConnectionGovernance<'a> {
    /// 当前连接。
    pub connection: &'a SupplierApiConnection,
    /// 当前连接能力。
    pub capabilities: &'a [SupplierApiCapability],
    /// 最新优先的采购确认历史。
    pub confirmations: &'a [BusinessCapabilityConfirmation],
    /// 最新优先的健康检查历史。
    pub health_runs: &'a [SupplierHealthCheckRun],
}

impl SupplierConnectionGovernance<'_> {
    /// 返回指定治理动作的全部阻塞原因。
    ///
    /// # 参数
    /// * `action` - 待评估动作
    /// * `impact` - 停用前活动业务影响计数
    /// * `reference_registry_available` - 权威引用注册表是否可用
    ///
    /// # 返回
    /// 返回稳定阻塞原因；空集合表示领域前置条件满足。
    pub fn blockers(
        &self,
        action: SupplierConnectionAction,
        impact: SupplierConnectionBusinessImpact,
        reference_registry_available: bool,
    ) -> Vec<SupplierGovernanceBlocker> {
        match action {
            SupplierConnectionAction::UpdateBusinessProfile
            | SupplierConnectionAction::BindEndpointReference
            | SupplierConnectionAction::BindCredentialReference => {
                self.reference_change_blockers(action, reference_registry_available)
            }
            SupplierConnectionAction::RunHealthCheck => self.health_check_blockers(action),
            SupplierConnectionAction::Enable => self.enable_blockers(action),
            SupplierConnectionAction::Disable => self.disable_blockers(action, impact),
            SupplierConnectionAction::StartCatalogSync => self.catalog_sync_blockers(action),
        }
    }

    /// 返回最近一次成功健康检查。
    ///
    /// # 返回
    /// 返回最新优先历史中的首个成功运行；没有时返回 `None`。
    pub fn latest_successful_health_run(&self) -> Option<&SupplierHealthCheckRun> {
        self.health_runs
            .iter()
            .find(|run| run.status == SupplierHealthCheckStatus::Succeeded)
    }

    /// 返回指定能力代码最近一次采购确认。
    ///
    /// # 参数
    /// * `capability_code` - 固定能力代码
    ///
    /// # 返回
    /// 返回最新优先历史中的首个匹配确认；没有时返回 `None`。
    pub fn latest_confirmation(
        &self,
        capability_code: SupplierApiCapabilityCode,
    ) -> Option<&BusinessCapabilityConfirmation> {
        BusinessCapabilityConfirmation::latest_for(self.confirmations, capability_code)
    }

    /// 返回连接引用或业务资料变更的阻塞原因。
    ///
    /// # 参数
    /// * `action` - 待评估的引用变更动作
    /// * `reference_registry_available` - 权威引用注册表是否可用
    ///
    /// # 返回
    /// 连接仍启用或注册表不可用时返回对应阻塞原因。
    fn reference_change_blockers(
        &self,
        action: SupplierConnectionAction,
        reference_registry_available: bool,
    ) -> Vec<SupplierGovernanceBlocker> {
        let mut blockers = Vec::new();
        if self.connection.is_active() {
            blockers.push(governance_blocker(
                action,
                "CONNECTION_ENABLED",
                "请先停用连接，再变更配置",
                None,
            ));
        }
        if !reference_registry_available {
            blockers.push(governance_blocker(
                action,
                "REFERENCE_REGISTRY_UNAVAILABLE",
                "权威引用注册表未接入，不能绑定或轮换引用",
                Some("W19"),
            ));
        }
        blockers
    }

    /// 返回执行技术健康检查前的阻塞原因。
    ///
    /// # 参数
    /// * `action` - 健康检查治理动作
    ///
    /// # 返回
    /// 地址或密钥引用未就绪时返回阻塞原因，否则返回空集合。
    fn health_check_blockers(&self, action: SupplierConnectionAction) -> Vec<SupplierGovernanceBlocker> {
        if self.connection.technical_references_ready() {
            return Vec::new();
        }
        vec![governance_blocker(
            action,
            "TECHNICAL_REFERENCES_MISSING",
            "地址与密钥引用均绑定后才能执行健康检查",
            None,
        )]
    }

    /// 返回停用连接前的阻塞原因。
    ///
    /// # 参数
    /// * `action` - 停用治理动作
    /// * `impact` - 当前活动业务影响计数
    ///
    /// # 返回
    /// 已停用或仍有活动业务对象时返回阻塞原因，否则返回空集合。
    fn disable_blockers(
        &self,
        action: SupplierConnectionAction,
        impact: SupplierConnectionBusinessImpact,
    ) -> Vec<SupplierGovernanceBlocker> {
        if self.connection.stable.status == SupplierApiConnectionStatus::Disabled {
            return vec![governance_blocker(
                action,
                "ALREADY_DISABLED",
                "连接已经停用",
                None,
            )];
        }
        if impact.has_blockers() {
            return vec![governance_blocker(
                action,
                "ACTIVE_BUSINESS_IMPACT",
                &format!(
                    "仍有{}条供给、{}条发布、{}张订单和{}个同步任务受影响",
                    impact.active_offerings,
                    impact.active_publications,
                    impact.open_supplier_orders,
                    impact.active_sync_jobs
                ),
                Some("W21"),
            )];
        }
        Vec::new()
    }

    /// 返回启用连接前的阻塞原因。
    ///
    /// # 参数
    /// * `action` - 启用治理动作
    ///
    /// # 返回
    /// 返回首个稳定阻塞原因；连接、能力、采购确认和技术证据均满足时返回空集合。
    fn enable_blockers(&self, action: SupplierConnectionAction) -> Vec<SupplierGovernanceBlocker> {
        if self.connection.is_active() {
            return vec![governance_blocker(
                action,
                "ALREADY_ENABLED",
                "连接已经启用",
                None,
            )];
        }
        if !self.connection.technical_references_ready() {
            return vec![governance_blocker(
                action,
                "TECHNICAL_REFERENCES_MISSING",
                "地址与密钥引用尚未全部绑定",
                None,
            )];
        }
        let active: Vec<&SupplierApiCapability> = self
            .capabilities
            .iter()
            .filter(|capability| capability.is_active())
            .collect();
        if active.is_empty() {
            return vec![governance_blocker(
                action,
                "NO_ACTIVE_CAPABILITY",
                "至少启用一项连接能力",
                None,
            )];
        }
        if active.iter().any(|capability| {
            !self
                .latest_confirmation(capability.capability_code)
                .is_some_and(|confirmation| confirmation.covers(capability))
        }) {
            return vec![governance_blocker(
                action,
                "BUSINESS_CONFIRMATION_MISSING",
                "启用能力必须有与当前配置匹配的采购业务确认",
                None,
            )];
        }
        let Some(health) = self.latest_successful_health_run() else {
            return vec![governance_blocker(
                action,
                "TECHNICAL_HEALTH_MISSING",
                "当前技术配置尚无成功健康检查证据",
                None,
            )];
        };
        if health.technical_config_version != self.connection.technical_config_version
            || active.iter().any(|capability| !health.verifies(capability))
        {
            return vec![governance_blocker(
                action,
                "TECHNICAL_HEALTH_STALE",
                "技术配置或能力已变化，请重新执行健康检查",
                None,
            )];
        }
        if self.connection.environment == ConnectionEnvironment::Production
            && active.iter().any(|capability| {
                self.latest_confirmation(capability.capability_code)
                    .is_some_and(|confirmation| confirmation.confirmed_by == health.requested_by)
            })
        {
            return vec![governance_blocker(
                action,
                "PRODUCTION_DUAL_ROLE_REQUIRED",
                "生产环境必须由不同人员完成采购业务确认与技术健康检查",
                None,
            )];
        }
        Vec::new()
    }

    /// 返回启动目录同步前的阻塞原因。
    ///
    /// # 参数
    /// * `action` - 目录同步治理动作
    ///
    /// # 返回
    /// 连接未启用、技术配置不健康或商品能力未启用时返回阻塞原因。
    fn catalog_sync_blockers(&self, action: SupplierConnectionAction) -> Vec<SupplierGovernanceBlocker> {
        if !self.connection.is_active() {
            return vec![governance_blocker(
                action,
                "CONNECTION_NOT_ENABLED",
                "连接启用后才能同步目录",
                None,
            )];
        }
        if !self.connection.current_technical_config_is_healthy() {
            return vec![governance_blocker(
                action,
                "TECHNICAL_HEALTH_MISSING",
                "当前技术配置健康后才能同步目录",
                None,
            )];
        }
        if !self.capabilities.iter().any(|capability| {
            capability.capability_code == SupplierApiCapabilityCode::Product && capability.is_active()
        }) {
            return vec![governance_blocker(
                action,
                "CATALOG_CAPABILITY_DISABLED",
                "商品目录能力尚未启用",
                None,
            )];
        }
        Vec::new()
    }
}

/// 校验能力代码集合不包含重复项。
///
/// # 参数
/// * `codes` - 待更新的固定能力代码
///
/// # 返回
/// 所有代码唯一时返回 `Ok(())`。
///
/// # 错误
/// 任一代码重复时返回业务错误。
pub fn ensure_unique_capability_codes(
    codes: impl IntoIterator<Item = SupplierApiCapabilityCode>,
) -> Result<()> {
    let mut seen = std::collections::HashSet::new();
    if codes.into_iter().any(|code| !seen.insert(code)) {
        return Err(Error::from("能力变更代码不能重复"));
    }
    Ok(())
}

/// 构造稳定连接治理阻塞原因。
///
/// # 参数
/// * `action` - 被阻塞的治理动作
/// * `code` - 稳定原因代码
/// * `message` - 面向用户的阻塞说明
/// * `destination_workspace_id` - 可选处理工作区
///
/// # 返回
/// 返回完整阻塞原因值对象。
fn governance_blocker(
    action: SupplierConnectionAction,
    code: &'static str,
    message: &str,
    destination_workspace_id: Option<&'static str>,
) -> SupplierGovernanceBlocker {
    SupplierGovernanceBlocker {
        action,
        code,
        message: message.to_string(),
        destination_workspace_id,
    }
}

/// 命令回执终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierCommandOutcome {
    Succeeded,
    Processing,
    Rejected,
    Unknown,
}

/// 供应商连接命令回执创建数据。
///
/// # 用途
/// 将回执构造所需字段打包，供 [`SupplierConnectionCommandReceipt::new`] 一次性接收。
///
/// # 参数
/// 无
///
/// # 返回
/// 无
///
/// # 错误
/// 无
///
/// # 关键业务约束
/// 本结构不校验；连接版本、操作人、幂等摘要、请求摘要与审计号由构造函数校验。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierConnectionCommandReceiptData {
    /// 连接。
    pub connection_id: SupplierApiConnectionId,
    /// 命令动作。
    pub action: SupplierConnectionAction,
    /// 操作人。
    pub actor_id: String,
    /// 客户端幂等键的不可逆摘要。
    pub idempotency_key_hash: String,
    /// 完整请求的不可逆摘要。
    pub request_fingerprint: String,
    /// 命令终态。
    pub outcome: SupplierCommandOutcome,
    /// 提交时连接版本。
    pub connection_version: u64,
    /// 后台任务 ID。
    pub job_id: Option<String>,
    /// 审计号。
    pub audit_event_id: String,
}

/// 供应商连接命令幂等回执。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct SupplierConnectionCommandReceipt {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: SupplierApiConnectionId,
    pub action: SupplierConnectionAction,
    pub actor_id: String,
    pub idempotency_key_hash: String,
    pub request_fingerprint: String,
    pub outcome: SupplierCommandOutcome,
    pub connection_version: u64,
    pub job_id: Option<String>,
    pub audit_event_id: String,
}

impl SupplierConnectionCommandReceipt {
    /// 构造不可变命令回执。
    ///
    /// # 用途
    /// 校验并规范化回执字段后创建不可变实体。
    ///
    /// # 参数
    /// * `id` - 回执主键
    /// * `data` - 回执字段
    ///
    /// # 返回
    /// 校验通过后的命令回执。
    ///
    /// # 错误
    /// 必填摘要、审计号为空或连接版本为零时返回错误。
    ///
    /// # 关键业务约束
    /// 连接版本必须大于零；不保存客户端原始幂等键或密钥正文。
    pub fn new(id: impl Into<String>, data: SupplierConnectionCommandReceiptData) -> Result<Self> {
        if data.connection_version == 0 {
            return Err(Error::from("命令回执的连接版本必须大于零"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id: data.connection_id,
            action: data.action,
            actor_id: required(data.actor_id, "操作人", ACTOR_ID_MAX_LEN)?,
            idempotency_key_hash: required(data.idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(data.request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            outcome: data.outcome,
            connection_version: data.connection_version,
            job_id: normalize_optional_text(data.job_id, "后台任务ID", OPERATION_ID_MAX_LEN)?,
            audit_event_id: required(data.audit_event_id, "审计号", OPERATION_ID_MAX_LEN)?,
        })
    }
}

fn required(value: String, label: &str, max_len: usize) -> Result<String> {
    normalize_required_text(
        value,
        &format!("{label}不能为空"),
        max_len,
        &format!("{label}过长"),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::supplier_api::HealthCheckResult;

    /// 构造已绑定技术引用的连接治理测试夹具。
    fn governance_connection(
        status: SupplierApiConnectionStatus,
        environment: ConnectionEnvironment,
    ) -> SupplierApiConnection {
        SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            crate::supplier_api::SupplierApiConnectionData {
                supplier_id: crate::ids::SupplierAccountId::new("supplier-1"),
                connection_code: "CONN-1".to_string(),
                environment,
                endpoint_reference: "endpoint://supplier-1".to_string(),
                credential_reference: Some("credential://supplier-1".to_string()),
                rate_limit_policy: None,
                status,
            },
            "creator-1",
        )
        .unwrap()
    }

    /// 构造启用状态的连接能力测试夹具。
    fn governance_capability(code: SupplierApiCapabilityCode) -> SupplierApiCapability {
        SupplierApiCapability::new(
            SupplierApiCapabilityId::new(format!("cap-{}", code.as_str())),
            crate::supplier_api::SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_code: code,
                status: crate::supplier_api::SupplierApiCapabilityStatus::Active,
                constraint_snapshot: None,
            },
        )
        .unwrap()
    }

    /// 构造覆盖当前能力版本的采购确认测试夹具。
    fn governance_confirmation(
        capability: &SupplierApiCapability,
        confirmed_by: &str,
    ) -> BusinessCapabilityConfirmation {
        BusinessCapabilityConfirmation::new(
            format!("confirm-{}", capability.capability_code.as_str()),
            BusinessCapabilityConfirmationData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_id: SupplierApiCapabilityId::new(capability.base.id.clone()),
                capability_code: capability.capability_code,
                requirement: BusinessCapabilityRequirement::Required,
                applicability_reference: None,
                evidence_references: vec![],
                reason_code: "REQUIRED".to_string(),
                connection_version: 1,
                capability_version: capability.base.version,
                operation_id: "operation-1".to_string(),
                idempotency_key_hash: "hash-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
                confirmed_by: confirmed_by.to_string(),
                confirmed_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap()
    }

    /// 构造验证当前技术配置和能力版本的成功健康证据。
    fn governance_health(
        connection: &SupplierApiConnection,
        capability: &SupplierApiCapability,
        requested_by: &str,
    ) -> SupplierHealthCheckRun {
        let mut run = SupplierHealthCheckRun::new(
            "health-1",
            SupplierHealthCheckRunData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                background_job_id: "job-1".to_string(),
                check_type: SupplierHealthCheckType::CapabilityMetadata,
                technical_config_version: connection.technical_config_version,
                capability_versions: vec![CapabilityVersionSnapshot {
                    capability_code: capability.capability_code,
                    version: capability.base.version,
                }],
                requested_by: requested_by.to_string(),
                idempotency_key_hash: "hash-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
            },
        )
        .unwrap();
        run.start(Instant::from_unix_secs(2)).unwrap();
        run.succeed(Instant::from_unix_secs(3), 10).unwrap();
        run
    }

    #[test]
    fn business_confirmation_deduplicates_evidence_without_mutating_capability() {
        let confirmation = BusinessCapabilityConfirmation::new(
            "confirm-1",
            BusinessCapabilityConfirmationData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_id: SupplierApiCapabilityId::new("cap-1"),
                capability_code: SupplierApiCapabilityCode::Order,
                requirement: BusinessCapabilityRequirement::Required,
                applicability_reference: Some(" scope://all ".to_string()),
                evidence_references: vec!["evidence://1".to_string(), "evidence://1".to_string()],
                reason_code: "ORDER_REQUIRED".to_string(),
                connection_version: 2,
                capability_version: 3,
                operation_id: "operation-1".to_string(),
                idempotency_key_hash: "hash-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
                confirmed_by: "buyer-1".to_string(),
                confirmed_at: Instant::from_unix_secs(1_700_000_000),
            },
        )
        .unwrap();

        assert_eq!(confirmation.evidence_references, vec!["evidence://1"]);
        assert_eq!(
            confirmation.applicability_reference.as_deref(),
            Some("scope://all")
        );
        assert_eq!(confirmation.capability_version, 3);
    }

    #[test]
    fn health_run_only_transitions_pending_running_terminal() {
        let mut run = SupplierHealthCheckRun::new(
            "health-1",
            SupplierHealthCheckRunData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                background_job_id: "job-1".to_string(),
                check_type: SupplierHealthCheckType::Connectivity,
                technical_config_version: 1,
                capability_versions: vec![CapabilityVersionSnapshot {
                    capability_code: SupplierApiCapabilityCode::Query,
                    version: 1,
                }],
                requested_by: "operator-1".to_string(),
                idempotency_key_hash: "hash-1".to_string(),
                request_fingerprint: "fingerprint-1".to_string(),
            },
        )
        .unwrap();

        assert!(run.succeed(Instant::now(), 1).is_err());
        run.start(Instant::from_unix_secs(1_700_000_000)).unwrap();
        run.fail(
            Instant::from_unix_secs(1_700_000_001),
            10,
            "ADAPTER_UNAVAILABLE".to_string(),
            "未配置连接适配器".to_string(),
        )
        .unwrap();
        assert_eq!(run.status, SupplierHealthCheckStatus::Failed);
        assert!(run.start(Instant::now()).is_err());
    }

    #[test]
    fn command_registry_is_fixed_and_complete() {
        assert_eq!(SupplierConnectionAction::all().len(), 7);
        assert_eq!(
            SupplierConnectionAction::BindCredentialReference.as_str(),
            "BIND_CREDENTIAL_REFERENCE"
        );
    }

    #[test]
    fn governance_blocks_reference_changes_without_registry() {
        let connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-governance"),
            crate::supplier_api::SupplierApiConnectionData {
                supplier_id: crate::ids::SupplierAccountId::new("supplier-1"),
                connection_code: "CONN-1".to_string(),
                environment: ConnectionEnvironment::Testing,
                endpoint_reference: String::new(),
                credential_reference: None,
                rate_limit_policy: None,
                status: SupplierApiConnectionStatus::Disabled,
            },
            "creator-1",
        )
        .unwrap();
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: &[],
            confirmations: &[],
            health_runs: &[],
        };
        let blockers = governance.blockers(
            SupplierConnectionAction::BindCredentialReference,
            SupplierConnectionBusinessImpact::default(),
            false,
        );
        assert_eq!(blockers[0].code, "REFERENCE_REGISTRY_UNAVAILABLE");
    }

    #[test]
    fn capability_confirmation_and_health_require_current_versions() {
        let capability = SupplierApiCapability::new(
            SupplierApiCapabilityId::new("cap-current"),
            crate::supplier_api::SupplierApiCapabilityData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_code: SupplierApiCapabilityCode::Product,
                status: crate::supplier_api::SupplierApiCapabilityStatus::Active,
                constraint_snapshot: None,
            },
        )
        .unwrap();
        let confirmation = BusinessCapabilityConfirmation::new(
            "confirm-current",
            BusinessCapabilityConfirmationData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                capability_id: SupplierApiCapabilityId::new("cap-current"),
                capability_code: SupplierApiCapabilityCode::Product,
                requirement: BusinessCapabilityRequirement::Required,
                applicability_reference: None,
                evidence_references: vec![],
                reason_code: "REQUIRED".to_string(),
                connection_version: 1,
                capability_version: capability.base.version,
                operation_id: "operation-current".to_string(),
                idempotency_key_hash: "hash-current".to_string(),
                request_fingerprint: "fingerprint-current".to_string(),
                confirmed_by: "buyer-1".to_string(),
                confirmed_at: Instant::from_unix_secs(1),
            },
        )
        .unwrap();
        assert!(confirmation.covers(&capability));

        let mut run = SupplierHealthCheckRun::new(
            "health-current",
            SupplierHealthCheckRunData {
                connection_id: SupplierApiConnectionId::new("conn-1"),
                background_job_id: "job-current".to_string(),
                check_type: SupplierHealthCheckType::CapabilityMetadata,
                technical_config_version: 1,
                capability_versions: vec![CapabilityVersionSnapshot {
                    capability_code: SupplierApiCapabilityCode::Product,
                    version: capability.base.version,
                }],
                requested_by: "operator-1".to_string(),
                idempotency_key_hash: "hash-current".to_string(),
                request_fingerprint: "fingerprint-current".to_string(),
            },
        )
        .unwrap();
        run.start(Instant::from_unix_secs(2)).unwrap();
        run.succeed(Instant::from_unix_secs(3), 10).unwrap();
        assert!(run.verifies(&capability));

        let mut changed = capability.clone();
        changed.base.version = capability.base.version + 2;
        assert!(!confirmation.covers(&changed));
        assert!(!run.verifies(&changed));
    }

    /// 启用连接要求当前业务确认与健康证据，生产环境还要求双人职责。
    #[test]
    fn enable_governance_requires_current_evidence_and_production_dual_roles() {
        let capability = governance_capability(SupplierApiCapabilityCode::Product);
        let confirmation = governance_confirmation(&capability, "operator-1");
        let testing = governance_connection(
            SupplierApiConnectionStatus::Disabled,
            ConnectionEnvironment::Testing,
        );
        let health = governance_health(&testing, &capability, "operator-1");
        let governance = SupplierConnectionGovernance {
            connection: &testing,
            capabilities: std::slice::from_ref(&capability),
            confirmations: std::slice::from_ref(&confirmation),
            health_runs: std::slice::from_ref(&health),
        };
        assert!(governance
            .blockers(
                SupplierConnectionAction::Enable,
                SupplierConnectionBusinessImpact::default(),
                true,
            )
            .is_empty());

        let production = governance_connection(
            SupplierApiConnectionStatus::Disabled,
            ConnectionEnvironment::Production,
        );
        let governance = SupplierConnectionGovernance {
            connection: &production,
            capabilities: std::slice::from_ref(&capability),
            confirmations: std::slice::from_ref(&confirmation),
            health_runs: std::slice::from_ref(&health),
        };
        assert_eq!(
            governance.blockers(
                SupplierConnectionAction::Enable,
                SupplierConnectionBusinessImpact::default(),
                true,
            )[0]
            .code,
            "PRODUCTION_DUAL_ROLE_REQUIRED"
        );
    }

    /// 停用连接拒绝活动业务影响，目录同步要求健康连接与商品能力。
    #[test]
    fn disable_and_catalog_governance_use_current_business_facts() {
        let capability = governance_capability(SupplierApiCapabilityCode::Product);
        let mut connection = governance_connection(
            SupplierApiConnectionStatus::Active,
            ConnectionEnvironment::Testing,
        );
        connection.record_health(HealthCheckResult::Healthy, Instant::from_unix_secs(4));
        let governance = SupplierConnectionGovernance {
            connection: &connection,
            capabilities: std::slice::from_ref(&capability),
            confirmations: &[],
            health_runs: &[],
        };
        assert!(governance
            .blockers(
                SupplierConnectionAction::StartCatalogSync,
                SupplierConnectionBusinessImpact::default(),
                true,
            )
            .is_empty());
        assert_eq!(
            governance.blockers(
                SupplierConnectionAction::Disable,
                SupplierConnectionBusinessImpact {
                    active_offerings: 1,
                    ..SupplierConnectionBusinessImpact::default()
                },
                true,
            )[0]
            .code,
            "ACTIVE_BUSINESS_IMPACT"
        );
    }

    /// 重复能力代码在实体边界稳定拒绝。
    #[test]
    fn capability_update_codes_must_be_unique() {
        assert!(ensure_unique_capability_codes([
            SupplierApiCapabilityCode::Product,
            SupplierApiCapabilityCode::Order,
        ])
        .is_ok());
        assert!(ensure_unique_capability_codes([
            SupplierApiCapabilityCode::Product,
            SupplierApiCapabilityCode::Product,
        ])
        .is_err());
    }
}
