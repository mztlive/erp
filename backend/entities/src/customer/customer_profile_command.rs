//! `customer_profile_command`：客户资料根级保存命令的幂等结果事实。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::common::time::BusinessDate;
use crate::errors::{Error, Result};
use crate::validation::normalize_required_text;

use super::profile_validation::CustomerProfileOperation;

/// 客户端幂等键最大长度。
const IDEMPOTENCY_KEY_MAX_LEN: usize = 128;
/// 当前客户资料请求指纹算法版本。
const REQUEST_FINGERPRINT_V1_PREFIX: &str = "sha256-json-v1:";
/// SHA-256 十六进制摘要长度。
const SHA256_HEX_LEN: usize = 64;

/// 客户资料请求的版本化 SHA-256 指纹。
///
/// v1 对 DTO 的 JSON 字节执行 SHA-256；内存值使用版本前缀约束字段顺序与
/// 算法演进。命令记录继续持久化历史 64 位裸摘要，保证旧应用可回读和回滚。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CustomerProfileRequestFingerprint(String);

impl CustomerProfileRequestFingerprint {
    /// 从调用方已固定的 v1 JSON 字节形成当前版本稳定指纹。
    ///
    /// # 参数
    /// * `payload` - DTO 层按 v1 字段顺序形成的 JSON 字节
    ///
    /// # 返回
    /// 返回 `sha256-json-v1:<64hex>` 形态的请求指纹。
    ///
    /// # 关键业务约束
    /// VO 只对显式字节计算摘要，不依赖或反向引用 Service DTO；调用方必须用
    /// golden 测试冻结 v1 JSON 字段顺序。
    pub fn from_json_bytes_v1(payload: &[u8]) -> Self {
        Self(format!(
            "{REQUEST_FINGERPRINT_V1_PREFIX}{}",
            hex::encode(Sha256::digest(payload))
        ))
    }

    /// 解析当前版本或历史裸 SHA-256 指纹。
    ///
    /// # 参数
    /// * `value` - 当前带版本前缀或历史 64 位裸摘要
    ///
    /// # 返回
    /// 返回规范化为当前版本前缀和小写摘要的值对象。
    ///
    /// # 错误
    /// 指纹版本、长度或十六进制格式非法时返回错误。
    pub fn parse_compatible(value: impl AsRef<str>) -> Result<Self> {
        let value = value.as_ref();
        if value.is_empty() || value != value.trim() {
            return Err(Error::from("客户资料请求指纹格式无效"));
        }
        let digest = value.strip_prefix(REQUEST_FINGERPRINT_V1_PREFIX).unwrap_or(value);
        if digest.len() != SHA256_HEX_LEN || !digest.bytes().all(|byte| byte.is_ascii_hexdigit()) {
            return Err(Error::from("客户资料请求指纹格式无效"));
        }
        Ok(Self(format!(
            "{REQUEST_FINGERPRINT_V1_PREFIX}{}",
            digest.to_ascii_lowercase()
        )))
    }

    /// 返回内存比较与诊断使用的带版本指纹。
    ///
    /// # 返回
    /// 返回 `sha256-json-v1:<64hex>` 字符串切片。
    pub fn as_str(&self) -> &str {
        &self.0
    }

    /// 返回 SHA-256 摘要部分。
    fn digest_hex(&self) -> &str {
        self.0
            .strip_prefix(REQUEST_FINGERPRINT_V1_PREFIX)
            .expect("已验证客户资料指纹必须包含 v1 前缀")
    }

    /// 核对持久化的当前或历史指纹是否与本请求相同。
    fn matches_persisted(&self, persisted: &str) -> bool {
        Self::parse_compatible(persisted)
            .map(|candidate| candidate.digest_hex() == self.digest_hex())
            .unwrap_or(false)
    }
}

/// 客户资料幂等重放所需的稳定请求身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerProfileReplayContext {
    idempotency_key: String,
    operation: CustomerProfileOperation,
    customer_id: Option<String>,
    initiated_by: String,
    request_fingerprint: CustomerProfileRequestFingerprint,
}

