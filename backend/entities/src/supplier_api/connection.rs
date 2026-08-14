//! `supplier_api_connection`：供应商 API 连接配置（数据模型 §6.14，页面 W20）。
//!
//! 连接是稳定配置对象（字典含启停/连接状态）→ 组合 [`crate::common::stable::StableBase`]；
//! 密钥只保存密钥管理系统引用，业务表和普通日志不得出现明文密钥。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::stable::StableBase;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierApiConnectionId};
use crate::validation::{normalize_optional_text, normalize_required_text};

/// 连接代码最大长度。
const CODE_MAX_LEN: usize = 64;
/// 地址配置引用最大长度。
const ENDPOINT_REFERENCE_MAX_LEN: usize = 512;
/// 密钥管理系统引用最大长度。
const CREDENTIAL_REFERENCE_MAX_LEN: usize = 256;

/// 连接环境（数据模型 §6.14：连接环境）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
#[serde(rename_all = "snake_case")]
pub enum ConnectionEnvironment {
    /// 生产环境。
    Production,
    /// 测试环境。
    Testing,
}

impl ConnectionEnvironment {
    /// 返回环境的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Production => "生产",
            Self::Testing => "测试",
        }
    }

    /// 返回环境的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Production => "production",
            Self::Testing => "testing",
        }
    }
}

/// 连接启停/故障状态（数据模型 §6.14：启用、停用、故障；固定枚举，无文档状态机）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SupplierApiConnectionStatus {
    /// 启用。
    #[default]
    Active,
    /// 停用。
    Disabled,
    /// 故障。
    Fault,
}

impl SupplierApiConnectionStatus {
    /// 返回状态的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Active => "启用",
            Self::Disabled => "停用",
            Self::Fault => "故障",
        }
    }

    /// 返回状态的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Active => "active",
            Self::Disabled => "disabled",
            Self::Fault => "fault",
        }
    }

    /// 判断连接是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        matches!(self, Self::Active)
    }
}

/// 最近健康检查结果（数据模型 §6.14：最近健康检查；固定枚举）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum HealthCheckResult {
    /// 检查正常。
    Healthy,
    /// 检查失败。
    Failed,
}

impl HealthCheckResult {
    /// 返回结果的中文展示名。
    ///
    /// # 返回
    /// 返回面向用户的中文标签。
    pub fn label(&self) -> &'static str {
        match self {
            Self::Healthy => "正常",
            Self::Failed => "故障",
        }
    }

    /// 返回结果的稳定代码。
    ///
    /// # 返回
    /// 返回用于持久化与查询的稳定字符串。
    pub fn as_str(&self) -> &'static str {
        match self {
            Self::Healthy => "healthy",
            Self::Failed => "failed",
        }
    }
}

/// 限流策略值对象（数据模型 §6.14：限流策略）。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct RateLimitPolicy {
    /// 窗口内最大请求数。
    max_requests: u32,
    /// 窗口时长（秒）。
    window_secs: u32,
}

impl RateLimitPolicy {
    /// 构造限流策略。
    ///
    /// # 参数
    /// * `max_requests` - 窗口内最大请求数
    /// * `window_secs` - 窗口时长（秒）
    ///
    /// # 返回
    /// 返回限流策略实例。
    ///
    /// # 错误
    /// 当最大请求数或窗口时长为零时返回错误。
    pub fn new(max_requests: u32, window_secs: u32) -> Result<Self> {
        if max_requests == 0 {
            return Err(Error::from("限流最大请求数必须大于零"));
        }
        if window_secs == 0 {
            return Err(Error::from("限流窗口时长必须大于零"));
        }
        Ok(Self {
            max_requests,
            window_secs,
        })
    }

    /// 返回窗口内最大请求数。
    ///
    /// # 返回
    /// 返回最大请求数。
    pub fn max_requests(&self) -> u32 {
        self.max_requests
    }

    /// 返回窗口时长（秒）。
    ///
    /// # 返回
    /// 返回窗口时长。
    pub fn window_secs(&self) -> u32 {
        self.window_secs
    }
}

/// 供应商 API 连接创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierApiConnectionData {
    /// API 供应商。
    pub supplier_id: SupplierAccountId,
    /// ERP 内稳定连接代码（唯一）。
    pub connection_code: String,
    /// 连接环境。
    pub environment: ConnectionEnvironment,
    /// 地址配置引用。
    pub endpoint_reference: String,
    /// 密钥管理系统引用（不保存明文密钥）。
    pub credential_reference: Option<String>,
    /// 限流策略。
    pub rate_limit_policy: Option<RateLimitPolicy>,
    /// 启停/故障状态。
    pub status: SupplierApiConnectionStatus,
}

