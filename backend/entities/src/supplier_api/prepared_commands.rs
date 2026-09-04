//! W20 连接创建与治理命令的预检形态（INT-E28）。
//!
//! 创建连接时的技术引用/初始状态/初始能力限制，以及七类治理动作的
//! required/forbidden 字段矩阵，原先由 Service 在 `create_connection` 与四个
//! `execute_*` 入口分散检查，未携带字段被静默忽略。本模块独占这组纯输入规则：
//! wire DTO 先通过既有 `Validate`，再经由本模块生成已预检命令后才进入编排；
//! 事务、授权、引用解析与持久化仍归 Service。

use serde::{Deserialize, Serialize};

use super::governance::SupplierConnectionAction;
use super::{SupplierApiConnectionStatus, SupplierHealthCheckType};

/// 连接治理命令各可选字段在矩阵中的位置（仅用于拒绝说明）。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CommandOptionalField {
    /// 服务端签发的不透明引用。
    PayloadReference,
    /// 固定业务原因代码。
    ReasonCode,
    /// 健康检查白名单类型。
    CheckType,
}

impl CommandOptionalField {
    /// 返回字段的中文说明。
    ///
    /// # 返回
    /// 返回面向用户的字段标签。
    pub fn label(self) -> &'static str {
        match self {
            Self::PayloadReference => "不透明引用",
            Self::ReasonCode => "业务原因代码",
            Self::CheckType => "健康检查类型",
        }
    }
}

/// 创建/命令形态校验拒绝原因（强类型，Service 按变体映射为既有错误语义）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SupplierCommandShapeRejection {
    /// 创建连接携带了技术引用（地址或密钥引用必须走不透明引用绑定命令）。
    TechnicalReferenceOnCreate,
    /// 创建连接未从停用状态开始。
    MustStartDisabled,
    /// 创建连接携带了初始能力（必须走独立能力配置命令）。
    CapabilitiesOnCreate,
    /// 连接版本不是正整数。
    InvalidExpectedVersion,
    /// 必填字段缺失或空白。
    MissingField(CommandOptionalField),
    /// 动作不接受该可选字段（多余字段必须拒绝，不得忽略）。
    ForbiddenField {
        /// 目标治理动作。
        action: SupplierConnectionAction,
        /// 被拒绝的可选字段。
        field: CommandOptionalField,
    },
}

impl std::fmt::Display for SupplierCommandShapeRejection {
    /// 返回与历史 Service 内联校验一致的中文说明。
    ///
    /// # 参数
    /// * `f` - 格式化目标
    ///
    /// # 返回
    /// 写入用户可读的拒绝说明。
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::TechnicalReferenceOnCreate => {
                write!(f, "创建连接只建立身份；技术引用必须通过不透明引用绑定命令提交")
            }
            Self::MustStartDisabled => write!(f, "新连接必须从停用状态开始"),
            Self::CapabilitiesOnCreate => {
                write!(f, "初始能力必须通过独立能力配置命令提交")
            }
            Self::InvalidExpectedVersion => write!(f, "连接版本必须大于0"),
            Self::MissingField(CommandOptionalField::PayloadReference) => {
                write!(f, "缺少不透明引用")
            }
            Self::MissingField(CommandOptionalField::ReasonCode) => {
                write!(f, "停用原因不能为空")
            }
            Self::MissingField(CommandOptionalField::CheckType) => {
                write!(f, "健康检查类型不能为空")
            }
            Self::ForbiddenField { action, field } => {
                write!(f, "{} 不接受{}，多余字段必须省略", action.as_str(), field.label())
            }
        }
    }
}

impl std::error::Error for SupplierCommandShapeRejection {}

impl From<SupplierCommandShapeRejection> for crate::errors::Error {
    /// 将形态拒绝转换为实体层通用错误（保留展示文本）。
    ///
    /// # 参数
    /// * `rejection` - 形态校验拒绝原因
    ///
    /// # 返回
    /// 携带同文本的实体层错误。
    fn from(rejection: SupplierCommandShapeRejection) -> Self {
        Self::from(rejection.to_string())
    }
}