impl CustomerProfileReplayContext {
    /// 构造并校验客户资料命令的重放身份。
    ///
    /// 创建命令尚无既有客户 ID，必须传入 `None`；修订命令必须传入非空
    /// 客户 ID，重放时按该 ID 精确核对。
    ///
    /// # 参数
    /// * `idempotency_key` - 客户端幂等键
    /// * `operation` - 创建或修订操作
    /// * `customer_id` - 修订目标客户；创建时为 `None`
    /// * `initiated_by` - 发起命令的账号 ID
    /// * `request_fingerprint` - 当前请求的版本化指纹
    ///
    /// # 返回
    /// 返回已规范化、可用于查询和重放核对的上下文。
    ///
    /// # 错误
    /// 幂等键、发起人或修订客户 ID 非法，或创建命令携带客户 ID 时返回错误。
    pub fn new(
        idempotency_key: impl Into<String>,
        operation: CustomerProfileOperation,
        customer_id: Option<String>,
        initiated_by: impl Into<String>,
        request_fingerprint: CustomerProfileRequestFingerprint,
    ) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            idempotency_key.into(),
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        let initiated_by = initiated_by.into();
        if initiated_by.is_empty() || initiated_by != initiated_by.trim() {
            return Err(Error::from("客户资料命令发起人不能为空"));
        }
        let customer_id = replay_customer_id(operation, customer_id)?;
        Ok(Self {
            idempotency_key,
            operation,
            customer_id,
            initiated_by,
            request_fingerprint,
        })
    }

    /// 返回规范化后的幂等键。
    ///
    /// # 返回
    /// 返回仓储查询和新命令持久化共用的幂等键。
    pub fn idempotency_key(&self) -> &str {
        &self.idempotency_key
    }

    /// 返回命令操作。
    ///
    /// # 返回
    /// 返回创建或修订操作。
    pub fn operation(&self) -> CustomerProfileOperation {
        self.operation
    }

    /// 返回修订目标客户 ID。
    ///
    /// # 返回
    /// 修订命令返回客户 ID，创建命令返回 `None`。
    pub fn customer_id(&self) -> Option<&str> {
        self.customer_id.as_deref()
    }

    /// 返回命令发起人。
    ///
    /// # 返回
    /// 返回已完成精确形态校验的账号 ID。
    pub fn initiated_by(&self) -> &str {
        &self.initiated_by
    }

    /// 返回当前请求指纹。
    ///
    /// # 返回
    /// 返回版本化请求指纹值对象。
    pub fn request_fingerprint(&self) -> &CustomerProfileRequestFingerprint {
        &self.request_fingerprint
    }
}

/// 已成功客户资料命令的结果数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CustomerProfileCommandResultData {
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

/// 已成功客户资料命令的数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CustomerProfileCommandData {
    /// 客户端幂等键。
    pub idempotency_key: String,
    /// 稳定操作名：`create` 或 `update`。
    pub operation: String,
    /// 发起命令的账号 ID。
    pub initiated_by: String,
    /// 规范化请求的 SHA-256 指纹。
    pub request_fingerprint: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

/// 已成功客户资料根级命令。
#[derive(Debug, Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct CustomerProfileCommand {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 客户端幂等键。
    pub idempotency_key: String,
    /// 稳定操作名。
    pub operation: String,
    /// 发起命令的账号 ID。
    pub initiated_by: String,
    /// 规范化请求指纹。
    pub request_fingerprint: String,
    /// 客户角色 ID。
    pub customer_id: String,
    /// 客户编号。
    pub customer_no: String,
    /// Party ID。
    pub party_id: String,
    /// 当前 Party 修订 ID。
    pub revision_id: String,
    /// 当前 Party 修订号。
    pub revision_no: u32,
    /// 保存后的客户乐观锁版本。
    pub customer_version: u64,
    /// 保存后的 Party 乐观锁版本。
    pub party_version: u64,
    /// 从属事实生效日期。
    pub effective_from: BusinessDate,
    /// 原始变更原因。
    pub change_reason: String,
}

