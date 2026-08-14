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
use crate::supplier_api::SupplierApiCapabilityCode;
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

/// 命令回执终态。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SupplierCommandOutcome {
    Succeeded,
    Processing,
    Rejected,
    Unknown,
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
    /// # Errors
    /// 必填摘要、审计号为空或连接版本为零时返回错误。
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        id: impl Into<String>,
        connection_id: SupplierApiConnectionId,
        action: SupplierConnectionAction,
        actor_id: String,
        idempotency_key_hash: String,
        request_fingerprint: String,
        outcome: SupplierCommandOutcome,
        connection_version: u64,
        job_id: Option<String>,
        audit_event_id: String,
    ) -> Result<Self> {
        if connection_version == 0 {
            return Err(Error::from("命令回执的连接版本必须大于零"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            connection_id,
            action,
            actor_id: required(actor_id, "操作人", ACTOR_ID_MAX_LEN)?,
            idempotency_key_hash: required(idempotency_key_hash, "幂等摘要", HASH_MAX_LEN)?,
            request_fingerprint: required(request_fingerprint, "请求摘要", HASH_MAX_LEN)?,
            outcome,
            connection_version,
            job_id: normalize_optional_text(job_id, "后台任务ID", OPERATION_ID_MAX_LEN)?,
            audit_event_id: required(audit_event_id, "审计号", OPERATION_ID_MAX_LEN)?,
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
}