/// 供应商 API 连接更新数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq, Default)]
pub struct SupplierApiConnectionUpdate {
    /// 连接环境；`None` 表示不修改。
    pub environment: Option<ConnectionEnvironment>,
    /// 地址配置引用；`None` 表示不修改。
    pub endpoint_reference: Option<String>,
    /// 密钥管理系统引用；`None` 表示不修改。
    pub credential_reference: Option<String>,
    /// 限流策略；`None` 表示不修改。
    pub rate_limit_policy: Option<RateLimitPolicy>,
    /// 启停/故障状态；`None` 表示不修改。
    pub status: Option<SupplierApiConnectionStatus>,
    /// 最近健康检查时间；与 `last_health_result` 必须成对出现。
    pub last_health_at: Option<Instant>,
    /// 最近健康检查结果；与 `last_health_at` 必须成对出现。
    pub last_health_result: Option<HealthCheckResult>,
}

/// 供应商 API 连接实体（稳定配置，数据模型 §6.14）。
///
/// `StableBase` 是 P0 冻结基元且未派生 `PartialEq`，因此本实体手工实现
/// `PartialEq`/`Eq`（全字段语义相等）以替代约定中的派生写法。
#[derive(Debug, Serialize, Deserialize, Clone, Entity)]
pub struct SupplierApiConnection {
    #[serde(flatten)]
    pub base: BaseModel,
    #[serde(flatten)]
    pub stable: StableBase<SupplierApiConnectionStatus>,
    /// API 供应商。
    pub supplier_id: SupplierAccountId,
    /// ERP 内稳定连接代码（创建后不可修改）。
    pub connection_code: String,
    /// 连接环境。
    pub environment: ConnectionEnvironment,
    /// 地址配置引用。
    pub endpoint_reference: String,
    /// 地址配置引用是否已由权威引用注册表确认绑定。
    pub endpoint_reference_bound: bool,
    /// 密钥管理系统引用（不保存明文密钥）。
    pub credential_reference: Option<String>,
    /// 密钥引用是否已由权威引用注册表确认绑定。
    pub credential_reference_bound: bool,
    /// 业务资料不透明引用；正文由业务资料注册表持有。
    pub business_profile_reference: Option<String>,
    /// 业务负责人账号。
    pub business_owner_id: Option<String>,
    /// 技术负责人账号。
    pub technical_owner_id: Option<String>,
    /// 技术配置版本；环境、引用或能力配置变更时递增。
    pub technical_config_version: u64,
    /// 最近成功健康证据所验证的技术配置版本。
    pub last_healthy_technical_config_version: Option<u64>,
    /// 限流策略。
    pub rate_limit_policy: Option<RateLimitPolicy>,
    /// 最近健康检查时间（与 `last_health_result` 成对出现）。
    pub last_health_at: Option<Instant>,
    /// 最近健康检查结果（与 `last_health_at` 成对出现）。
    pub last_health_result: Option<HealthCheckResult>,
}

impl PartialEq for SupplierApiConnection {
    /// 全字段语义相等。
    fn eq(&self, other: &Self) -> bool {
        self.base == other.base
            && self.stable.status == other.stable.status
            && self.stable.current_revision_id == other.stable.current_revision_id
            && self.stable.created_by == other.stable.created_by
            && self.stable.updated_by == other.stable.updated_by
            && self.supplier_id == other.supplier_id
            && self.connection_code == other.connection_code
            && self.environment == other.environment
            && self.endpoint_reference == other.endpoint_reference
            && self.endpoint_reference_bound == other.endpoint_reference_bound
            && self.credential_reference == other.credential_reference
            && self.credential_reference_bound == other.credential_reference_bound
            && self.business_profile_reference == other.business_profile_reference
            && self.business_owner_id == other.business_owner_id
            && self.technical_owner_id == other.technical_owner_id
            && self.technical_config_version == other.technical_config_version
            && self.last_healthy_technical_config_version == other.last_healthy_technical_config_version
            && self.rate_limit_policy == other.rate_limit_policy
            && self.last_health_at == other.last_health_at
            && self.last_health_result == other.last_health_result
    }
}

impl Eq for SupplierApiConnection {}