impl CustomerProfileCommand {
    /// 创建客户资料命令的稳定成功结果。
    ///
    /// # Errors
    /// 幂等键、操作、请求指纹、结果身份或变更原因为空时返回校验错误。
    pub fn new(id: impl Into<String>, data: CustomerProfileCommandData) -> Result<Self> {
        let idempotency_key = normalize_required_text(
            data.idempotency_key,
            "幂等键不能为空",
            IDEMPOTENCY_KEY_MAX_LEN,
            "幂等键过长",
        )?;
        let operation = data.operation.as_str();
        if !matches!(operation, "create" | "update") {
            return Err(Error::from("客户资料命令操作无效"));
        }
        let fingerprint = CustomerProfileRequestFingerprint::parse_compatible(&data.request_fingerprint)?;
        let request_fingerprint = fingerprint.digest_hex().to_string();
        let required = [
            data.initiated_by.as_str(),
            data.customer_id.as_str(),
            data.customer_no.as_str(),
            data.party_id.as_str(),
            data.revision_id.as_str(),
            data.change_reason.as_str(),
        ];
        if required.iter().any(|value| value.trim().is_empty()) {
            return Err(Error::from("客户资料命令结果身份、请求指纹或变更原因不能为空"));
        }
        Ok(Self {
            base: BaseModel::new(id.into()),
            idempotency_key,
            operation: operation.to_string(),
            initiated_by: data.initiated_by,
            request_fingerprint,
            customer_id: data.customer_id,
            customer_no: data.customer_no,
            party_id: data.party_id,
            revision_id: data.revision_id,
            revision_no: data.revision_no,
            customer_version: data.customer_version,
            party_version: data.party_version,
            effective_from: data.effective_from,
            change_reason: data.change_reason,
        })
    }

    /// 记录由稳定重放上下文产生的客户资料成功结果。
    ///
    /// 创建命令接受新生成的客户 ID；修订命令要求结果客户与上下文目标精确
    /// 相同。新记录继续持久化历史裸 SHA-256 摘要以保持 BSON 与回滚兼容；
    /// 原始请求正文不得持久化。
    ///
    /// # 参数
    /// * `id` - 命令记录 ID
    /// * `context` - 已验证的请求重放身份
    /// * `result` - 事务成功后的稳定结果身份和版本
    ///
    /// # 返回
    /// 返回可与业务写入同事务持久化的命令记录。
    ///
    /// # 错误
    /// 修订客户不匹配或结果字段非法时返回错误。
    pub fn record_success(
        id: impl Into<String>,
        context: &CustomerProfileReplayContext,
        result: CustomerProfileCommandResultData,
    ) -> Result<Self> {
        if context
            .customer_id()
            .is_some_and(|customer_id| customer_id != result.customer_id.as_str())
        {
            return Err(Error::from("客户资料命令结果不属于修订目标客户"));
        }
        Self::new(
            id,
            CustomerProfileCommandData {
                idempotency_key: context.idempotency_key().to_string(),
                operation: context.operation().as_str().to_string(),
                initiated_by: context.initiated_by().to_string(),
                request_fingerprint: context.request_fingerprint().digest_hex().to_string(),
                customer_id: result.customer_id,
                customer_no: result.customer_no,
                party_id: result.party_id,
                revision_id: result.revision_id,
                revision_no: result.revision_no,
                customer_version: result.customer_version,
                party_version: result.party_version,
                effective_from: result.effective_from,
                change_reason: result.change_reason,
            },
        )
    }

