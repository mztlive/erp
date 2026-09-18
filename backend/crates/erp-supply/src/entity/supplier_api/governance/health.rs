//! 供应商连接技术健康检查。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::ids::SupplierApiConnectionId;
use erp_core::validation::normalize_optional_text;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::{
    ACTOR_ID_MAX_LEN, ERROR_CODE_MAX_LEN, ERROR_SUMMARY_MAX_LEN, HASH_MAX_LEN, OPERATION_ID_MAX_LEN, required,
};
use crate::entity::supplier_api::{SupplierApiCapability, SupplierApiCapabilityCode};

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
        self.finish(SupplierHealthCheckStatus::Failed, at, latency_ms, Some(error_code), Some(error_summary))
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
        self.finish(SupplierHealthCheckStatus::Unknown, at, latency_ms, Some(error_code), Some(error_summary))
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
        self.error_code =
            error_code.map(|value| required(value, "错误代码", ERROR_CODE_MAX_LEN)).transpose()?;
        self.error_summary = normalize_optional_text(error_summary, "错误摘要", ERROR_SUMMARY_MAX_LEN)?;
        self.status = status;
        self.finished_at = Some(at);
        self.latency_ms = Some(latency_ms);
        Ok(())
    }
}