impl SupplierApiConnection {
    /// 创建供应商 API 连接。
    ///
    /// 完成 connection_code/endpoint_reference/credential_reference 的完整校验与
    /// 规范化（去首尾空白、非空、长度上限）；健康检查字段初始为空。
    ///
    /// # 参数
    /// * `id` - 实体主键（`entities::ids::SupplierApiConnectionId`）
    /// * `data` - 创建数据
    /// * `created_by` - 创建人（账号或系统身份）
    ///
    /// # 返回
    /// 返回新建的连接实体。
    ///
    /// # 错误
    /// 当连接代码/地址配置引用为空或超长、密钥引用超长时返回错误。
    pub fn new(
        id: SupplierApiConnectionId,
        data: SupplierApiConnectionData,
        created_by: impl Into<String>,
    ) -> Result<Self> {
        let connection_code = normalize_required_text(
            data.connection_code,
            "连接代码不能为空",
            CODE_MAX_LEN,
            "连接代码过长",
        )?;
        let endpoint_reference = normalize_optional_text(
            Some(data.endpoint_reference),
            "地址配置引用",
            ENDPOINT_REFERENCE_MAX_LEN,
        )?
        .unwrap_or_default();
        let credential_reference = normalize_optional_text(
            data.credential_reference,
            "密钥引用",
            CREDENTIAL_REFERENCE_MAX_LEN,
        )?;

        Ok(Self {
            base: BaseModel::new(id.to_string()),
            stable: StableBase::new(data.status, created_by),
            supplier_id: data.supplier_id,
            connection_code,
            environment: data.environment,
            endpoint_reference_bound: !endpoint_reference.is_empty(),
            endpoint_reference,
            credential_reference_bound: credential_reference.is_some(),
            credential_reference,
            business_profile_reference: None,
            business_owner_id: None,
            technical_owner_id: None,
            technical_config_version: 1,
            last_healthy_technical_config_version: None,
            rate_limit_policy: data.rate_limit_policy,
            last_health_at: None,
            last_health_result: None,
        })
    }