/// 已预检的连接创建形态（创建只建立身份，状态恒为停用）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct PreparedSupplierConnectionCreate {
    /// 新连接起始状态（恒为停用）。
    status: SupplierApiConnectionStatus,
}

impl PreparedSupplierConnectionCreate {
    /// 预检创建请求的技术引用、初始状态与初始能力限制。
    ///
    /// # 参数
    /// * `endpoint_reference` - 地址配置引用；创建时必须为空
    /// * `credential_reference` - 密钥管理系统引用；创建时必须为空
    /// * `status` - 请求状态；缺省或停用通过，其余拒绝
    /// * `capability_count` - 初始能力条数；必须为零
    ///
    /// # 返回
    /// 返回起始状态恒为停用的已预检创建形态。
    ///
    /// # 错误
    /// 当携带技术引用、状态非停用或携带初始能力时返回
    /// [`SupplierCommandShapeRejection`]。
    ///
    /// # 约束
    /// 纯内存校验；不访问 MongoDB、时钟、ID 生成器或密钥。
    pub fn try_new(
        endpoint_reference: Option<&str>,
        credential_reference: Option<&str>,
        status: Option<SupplierApiConnectionStatus>,
        capability_count: usize,
    ) -> Result<Self, SupplierCommandShapeRejection> {
        if endpoint_reference.is_some() || credential_reference.is_some() {
            return Err(SupplierCommandShapeRejection::TechnicalReferenceOnCreate);
        }
        if status.is_some_and(|status| status != SupplierApiConnectionStatus::Disabled) {
            return Err(SupplierCommandShapeRejection::MustStartDisabled);
        }
        if capability_count > 0 {
            return Err(SupplierCommandShapeRejection::CapabilitiesOnCreate);
        }
        Ok(Self {
            status: SupplierApiConnectionStatus::Disabled,
        })
    }

    /// 返回新连接起始状态（恒为停用）。
    ///
    /// # 返回
    /// 返回停用状态。
    pub fn status(self) -> SupplierApiConnectionStatus {
        self.status
    }
}

/// 已预检的连接治理命令（按动作分载体的 tagged 变体，各变体只携带本动作字段）。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PreparedSupplierConnectionCommand {
    /// 更新业务资料引用（必须携带不透明引用）。
    UpdateBusinessProfile {
        /// 期望连接版本。
        expected_version: u64,
        /// 已规范化的不透明引用。
        payload_reference: String,
    },
    /// 绑定地址引用（必须携带不透明引用）。
    BindEndpointReference {
        /// 期望连接版本。
        expected_version: u64,
        /// 已规范化的不透明引用。
        payload_reference: String,
    },
    /// 绑定密钥引用（必须携带不透明引用）。
    BindCredentialReference {
        /// 期望连接版本。
        expected_version: u64,
        /// 已规范化的不透明引用。
        payload_reference: String,
    },
    /// 执行健康检查（必须携带检查类型）。
    RunHealthCheck {
        /// 期望连接版本。
        expected_version: u64,
        /// 健康检查白名单类型。
        check_type: SupplierHealthCheckType,
    },
    /// 启用连接（不接受任何可选字段）。
    Enable {
        /// 期望连接版本。
        expected_version: u64,
    },
    /// 停用连接（必须携带原因代码）。
    Disable {
        /// 期望连接版本。
        expected_version: u64,
        /// 已规范化的停用原因代码。
        reason_code: String,
    },
    /// 启动目录同步（不接受任何可选字段）。
    StartCatalogSync {
        /// 期望连接版本。
        expected_version: u64,
    },
}