    /// 核对已提交命令是否可安全重放给当前请求。
    ///
    /// 操作、修订客户、发起人和请求指纹必须全部一致；创建命令没有既有
    /// 客户 ID，因此只核对其余身份。当前带版本指纹与历史裸摘要按摘要兼容。
    ///
    /// # 参数
    /// * `context` - 当前请求构造的稳定重放身份
    ///
    /// # 返回
    /// 全部身份一致时返回 `Ok(())`。
    ///
    /// # 错误
    /// 任一身份不一致或持久化指纹损坏时返回错误。
    pub fn ensure_replay_matches(&self, context: &CustomerProfileReplayContext) -> Result<()> {
        let same_customer = context
            .customer_id()
            .is_none_or(|customer_id| customer_id == self.customer_id);
        let same_fingerprint = context
            .request_fingerprint()
            .matches_persisted(&self.request_fingerprint);
        if self.idempotency_key != context.idempotency_key()
            || self.operation != context.operation().as_str()
            || !same_customer
            || self.initiated_by != context.initiated_by()
            || !same_fingerprint
        {
            return Err(Error::from("客户资料幂等命令与当前请求不匹配"));
        }
        Ok(())
    }
}

/// 校验创建或修订命令的客户身份形状。
fn replay_customer_id(
    operation: CustomerProfileOperation,
    customer_id: Option<String>,
) -> Result<Option<String>> {
    match operation {
        CustomerProfileOperation::Create if customer_id.is_some() => {
            Err(Error::from("创建客户资料命令不能携带既有客户 ID"))
        }
        CustomerProfileOperation::Create => Ok(None),
        CustomerProfileOperation::Update => {
            let customer_id = customer_id.ok_or_else(|| Error::from("修订客户资料命令缺少客户 ID"))?;
            if customer_id.is_empty() || customer_id != customer_id.trim() {
                return Err(Error::from("修订客户资料命令缺少客户 ID"));
            }
            Ok(Some(customer_id))
        }
    }
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use crate::common::time::BusinessDate;

    use super::{
        CustomerProfileCommand, CustomerProfileCommandData, CustomerProfileCommandResultData,
        CustomerProfileOperation, CustomerProfileReplayContext, CustomerProfileRequestFingerprint,
        REQUEST_FINGERPRINT_V1_PREFIX, SHA256_HEX_LEN,
    };

    const ZERO_DIGEST: &str = "0000000000000000000000000000000000000000000000000000000000000000";
    const ONE_DIGEST: &str = "1111111111111111111111111111111111111111111111111111111111111111";

    #[derive(Serialize)]
    struct SensitiveRequest<'a> {
        legal_name: &'a str,
        mobile: &'a str,
        account_number: &'a str,
    }

    #[test]
    fn fingerprint_is_versioned_deterministic_and_accepts_legacy_digest() {
        let request = sensitive_request();
        let payload = serde_json::to_vec(&request).unwrap();
        let first = CustomerProfileRequestFingerprint::from_json_bytes_v1(&payload);
        let second = CustomerProfileRequestFingerprint::from_json_bytes_v1(&payload);

        assert_eq!(first, second);
        assert_eq!(
            first.as_str(),
            "sha256-json-v1:88bbb7a6b52992122ef2b8910ab733265e709a0ac511791e03baace7e4780670"
        );
        assert_eq!(
            CustomerProfileRequestFingerprint::parse_compatible(first.digest_hex())
                .unwrap()
                .as_str(),
            first.as_str()
        );
        assert!(CustomerProfileRequestFingerprint::parse_compatible("sha256-json-v2:00").is_err());
    }

    #[test]
    fn replay_context_enforces_create_and_update_customer_shapes() {
        let create = replay_context(CustomerProfileOperation::Create, None, "admin-1", ZERO_DIGEST);
        assert!(create.is_ok());
        assert!(replay_context(
            CustomerProfileOperation::Create,
            Some("customer-1"),
            "admin-1",
            ZERO_DIGEST
        )
        .is_err());
        assert!(replay_context(CustomerProfileOperation::Update, None, "admin-1", ZERO_DIGEST).is_err());
        assert!(replay_context(CustomerProfileOperation::Create, None, " admin-1", ZERO_DIGEST).is_err());
        assert!(replay_context(
            CustomerProfileOperation::Update,
            Some(" "),
            "admin-1",
            ZERO_DIGEST
        )
        .is_err());
        assert!(replay_context(
            CustomerProfileOperation::Update,
            Some(" customer-1 "),
            "admin-1",
            ZERO_DIGEST,
        )
        .is_err());
        let update = replay_context(
            CustomerProfileOperation::Update,
            Some("customer-1"),
            "admin-1",
            ZERO_DIGEST,
        )
        .unwrap();
        assert_eq!(update.customer_id(), Some("customer-1"));
        assert_eq!(update.idempotency_key(), "customer-save-1");
    }

    #[test]
    fn command_replay_requires_each_identity_dimension_to_match() {
        let matching = replay_context(
            CustomerProfileOperation::Update,
            Some("customer-1"),
            "admin-1",
            ZERO_DIGEST,
        )
        .unwrap();
        let command =
            CustomerProfileCommand::record_success("command-1", &matching, result_data("customer-1"))
                .unwrap();

        assert!(command.ensure_replay_matches(&matching).is_ok());
        assert!(command
            .ensure_replay_matches(
                &replay_context(CustomerProfileOperation::Create, None, "admin-1", ZERO_DIGEST).unwrap()
            )
            .is_err());
        assert!(command
            .ensure_replay_matches(
                &replay_context(
                    CustomerProfileOperation::Update,
                    Some("customer-2"),
                    "admin-1",
                    ZERO_DIGEST,
                )
                .unwrap()
            )
            .is_err());
        assert!(command
            .ensure_replay_matches(
                &replay_context(
                    CustomerProfileOperation::Update,
                    Some("customer-1"),
                    "admin-2",
                    ZERO_DIGEST,
                )
                .unwrap()
            )
            .is_err());
        assert!(command
            .ensure_replay_matches(
                &replay_context(
                    CustomerProfileOperation::Update,
                    Some("customer-1"),
                    "admin-1",
                    ONE_DIGEST,
                )
                .unwrap()
            )
            .is_err());
    }

    #[test]
    fn create_command_records_generated_customer_without_request_customer_id() {
        let request = sensitive_request();
        let payload = serde_json::to_vec(&request).unwrap();
        let fingerprint = CustomerProfileRequestFingerprint::from_json_bytes_v1(&payload);
        let context = CustomerProfileReplayContext::new(
            "customer-save-create",
            CustomerProfileOperation::Create,
            None,
            "admin-1",
            fingerprint,
        )
        .unwrap();
        let command = CustomerProfileCommand::record_success(
            "command-create",
            &context,
            result_data("generated-customer-1"),
        )
        .unwrap();

        assert_eq!(command.operation, "create");
        assert_eq!(command.customer_id, "generated-customer-1");
        assert_eq!(command.request_fingerprint.len(), SHA256_HEX_LEN);
        assert!(!command.request_fingerprint.contains(':'));
        assert!(command.ensure_replay_matches(&context).is_ok());

        let json = serde_json::to_string(&command).unwrap();
        let bson = bson::serialize_to_document(&command).unwrap();
        for secret in [request.mobile, request.account_number] {
            assert!(!json.contains(secret));
            assert!(!bson.to_string().contains(secret));
        }
        assert!(!bson.contains_key("request"));
        assert!(!bson.contains_key("mobile"));
        assert!(!bson.contains_key("account_number"));
        let bson_roundtrip: CustomerProfileCommand =
            bson::deserialize_from_document(bson).expect("当前命令 BSON 必须可回读");
        let json_roundtrip: CustomerProfileCommand =
            serde_json::from_str(&json).expect("当前命令 JSON 必须可回读");
        assert_eq!(bson_roundtrip, command);
        assert_eq!(json_roundtrip, command);
    }

    #[test]
    fn legacy_bare_fingerprint_replays_against_current_context_and_preserves_bson_shape() {
        let legacy = CustomerProfileCommand::new("command-legacy", command_data(ZERO_DIGEST)).unwrap();
        let prefixed = CustomerProfileCommand::new(
            "command-prefixed-input",
            command_data(&format!("{REQUEST_FINGERPRINT_V1_PREFIX}{ZERO_DIGEST}")),
        )
        .unwrap();
        let context = replay_context(
            CustomerProfileOperation::Update,
            Some("customer-1"),
            "admin-1",
            ZERO_DIGEST,
        )
        .unwrap();

        assert_eq!(legacy.request_fingerprint, ZERO_DIGEST);
        assert_eq!(prefixed.request_fingerprint, ZERO_DIGEST);
        assert!(legacy.ensure_replay_matches(&context).is_ok());
        let document = bson::serialize_to_document(&legacy).unwrap();
        assert_eq!(document.get_str("operation").unwrap(), "update");
        assert_eq!(document.get_str("request_fingerprint").unwrap(), ZERO_DIGEST);
        assert_eq!(document.get_str("customer_id").unwrap(), "customer-1");
        let roundtrip: CustomerProfileCommand =
            bson::deserialize_from_document(document).expect("历史命令 BSON 必须可回读");
        assert_eq!(roundtrip, legacy);
    }

    #[test]
    fn update_command_result_must_match_target_customer() {
        let context = replay_context(
            CustomerProfileOperation::Update,
            Some("customer-1"),
            "admin-1",
            ZERO_DIGEST,
        )
        .unwrap();
        assert!(CustomerProfileCommand::record_success(
            "command-wrong-customer",
            &context,
            result_data("customer-2"),
        )
        .is_err());
    }

    #[test]
    fn command_rejects_invalid_operation_key_and_fingerprint() {
        let blank_key = CustomerProfileCommandData {
            idempotency_key: " ".to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(CustomerProfileCommand::new("command-2", blank_key).is_err());

        let invalid_fingerprint = CustomerProfileCommandData {
            request_fingerprint: "not-a-sha256".to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(CustomerProfileCommand::new("command-3", invalid_fingerprint).is_err());

        let invalid_operation = CustomerProfileCommandData {
            operation: "delete".to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(CustomerProfileCommand::new("command-4", invalid_operation).is_err());

        let padded_operation = CustomerProfileCommandData {
            operation: " update ".to_string(),
            ..command_data(ZERO_DIGEST)
        };
        assert!(CustomerProfileCommand::new("command-5", padded_operation).is_err());

        let padded_fingerprint = CustomerProfileCommandData {
            request_fingerprint: format!(" {ZERO_DIGEST}"),
            ..command_data(ZERO_DIGEST)
        };
        assert!(CustomerProfileCommand::new("command-6", padded_fingerprint).is_err());
    }

    fn replay_context(
        operation: CustomerProfileOperation,
        customer_id: Option<&str>,
        initiated_by: &str,
        digest: &str,
    ) -> crate::errors::Result<CustomerProfileReplayContext> {
        CustomerProfileReplayContext::new(
            " customer-save-1 ",
            operation,
            customer_id.map(str::to_string),
            initiated_by,
            CustomerProfileRequestFingerprint::parse_compatible(digest).unwrap(),
        )
    }

    fn result_data(customer_id: &str) -> CustomerProfileCommandResultData {
        CustomerProfileCommandResultData {
            customer_id: customer_id.to_string(),
            customer_no: "KH-1".to_string(),
            party_id: "party-1".to_string(),
            revision_id: "revision-2".to_string(),
            revision_no: 2,
            customer_version: 2,
            party_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
            change_reason: "资料修订".to_string(),
        }
    }

    fn command_data(fingerprint: &str) -> CustomerProfileCommandData {
        CustomerProfileCommandData {
            idempotency_key: " customer-save-1 ".to_string(),
            operation: "update".to_string(),
            initiated_by: "admin-1".to_string(),
            request_fingerprint: fingerprint.to_string(),
            customer_id: "customer-1".to_string(),
            customer_no: "KH-1".to_string(),
            party_id: "party-1".to_string(),
            revision_id: "revision-2".to_string(),
            revision_no: 2,
            customer_version: 2,
            party_version: 2,
            effective_from: BusinessDate::from_ymd(2026, 8, 8).unwrap(),
            change_reason: "资料修订".to_string(),
        }
    }

    fn sensitive_request() -> SensitiveRequest<'static> {
        SensitiveRequest {
            legal_name: "Acme",
            mobile: "13800000000",
            account_number: "6222020000000000",
        }
    }
}