    /// 更新供应商 API 连接。
    ///
    /// 复用 `new` 的校验规则；`supplier_id`/`connection_code` 是稳定键，不允许在
    /// 通用更新中修改。健康检查时间与结果必须成对出现。
    ///
    /// # 参数
    /// * `update` - 更新数据
    /// * `updated_by` - 本次更新执行人
    ///
    /// # 返回
    /// 更新成功返回 `Ok(())`。
    ///
    /// # 错误
    /// 当更新字段校验失败或健康检查信息不完整时返回错误。
    pub fn update(
        &mut self,
        update: SupplierApiConnectionUpdate,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        let environment_changed = self.apply_environment(update.environment);
        let endpoint_changed = self.apply_endpoint_reference(update.endpoint_reference)?;
        let credential_changed = self.apply_credential_reference(update.credential_reference)?;
        self.apply_rate_limit_policy(update.rate_limit_policy);
        self.apply_health(update.last_health_at, update.last_health_result)?;
        self.apply_status(update.status);
        if environment_changed || endpoint_changed || credential_changed {
            self.bump_technical_config_version()?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 绑定已由权威注册表解析的地址引用。
    ///
    /// # Errors
    /// 引用为空、过长或技术配置版本溢出时返回错误。
    pub fn bind_endpoint_reference(
        &mut self,
        reference: String,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if self.apply_endpoint_reference(Some(reference))? {
            self.bump_technical_config_version()?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 绑定已由权威注册表解析的密钥引用。
    ///
    /// # Errors
    /// 引用为空、过长或技术配置版本溢出时返回错误。
    pub fn bind_credential_reference(
        &mut self,
        reference: String,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        if self.apply_credential_reference(Some(reference))? {
            self.bump_technical_config_version()?;
        }
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 绑定业务资料注册表返回的不透明引用。
    ///
    /// # Errors
    /// 引用为空或过长时返回错误。
    pub fn update_business_profile(
        &mut self,
        reference: String,
        updated_by: impl Into<String>,
    ) -> Result<()> {
        self.business_profile_reference = Some(normalize_required_text(
            reference,
            "业务资料引用不能为空",
            ENDPOINT_REFERENCE_MAX_LEN,
            "业务资料引用过长",
        )?);
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 记录一次业务能力确认触及连接版本，但不修改能力启停状态。
    pub fn touch_business_confirmation(&mut self, updated_by: impl Into<String>) {
        self.stable.touch(updated_by);
    }

    /// 记录能力配置变更并使旧技术健康证据失效。
    ///
    /// # Errors
    /// 技术配置版本溢出时返回错误。
    pub fn record_capability_configuration(&mut self, updated_by: impl Into<String>) -> Result<()> {
        self.bump_technical_config_version()?;
        self.stable.touch(updated_by);
        Ok(())
    }

    /// 启用连接。
    pub fn enable(&mut self, updated_by: impl Into<String>) {
        self.stable.status = SupplierApiConnectionStatus::Active;
        self.stable.touch(updated_by);
    }

    /// 停用连接；历史连接身份与业务事实保持不变。
    pub fn disable(&mut self, updated_by: impl Into<String>) {
        self.stable.status = SupplierApiConnectionStatus::Disabled;
        self.stable.touch(updated_by);
    }

    /// 判断地址与密钥引用是否都已经由权威注册表绑定。
    pub fn technical_references_ready(&self) -> bool {
        self.endpoint_reference_bound && self.credential_reference_bound
    }

    /// 判断当前技术配置是否已有成功健康证据。
    pub fn current_technical_config_is_healthy(&self) -> bool {
        self.last_health_result == Some(HealthCheckResult::Healthy)
            && self.last_healthy_technical_config_version == Some(self.technical_config_version)
    }

    /// 记录一次健康检查结果。
    ///
    /// 时间与结果成对写入，不产生单独置半边的状态。
    ///
    /// # 参数
    /// * `result` - 健康检查结果
    /// * `at` - 检查时间
    pub fn record_health(&mut self, result: HealthCheckResult, at: Instant) {
        self.last_health_result = Some(result);
        self.last_health_at = Some(at);
        if result == HealthCheckResult::Healthy {
            self.last_healthy_technical_config_version = Some(self.technical_config_version);
        }
    }

    /// 判断连接是否处于启用状态。
    ///
    /// # 返回
    /// 状态为 `Active` 时返回 `true`。
    pub fn is_active(&self) -> bool {
        self.stable.status().is_active()
    }

    /// 应用连接环境更新。
    ///
    /// # 参数
    /// * `environment` - 可选连接环境
    fn apply_environment(&mut self, environment: Option<ConnectionEnvironment>) -> bool {
        if let Some(environment) = environment {
            let changed = self.environment != environment;
            self.environment = environment;
            return changed;
        }
        false
    }

    /// 应用地址配置引用更新。
    ///
    /// # 参数
    /// * `endpoint_reference` - 可选地址配置引用
    ///
    /// # 错误
    /// 当地址配置引用为空或超长时返回错误。
    fn apply_endpoint_reference(&mut self, endpoint_reference: Option<String>) -> Result<bool> {
        if let Some(endpoint_reference) = endpoint_reference {
            let endpoint_reference = normalize_required_text(
                endpoint_reference,
                "地址配置引用不能为空",
                ENDPOINT_REFERENCE_MAX_LEN,
                "地址配置引用过长",
            )?;
            let changed = self.endpoint_reference != endpoint_reference || !self.endpoint_reference_bound;
            self.endpoint_reference = endpoint_reference;
            self.endpoint_reference_bound = true;
            return Ok(changed);
        }
        Ok(false)
    }

    /// 应用密钥管理系统引用更新。
    ///
    /// # 参数
    /// * `credential_reference` - 可选密钥引用
    ///
    /// # 错误
    /// 当密钥引用超长时返回错误。
    fn apply_credential_reference(&mut self, credential_reference: Option<String>) -> Result<bool> {
        if let Some(credential_reference) = credential_reference {
            let credential_reference = normalize_optional_text(
                Some(credential_reference),
                "密钥引用",
                CREDENTIAL_REFERENCE_MAX_LEN,
            )?
            .ok_or_else(|| Error::from("密钥引用不能为空"))?;
            let changed = self.credential_reference.as_deref() != Some(credential_reference.as_str())
                || !self.credential_reference_bound;
            self.credential_reference = Some(credential_reference);
            self.credential_reference_bound = true;
            return Ok(changed);
        }
        Ok(false)
    }

    /// 应用限流策略更新。
    ///
    /// # 参数
    /// * `rate_limit_policy` - 可选限流策略
    fn apply_rate_limit_policy(&mut self, rate_limit_policy: Option<RateLimitPolicy>) {
        if let Some(rate_limit_policy) = rate_limit_policy {
            self.rate_limit_policy = Some(rate_limit_policy);
        }
    }

    /// 应用健康检查信息更新。
    ///
    /// # 参数
    /// * `last_health_at` - 可选健康检查时间
    /// * `last_health_result` - 可选健康检查结果
    ///
    /// # 错误
    /// 当健康检查时间与结果只有其一出现时返回错误。
    fn apply_health(
        &mut self,
        last_health_at: Option<Instant>,
        last_health_result: Option<HealthCheckResult>,
    ) -> Result<()> {
        if last_health_at.is_some() != last_health_result.is_some() {
            return Err(Error::from("健康检查时间与结果必须同时提供或同时省略"));
        }
        if let Some(last_health_at) = last_health_at {
            self.last_health_at = Some(last_health_at);
            self.last_health_result = last_health_result;
        }
        Ok(())
    }

    /// 应用启停/故障状态更新。
    ///
    /// # 参数
    /// * `status` - 可选连接状态
    fn apply_status(&mut self, status: Option<SupplierApiConnectionStatus>) {
        if let Some(status) = status {
            self.stable.status = status;
        }
    }

    fn bump_technical_config_version(&mut self) -> Result<()> {
        self.technical_config_version = self
            .technical_config_version
            .checked_add(1)
            .ok_or_else(|| Error::from("技术配置版本溢出"))?;
        self.last_healthy_technical_config_version = None;
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ConnectionEnvironment, HealthCheckResult, RateLimitPolicy, SupplierApiConnection,
        SupplierApiConnectionData, SupplierApiConnectionStatus, SupplierApiConnectionUpdate,
    };
    use crate::common::time::Instant;
    use crate::ids::{SupplierAccountId, SupplierApiConnectionId};

    fn connection_data() -> SupplierApiConnectionData {
        SupplierApiConnectionData {
            supplier_id: SupplierAccountId::new("sup-1"),
            connection_code: " SUP-CN-001 ".to_string(),
            environment: ConnectionEnvironment::Production,
            endpoint_reference: " config://supplier/001 ".to_string(),
            credential_reference: Some(" kms://prod/erp/sup-001 ".to_string()),
            rate_limit_policy: None,
            status: SupplierApiConnectionStatus::Active,
        }
    }

    #[test]
    fn connection_new_trims_and_validates_text_fields() {
        let connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            connection_data(),
            "admin-1",
        )
        .unwrap();

        assert_eq!(connection.connection_code, "SUP-CN-001");
        assert_eq!(connection.endpoint_reference, "config://supplier/001");
        assert_eq!(
            connection.credential_reference.as_deref(),
            Some("kms://prod/erp/sup-001")
        );
        assert_eq!(connection.supplier_id, SupplierAccountId::new("sup-1"));
        assert_eq!(connection.environment, ConnectionEnvironment::Production);
        assert_eq!(connection.stable.status(), SupplierApiConnectionStatus::Active);
        assert_eq!(connection.stable.created_by, "admin-1");
        assert!(connection.last_health_at.is_none());
        assert!(connection.is_active());
    }

    #[test]
    fn connection_new_rejects_empty_and_overlong_code() {
        let empty_code = SupplierApiConnectionData {
            connection_code: "   ".to_string(),
            ..connection_data()
        };
        assert!(
            SupplierApiConnection::new(SupplierApiConnectionId::new("conn-2"), empty_code, "admin-1")
                .is_err()
        );

        let overlong_code = SupplierApiConnectionData {
            connection_code: "x".repeat(65),
            ..connection_data()
        };
        assert!(
            SupplierApiConnection::new(SupplierApiConnectionId::new("conn-3"), overlong_code, "admin-1")
                .is_err()
        );
    }

    #[test]
    fn connection_new_allows_missing_endpoint_but_rejects_overlong_credential_reference() {
        let blank_endpoint = SupplierApiConnectionData {
            endpoint_reference: "  ".to_string(),
            ..connection_data()
        };
        let connection =
            SupplierApiConnection::new(SupplierApiConnectionId::new("conn-4"), blank_endpoint, "admin-1")
                .unwrap();
        assert!(!connection.endpoint_reference_bound);
        assert!(connection.endpoint_reference.is_empty());

        let overlong_credential = SupplierApiConnectionData {
            credential_reference: Some("k".repeat(257)),
            ..connection_data()
        };
        assert!(SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-5"),
            overlong_credential,
            "admin-1"
        )
        .is_err());
    }

    #[test]
    fn connection_update_applies_fields_and_keeps_stable_keys() {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            connection_data(),
            "admin-1",
        )
        .unwrap();

        connection
            .update(
                SupplierApiConnectionUpdate {
                    environment: Some(ConnectionEnvironment::Testing),
                    status: Some(SupplierApiConnectionStatus::Disabled),
                    rate_limit_policy: Some(RateLimitPolicy::new(100, 60).unwrap()),
                    last_health_at: Some(Instant::from_unix_secs(1_700_000_000)),
                    last_health_result: Some(HealthCheckResult::Healthy),
                    ..Default::default()
                },
                "admin-2",
            )
            .unwrap();

        assert_eq!(connection.environment, ConnectionEnvironment::Testing);
        assert_eq!(connection.stable.status(), SupplierApiConnectionStatus::Disabled);
        assert!(!connection.is_active());
        assert_eq!(connection.rate_limit_policy.unwrap().max_requests(), 100);
        assert_eq!(connection.last_health_at.unwrap().unix_secs(), 1_700_000_000);
        assert_eq!(connection.connection_code, "SUP-CN-001", "稳定键不可修改");
        assert_eq!(connection.stable.updated_by, "admin-2");
        assert_eq!(connection.stable.created_by, "admin-1", "touch 不修改创建人");
    }

    #[test]
    fn connection_update_rejects_blank_endpoint_and_half_health_pair() {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            connection_data(),
            "admin-1",
        )
        .unwrap();

        let blank_endpoint = SupplierApiConnectionUpdate {
            endpoint_reference: Some("   ".to_string()),
            ..Default::default()
        };
        assert!(connection.update(blank_endpoint, "admin-2").is_err());

        let half_health = SupplierApiConnectionUpdate {
            last_health_at: Some(Instant::from_unix_secs(1_700_000_000)),
            last_health_result: None,
            ..Default::default()
        };
        assert!(connection.update(half_health, "admin-2").is_err());
    }

    #[test]
    fn connection_record_health_sets_pair_and_status_flags() {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            connection_data(),
            "admin-1",
        )
        .unwrap();

        connection.record_health(HealthCheckResult::Failed, Instant::from_unix_secs(1_700_000_000));
        assert_eq!(connection.last_health_result, Some(HealthCheckResult::Failed));
        assert_eq!(connection.last_health_at.unwrap().unix_secs(), 1_700_000_000);
        assert!(connection.is_active(), "健康检查结果不自动改变启停状态");
        assert!(!connection.current_technical_config_is_healthy());

        connection.record_health(HealthCheckResult::Healthy, Instant::from_unix_secs(1_700_000_001));
        assert!(connection.current_technical_config_is_healthy());
    }

    #[test]
    fn binding_reference_invalidates_old_health_evidence() {
        let mut connection = SupplierApiConnection::new(
            SupplierApiConnectionId::new("conn-1"),
            connection_data(),
            "admin-1",
        )
        .unwrap();
        connection.record_health(HealthCheckResult::Healthy, Instant::from_unix_secs(1));
        assert!(connection.current_technical_config_is_healthy());

        connection
            .bind_credential_reference("kms://prod/new".to_string(), "operator-1")
            .unwrap();
        assert_eq!(connection.technical_config_version, 2);
        assert!(!connection.current_technical_config_is_healthy());
    }

    #[test]
    fn rate_limit_policy_rejects_zero_bounds() {
        assert!(RateLimitPolicy::new(0, 60).is_err());
        assert!(RateLimitPolicy::new(100, 0).is_err());
        let policy = RateLimitPolicy::new(100, 60).unwrap();
        assert_eq!(policy.window_secs(), 60);
    }

    #[test]
    fn connection_enums_serialize_with_stable_codes_and_expose_labels() {
        assert_eq!(
            serde_json::to_string(&ConnectionEnvironment::Testing).unwrap(),
            "\"testing\""
        );
        assert_eq!(
            serde_json::to_string(&SupplierApiConnectionStatus::Fault).unwrap(),
            "\"fault\""
        );
        assert_eq!(
            serde_json::to_string(&HealthCheckResult::Healthy).unwrap(),
            "\"healthy\""
        );

        assert_eq!(ConnectionEnvironment::Production.label(), "生产");
        assert_eq!(SupplierApiConnectionStatus::Disabled.label(), "停用");
        assert_eq!(HealthCheckResult::Failed.label(), "故障");
        assert_eq!(HealthCheckResult::Healthy.as_str(), "healthy");
    }
}