impl PreparedSupplierConnectionCommand {
    /// 按七类动作的 required/forbidden 矩阵预检扁平命令字段。
    ///
    /// 引用类动作要求不透明引用；健康检查要求检查类型；停用要求原因代码；
    /// 各动作不接受的多余字段一律拒绝，不再静默忽略。幂等键的形态校验仍由
    /// 命令身份在 Service 侧完成，本方法不解释幂等语义。
    ///
    /// # 参数
    /// * `action` - 固定治理动作
    /// * `expected_version` - 期望连接版本（必须大于 `0`）
    /// * `payload_reference` - 不透明引用（引用类动作必填，其余禁止）
    /// * `reason_code` - 业务原因代码（停用必填，其余禁止）
    /// * `check_type` - 健康检查类型（健康检查必填，其余禁止）
    ///
    /// # 返回
    /// 返回只携带本动作字段的 tagged 命令变体。
    ///
    /// # 错误
    /// 当版本为零、必填字段缺失或携带多余字段时返回
    /// [`SupplierCommandShapeRejection`]。
    ///
    /// # 约束
    /// 纯内存校验；不访问 MongoDB、时钟、ID 生成器、密钥或外部注册表。
    pub fn try_from_parts(
        action: SupplierConnectionAction,
        expected_version: u64,
        payload_reference: Option<&str>,
        reason_code: Option<&str>,
        check_type: Option<SupplierHealthCheckType>,
    ) -> Result<Self, SupplierCommandShapeRejection> {
        if expected_version == 0 {
            return Err(SupplierCommandShapeRejection::InvalidExpectedVersion);
        }
        match action {
            SupplierConnectionAction::UpdateBusinessProfile => {
                let prepared = Self::UpdateBusinessProfile {
                    expected_version,
                    payload_reference: require_reference(payload_reference)?,
                };
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(prepared)
            }
            SupplierConnectionAction::BindEndpointReference => {
                let prepared = Self::BindEndpointReference {
                    expected_version,
                    payload_reference: require_reference(payload_reference)?,
                };
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(prepared)
            }
            SupplierConnectionAction::BindCredentialReference => {
                let prepared = Self::BindCredentialReference {
                    expected_version,
                    payload_reference: require_reference(payload_reference)?,
                };
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(prepared)
            }
            SupplierConnectionAction::RunHealthCheck => {
                forbid(action, CommandOptionalField::PayloadReference, payload_reference)?;
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                let check_type = check_type.ok_or(SupplierCommandShapeRejection::MissingField(
                    CommandOptionalField::CheckType,
                ))?;
                Ok(Self::RunHealthCheck {
                    expected_version,
                    check_type,
                })
            }
            SupplierConnectionAction::Enable => {
                forbid(action, CommandOptionalField::PayloadReference, payload_reference)?;
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(Self::Enable { expected_version })
            }
            SupplierConnectionAction::Disable => {
                forbid(action, CommandOptionalField::PayloadReference, payload_reference)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(Self::Disable {
                    expected_version,
                    reason_code: require_reason(reason_code)?,
                })
            }
            SupplierConnectionAction::StartCatalogSync => {
                forbid(action, CommandOptionalField::PayloadReference, payload_reference)?;
                forbid(action, CommandOptionalField::ReasonCode, reason_code)?;
                forbid(action, CommandOptionalField::CheckType, check_type)?;
                Ok(Self::StartCatalogSync { expected_version })
            }
        }
    }

    /// 返回命令的固定治理动作。
    ///
    /// # 返回
    /// 返回本变体对应的治理动作。
    pub fn action(&self) -> SupplierConnectionAction {
        match self {
            Self::UpdateBusinessProfile { .. } => SupplierConnectionAction::UpdateBusinessProfile,
            Self::BindEndpointReference { .. } => SupplierConnectionAction::BindEndpointReference,
            Self::BindCredentialReference { .. } => SupplierConnectionAction::BindCredentialReference,
            Self::RunHealthCheck { .. } => SupplierConnectionAction::RunHealthCheck,
            Self::Enable { .. } => SupplierConnectionAction::Enable,
            Self::Disable { .. } => SupplierConnectionAction::Disable,
            Self::StartCatalogSync { .. } => SupplierConnectionAction::StartCatalogSync,
        }
    }

    /// 返回命令携带的期望连接版本。
    ///
    /// # 返回
    /// 返回大于 `0` 的期望版本。
    pub fn expected_version(&self) -> u64 {
        match self {
            Self::UpdateBusinessProfile { expected_version, .. }
            | Self::BindEndpointReference { expected_version, .. }
            | Self::BindCredentialReference { expected_version, .. }
            | Self::RunHealthCheck { expected_version, .. }
            | Self::Enable { expected_version }
            | Self::Disable { expected_version, .. }
            | Self::StartCatalogSync { expected_version } => *expected_version,
        }
    }
}

/// 要求不透明引用存在且去除首尾空白后非空。
///
/// # 参数
/// * `value` - 可选不透明引用
///
/// # 返回
/// 返回规范化后的引用正文。
///
/// # 错误
/// 当引用缺失或空白时返回缺失拒绝。
fn require_reference(value: Option<&str>) -> Result<String, SupplierCommandShapeRejection> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(SupplierCommandShapeRejection::MissingField(
            CommandOptionalField::PayloadReference,
        ))
}

/// 要求停用原因代码存在且去除首尾空白后非空。
///
/// # 参数
/// * `value` - 可选原因代码
///
/// # 返回
/// 返回规范化后的原因代码。
///
/// # 错误
/// 当原因代码缺失或空白时返回缺失拒绝。
fn require_reason(value: Option<&str>) -> Result<String, SupplierCommandShapeRejection> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .ok_or(SupplierCommandShapeRejection::MissingField(
            CommandOptionalField::ReasonCode,
        ))
}

/// 拒绝指定动作不接受的可选字段（`Some` 即拒绝，`None` 通过）。
///
/// # 参数
/// * `action` - 目标治理动作
/// * `field` - 被检查的可选字段
/// * `value` - 字段取值；`None` 表示未携带
///
/// # 返回
/// 未携带时返回 `Ok(())`。
///
/// # 错误
/// 当字段被携带时返回禁止拒绝。
fn forbid<T>(
    action: SupplierConnectionAction,
    field: CommandOptionalField,
    value: Option<T>,
) -> Result<(), SupplierCommandShapeRejection> {
    if value.is_some() {
        return Err(SupplierCommandShapeRejection::ForbiddenField { action, field });
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::{
        PreparedSupplierConnectionCommand, PreparedSupplierConnectionCreate, SupplierCommandShapeRejection,
    };
    use crate::supplier_api::{
        SupplierApiConnectionStatus, SupplierConnectionAction, SupplierHealthCheckType,
    };

    #[test]
    fn prepared_create_accepts_identity_only_and_forbids_technical_fields() {
        let prepared = PreparedSupplierConnectionCreate::try_new(None, None, None, 0).unwrap();
        assert_eq!(prepared.status(), SupplierApiConnectionStatus::Disabled);
        let explicit = PreparedSupplierConnectionCreate::try_new(
            None,
            None,
            Some(SupplierApiConnectionStatus::Disabled),
            0,
        )
        .unwrap();
        assert_eq!(explicit.status(), SupplierApiConnectionStatus::Disabled);

        assert_eq!(
            PreparedSupplierConnectionCreate::try_new(Some("config://x"), None, None, 0)
                .expect_err("创建时技术引用必须拒绝"),
            SupplierCommandShapeRejection::TechnicalReferenceOnCreate
        );
        assert_eq!(
            PreparedSupplierConnectionCreate::try_new(None, Some("kms://x"), None, 0)
                .expect_err("创建时密钥引用必须拒绝"),
            SupplierCommandShapeRejection::TechnicalReferenceOnCreate
        );
        assert_eq!(
            PreparedSupplierConnectionCreate::try_new(
                None,
                None,
                Some(SupplierApiConnectionStatus::Active),
                0
            )
            .expect_err("非停用初始状态必须拒绝"),
            SupplierCommandShapeRejection::MustStartDisabled
        );
        assert_eq!(
            PreparedSupplierConnectionCreate::try_new(None, None, None, 1).expect_err("初始能力必须拒绝"),
            SupplierCommandShapeRejection::CapabilitiesOnCreate
        );
    }

    #[test]
    fn prepared_reference_commands_require_payload_and_forbid_extras() {
        for action in [
            SupplierConnectionAction::UpdateBusinessProfile,
            SupplierConnectionAction::BindEndpointReference,
            SupplierConnectionAction::BindCredentialReference,
        ] {
            let prepared = PreparedSupplierConnectionCommand::try_from_parts(
                action,
                3,
                Some("  opaque-ref  "),
                None,
                None,
            )
            .unwrap();
            assert_eq!(prepared.action(), action);
            assert_eq!(prepared.expected_version(), 3);

            assert!(
                PreparedSupplierConnectionCommand::try_from_parts(action, 3, None, None, None).is_err(),
                "引用类动作缺失引用必须拒绝"
            );
            assert!(
                PreparedSupplierConnectionCommand::try_from_parts(
                    action,
                    3,
                    Some("opaque-ref"),
                    Some("reason"),
                    None
                )
                .is_err(),
                "引用类动作多余原因代码必须拒绝"
            );
            assert!(
                PreparedSupplierConnectionCommand::try_from_parts(
                    action,
                    3,
                    Some("opaque-ref"),
                    None,
                    Some(SupplierHealthCheckType::Connectivity)
                )
                .is_err(),
                "引用类动作多余检查类型必须拒绝"
            );
        }
    }

    #[test]
    fn prepared_health_command_requires_check_type_and_forbids_reference() {
        let prepared = PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::RunHealthCheck,
            2,
            None,
            None,
            Some(SupplierHealthCheckType::Authentication),
        )
        .unwrap();
        assert!(matches!(
            prepared,
            PreparedSupplierConnectionCommand::RunHealthCheck { .. }
        ));

        assert!(PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::RunHealthCheck,
            2,
            None,
            None,
            None
        )
        .is_err());
        assert!(PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::RunHealthCheck,
            2,
            Some("opaque-ref"),
            None,
            Some(SupplierHealthCheckType::Connectivity)
        )
        .is_err());
    }

    #[test]
    fn prepared_status_and_catalog_commands_fix_required_and_forbidden_fields() {
        let enable = PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::Enable,
            1,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(enable.action(), SupplierConnectionAction::Enable);
        assert!(PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::Enable,
            1,
            None,
            Some("reason"),
            None
        )
        .is_err());

        let disable = PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::Disable,
            1,
            None,
            Some("  ops-freeze  "),
            None,
        )
        .unwrap();
        assert!(matches!(
            disable,
            PreparedSupplierConnectionCommand::Disable { .. }
        ));
        assert!(PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::Disable,
            1,
            None,
            Some("   "),
            None
        )
        .is_err());

        let catalog = PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::StartCatalogSync,
            1,
            None,
            None,
            None,
        )
        .unwrap();
        assert_eq!(catalog.action(), SupplierConnectionAction::StartCatalogSync);
        assert!(PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::StartCatalogSync,
            1,
            None,
            None,
            Some(SupplierHealthCheckType::Connectivity)
        )
        .is_err());
    }

    #[test]
    fn prepared_command_rejects_zero_version_and_reports_legacy_texts() {
        let rejection = PreparedSupplierConnectionCommand::try_from_parts(
            SupplierConnectionAction::Enable,
            0,
            None,
            None,
            None,
        )
        .expect_err("零版本必须拒绝");
        assert_eq!(rejection, SupplierCommandShapeRejection::InvalidExpectedVersion);
        assert_eq!(
            SupplierCommandShapeRejection::TechnicalReferenceOnCreate.to_string(),
            "创建连接只建立身份；技术引用必须通过不透明引用绑定命令提交"
        );
        let error: crate::errors::Error = rejection.into();
        assert_eq!(error.to_string(), "连接版本必须大于0");
    }
}
