//! 审批命令的规范幂等键、无碰撞载荷编码和 v3 身份。
//!
//! 本模块只处理不依赖 I/O 的命令身份规则。调用方必须先完成各字段自身的
//! 业务规范化，再按命令域固定的字段顺序构造载荷；本模块不会替调用方 trim
//! 载荷文本，也不会按集合遍历顺序重排 sequence。

use std::fmt;

use serde::de::Error as DeserializeError;
use serde::{Deserialize, Deserializer, Serialize};
use sha2::{Digest, Sha256};

use super::types::{ApprovalCommandKind, ModelError, ModelResult, SCOPE_MAX_LEN};

const V3_PREFIX: &str = "v3:";
const COMMAND_SCOPE_NAMESPACE: &[u8] = b"erp.approval.command-scope.v3";
const COMMAND_DIGEST_NAMESPACE: &[u8] = b"erp.approval.command-digest.v3";

/// 已 trim 且可直接用于命令收据唯一键的调用方幂等键。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct IdempotencyKey(String);

impl IdempotencyKey {
    /// 规范化外部幂等键。
    ///
    /// 首尾空白会被移除；规范结果必须包含 1..=128 个 UTF-8 字节。字符数不
    /// 代替字节数，以保持 HTTP、领域模型和 MongoDB 索引键使用同一边界。
    ///
    /// # 参数
    /// * `raw` - 外部幂等键，已拥有且无需 trim 时复用原字符串
    ///
    /// # 返回
    /// 返回包含规范幂等键的值对象。
    ///
    /// # 错误
    /// trim 后为空或超过 128 个 UTF-8 字节时返回 [`ModelError::InvalidField`]。
    pub fn parse(raw: impl Into<String>) -> ModelResult<Self> {
        let raw = raw.into();
        let canonical = raw.trim();
        if canonical.is_empty() {
            return Err(ModelError::InvalidField("幂等键不能为空"));
        }
        if canonical.len() > SCOPE_MAX_LEN {
            return Err(ModelError::InvalidField("幂等键过长"));
        }
        if canonical.len() == raw.len() { Ok(Self(raw)) } else { Ok(Self(canonical.to_string())) }
    }

    /// 返回可直接持久化和精确查询的规范键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 trim 后保存的幂等键。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for IdempotencyKey {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl PartialEq<str> for IdempotencyKey {
    fn eq(&self, other: &str) -> bool {
        self.as_str() == other
    }
}

impl PartialEq<&str> for IdempotencyKey {
    fn eq(&self, other: &&str) -> bool {
        self.as_str() == *other
    }
}

impl PartialEq<String> for IdempotencyKey {
    fn eq(&self, other: &String) -> bool {
        self.as_str() == other
    }
}

impl fmt::Display for IdempotencyKey {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

impl<'de> Deserialize<'de> for IdempotencyKey {
    /// 持久化值必须已经规范化；反序列化不得静默改写索引身份。
    ///
    /// # 参数
    /// * `deserializer` - 幂等键字符串的反序列化器
    ///
    /// # 返回
    /// 持久化字符串已是规范形态时返回 [`IdempotencyKey`]。
    ///
    /// # 错误
    /// 空值、超长，或含首尾空白因而不是规范形态时，返回反序列化错误。
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let persisted = String::deserialize(deserializer)?;
        let was_canonical = persisted.trim() == persisted;
        let parsed = Self::parse(persisted).map_err(D::Error::custom)?;
        if !was_canonical {
            return Err(D::Error::custom("持久化幂等键不是规范形态"));
        }
        Ok(parsed)
    }
}

/// 规范命令载荷中的一个带类型字段。
///
/// `Sequence` 保留调用方给定顺序，并对每个元素递归保留类型与边界。若业务
/// 语义要求集合无序，调用方必须先按该命令合同排序，再构造本字段。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CommandPayloadField<'a> {
    /// 原样 UTF-8 文本；不执行 trim。
    Text(&'a str),
    /// 无符号 32 位整数。
    U32(u32),
    /// 无符号 64 位整数。
    U64(u64),
    /// 显式区分 `None`、空文本和字面量 `NULL` 的可选文本。
    OptionalText(Option<&'a str>),
    /// 显式区分 `None` 和数值的可选无符号 64 位整数。
    OptionalU64(Option<u64>),
    /// 有序、可嵌套的字段序列。
    Sequence(Vec<CommandPayloadField<'a>>),
}

/// 已按字段类型、顺序和长度边界编码的规范命令载荷。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct CanonicalCommandPayload {
    encoded: Vec<u8>,
}

impl CanonicalCommandPayload {
    /// 创建空载荷 builder。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回尚未写入字段的载荷。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new() -> Self {
        Self::default()
    }

    /// 按调用顺序追加一个 typed 字段并返回 builder。
    ///
    /// 文本按原字节编码，绝不 trim；每个字段均包含独立类型标签和 u64 大端
    /// 长度前缀，因此分隔符、字段边界、可选值和整数类型均不能互相碰撞。
    ///
    /// # 参数
    /// * `field` - 按调用顺序追加的带类型字段
    ///
    /// # 返回
    /// 返回已追加该字段的载荷。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # Panics
    /// 字段值字节长度无法用 `u64` 回填时 panic。`usize` 到 `u64` 在目标平台上不会失败，
    /// 该分支只守住长度前缀写不进去的编程错误。
    pub fn field(mut self, field: CommandPayloadField<'_>) -> Self {
        encode_field(&field, &mut self.encoded);
        self
    }

    fn as_bytes(&self) -> &[u8] {
        &self.encoded
    }
}

/// v3 命令唯一作用域。新写固定为 `v3:` 加 64 位小写 SHA-256。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CommandScope(String);

impl CommandScope {
    /// 按命令种类、命令域与规范载荷形成 v3 作用域。
    ///
    /// # 参数
    /// * `command_kind` - 命令种类
    /// * `domain` - 命令域
    /// * `payload` - 作用域载荷
    ///
    /// # 返回
    /// 返回 `v3:` 加小写 SHA-256 的作用域。
    ///
    /// # 错误
    /// 命令域为空、含首尾空白或超过 128 个 UTF-8 字节时返回 [`ModelError::InvalidField`]。
    pub fn v3(
        command_kind: ApprovalCommandKind,
        domain: &str,
        payload: &CanonicalCommandPayload,
    ) -> ModelResult<Self> {
        Ok(Self(hashed_identity_value(COMMAND_SCOPE_NAMESPACE, command_kind, domain, payload)?))
    }

    /// 返回稳定持久化字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `v3:` 加小写 SHA-256 的作用域字符串。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommandScope {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CommandScope {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// v3 命令载荷摘要。新写固定为 `v3:` 加 64 位小写 SHA-256。
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize)]
#[serde(transparent)]
pub struct CommandDigest(String);

impl CommandDigest {
    /// 按命令种类、命令域与规范载荷形成 v3 摘要。
    ///
    /// 使用与 [`CommandScope`] 不同的 namespace，即便输入相同也不得产生相同
    /// 字符串。
    ///
    /// # 参数
    /// * `command_kind` - 命令种类
    /// * `domain` - 命令域
    /// * `payload` - 摘要载荷
    ///
    /// # 返回
    /// 返回 `v3:` 加小写 SHA-256 的摘要。
    ///
    /// # 错误
    /// 命令域为空、含首尾空白或超过 128 个 UTF-8 字节时返回 [`ModelError::InvalidField`]。
    pub fn v3(
        command_kind: ApprovalCommandKind,
        domain: &str,
        payload: &CanonicalCommandPayload,
    ) -> ModelResult<Self> {
        Ok(Self(hashed_identity_value(COMMAND_DIGEST_NAMESPACE, command_kind, domain, payload)?))
    }

    /// 返回稳定持久化字符串。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回 `v3:` 加小写 SHA-256 的摘要字符串。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl AsRef<str> for CommandDigest {
    fn as_ref(&self) -> &str {
        self.as_str()
    }
}

impl fmt::Display for CommandDigest {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(self.as_str())
    }
}

/// 新写命令收据所需的完整、已规范化审批命令身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ApprovalCommandIdentity {
    command_kind: ApprovalCommandKind,
    scope: CommandScope,
    idempotency_key: IdempotencyKey,
    digest: CommandDigest,
}

impl ApprovalCommandIdentity {
    /// 从固定命令域、规范 key 以及 scope/digest 的完整字段载荷构造 v3 身份。
    ///
    /// 同一命令域必须在所有调用点使用完全相同的字段顺序；scope 只承载唯一
    /// 资源范围，digest 必须承载全部会影响执行结果的请求字段。
    ///
    /// # 参数
    /// * `command_kind` - 命令种类
    /// * `domain` - 命令域，同一命令的所有调用点必须相同
    /// * `idempotency_key` - 已规范化的幂等键
    /// * `scope_payload` - 只承载唯一资源范围的载荷
    /// * `digest_payload` - 承载全部影响执行结果的请求字段
    ///
    /// # 返回
    /// 返回 v3 作用域与摘要都已算好的身份。
    ///
    /// # 错误
    /// 命令域非法时返回 [`ModelError::InvalidField`]。
    pub fn new(
        command_kind: ApprovalCommandKind,
        domain: &str,
        idempotency_key: IdempotencyKey,
        scope_payload: CanonicalCommandPayload,
        digest_payload: CanonicalCommandPayload,
    ) -> ModelResult<Self> {
        Ok(Self {
            command_kind,
            scope: CommandScope::v3(command_kind, domain, &scope_payload)?,
            idempotency_key,
            digest: CommandDigest::v3(command_kind, domain, &digest_payload)?,
        })
    }

    /// 返回权威命令种类。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时写入的 [`ApprovalCommandKind`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn command_kind(&self) -> ApprovalCommandKind {
        self.command_kind
    }

    /// 返回新写与当前格式查询使用的 v3 作用域。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时写入的 [`CommandScope`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn scope(&self) -> &CommandScope {
        &self.scope
    }

    /// 返回唯一键查询使用的规范幂等键。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时写入的 [`IdempotencyKey`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn idempotency_key(&self) -> &IdempotencyKey {
        &self.idempotency_key
    }

    /// 返回新写与当前格式回放比较使用的 v3 摘要。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 返回构造时写入的 [`CommandDigest`]。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn digest(&self) -> &CommandDigest {
        &self.digest
    }
}

/// 命令域必须非空、无首尾空白，且不超过 [`SCOPE_MAX_LEN`] 字节。
fn validate_domain(domain: &str) -> ModelResult<()> {
    if domain.is_empty() || domain.trim() != domain {
        return Err(ModelError::InvalidField("命令摘要域无效"));
    }
    if domain.len() > SCOPE_MAX_LEN {
        return Err(ModelError::InvalidField("命令摘要域过长"));
    }
    Ok(())
}

/// 校验命令域并按命名空间计算版本化哈希。作用域与摘要共用本规则，仅命名空间不同。
fn hashed_identity_value(
    namespace: &[u8],
    command_kind: ApprovalCommandKind,
    domain: &str,
    payload: &CanonicalCommandPayload,
) -> ModelResult<String> {
    validate_domain(domain)?;
    Ok(versioned_hash(namespace, command_kind, domain, payload))
}

/// 用长度前缀拼接命名空间、命令种类、命令域与载荷，得到 `v3:` 加小写 SHA-256。
fn versioned_hash(
    namespace: &[u8],
    command_kind: ApprovalCommandKind,
    domain: &str,
    payload: &CanonicalCommandPayload,
) -> String {
    let mut hasher = Sha256::new();
    update_length_prefixed(&mut hasher, namespace);
    update_length_prefixed(&mut hasher, command_kind.as_str().as_bytes());
    update_length_prefixed(&mut hasher, domain.as_bytes());
    update_length_prefixed(&mut hasher, payload.as_bytes());
    format!("{V3_PREFIX}{}", hex::encode(hasher.finalize()))
}

/// 先写入字节长度的大端 `u64`，再写入内容，避免不同分段拼接后碰撞。
fn update_length_prefixed(hasher: &mut Sha256, value: &[u8]) {
    hasher.update((value.len() as u64).to_be_bytes());
    hasher.update(value);
}

/// 直接追加字段编码并回填长度，保持类型标记、字段边界与序列顺序。
///
/// # Panics
/// 字段值字节长度无法用 `u64` 表示时 panic。`usize` 到 `u64` 在目标平台上不会失败，
/// 该分支只守住长度前缀写不进去的编程错误。
fn encode_field(field: &CommandPayloadField<'_>, target: &mut Vec<u8>) {
    target.push(match field {
        CommandPayloadField::Text(_) => 1,
        CommandPayloadField::U32(_) => 2,
        CommandPayloadField::U64(_) => 3,
        CommandPayloadField::OptionalText(_) => 4,
        CommandPayloadField::OptionalU64(_) => 5,
        CommandPayloadField::Sequence(_) => 6,
    });
    let length_offset = target.len();
    target.extend_from_slice(&[0; 8]);
    let value_start = target.len();
    match field {
        CommandPayloadField::Text(value) => target.extend_from_slice(value.as_bytes()),
        CommandPayloadField::U32(value) => target.extend_from_slice(&value.to_be_bytes()),
        CommandPayloadField::U64(value) => target.extend_from_slice(&value.to_be_bytes()),
        CommandPayloadField::OptionalText(value) => {
            target.push(u8::from(value.is_some()));
            if let Some(value) = value {
                target.extend_from_slice(value.as_bytes());
            }
        },
        CommandPayloadField::OptionalU64(value) => {
            target.push(u8::from(value.is_some()));
            if let Some(value) = value {
                target.extend_from_slice(&value.to_be_bytes());
            }
        },
        CommandPayloadField::Sequence(fields) => {
            target.extend_from_slice(&(fields.len() as u64).to_be_bytes());
            for field in fields {
                encode_field(field, target);
            }
        },
    }
    let value_len = u64::try_from(target.len() - value_start).expect("载荷字节长度不超过 u64");
    target[length_offset..value_start].copy_from_slice(&value_len.to_be_bytes());
}

#[cfg(test)]
mod tests {
    use hex::encode;
    use serde_json::json;

    use super::{ApprovalCommandIdentity, CanonicalCommandPayload, CommandPayloadField, IdempotencyKey};
    use crate::model::types::ApprovalCommandKind;

    #[test]
    fn idempotency_key_normalizes_external_input_and_enforces_utf8_byte_boundary() {
        assert_eq!(IdempotencyKey::parse("  key-1  ").unwrap().as_str(), "key-1");
        assert_eq!(IdempotencyKey::parse("界".repeat(42)).unwrap().as_str().len(), 126);
        assert!(IdempotencyKey::parse("界".repeat(43)).is_err());
        assert!(IdempotencyKey::parse(" \t\n ").is_err());
        assert!(IdempotencyKey::parse("k".repeat(128)).is_ok());
        assert!(IdempotencyKey::parse("k".repeat(129)).is_err());
    }

    #[test]
    fn persisted_idempotency_key_must_already_be_canonical() {
        let key: IdempotencyKey = serde_json::from_value(json!("key-1")).unwrap();
        assert_eq!(key.as_str(), "key-1");
        assert!(serde_json::from_value::<IdempotencyKey>(json!(" key-1 ")).is_err());
    }

    /// 固定字节向量覆盖全部字段类型与嵌套序列，保护既有命令身份编码。
    #[test]
    fn canonical_payload_all_field_types_match_stable_bytes() {
        let payload = CanonicalCommandPayload::new()
            .field(CommandPayloadField::Text("界"))
            .field(CommandPayloadField::U32(0x01020304))
            .field(CommandPayloadField::U64(0x0102030405060708))
            .field(CommandPayloadField::OptionalText(None))
            .field(CommandPayloadField::OptionalText(Some("")))
            .field(CommandPayloadField::OptionalText(Some("é")))
            .field(CommandPayloadField::OptionalU64(None))
            .field(CommandPayloadField::OptionalU64(Some(0)))
            .field(CommandPayloadField::Sequence(vec![
                CommandPayloadField::Text("nested"),
                CommandPayloadField::Sequence(vec![
                    CommandPayloadField::U32(9),
                    CommandPayloadField::OptionalText(None),
                ]),
            ]));
        assert_eq!(
            encode(payload.as_bytes()),
            concat!(
                "010000000000000003e7958c",
                "02000000000000000401020304",
                "0300000000000000080102030405060708",
                "04000000000000000100",
                "04000000000000000101",
                "04000000000000000301c3a9",
                "05000000000000000100",
                "050000000000000009010000000000000000",
                "06000000000000003f0000000000000002",
                "0100000000000000066e6573746564",
                "06000000000000001f0000000000000002",
                "02000000000000000400000009",
                "04000000000000000100",
            )
        );
    }

    #[test]
    fn typed_length_prefixes_prevent_text_optional_and_integer_collisions() {
        let digest = |payload| {
            ApprovalCommandIdentity::new(
                ApprovalCommandKind::SubmitDecision,
                "approval.submit-decision",
                IdempotencyKey::parse("key-1").unwrap(),
                CanonicalCommandPayload::new().field(CommandPayloadField::Text("instance-1")),
                payload,
            )
            .unwrap()
            .digest()
            .as_str()
            .to_string()
        };

        assert_ne!(
            digest(
                CanonicalCommandPayload::new()
                    .field(CommandPayloadField::Text("a\u{1f}b"))
                    .field(CommandPayloadField::Text("c"))
            ),
            digest(
                CanonicalCommandPayload::new()
                    .field(CommandPayloadField::Text("a"))
                    .field(CommandPayloadField::Text("b\u{1f}c"))
            )
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::OptionalText(None))),
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::OptionalText(Some("NULL"))))
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::Text("1"))),
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::U32(1)))
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::U32(1))),
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::U64(1)))
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::OptionalU64(None))),
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::OptionalU64(Some(0))))
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::Text("é"))),
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::Text("e\u{301}")))
        );
        assert_ne!(
            digest(CanonicalCommandPayload::new().field(CommandPayloadField::Sequence(vec![
                CommandPayloadField::Text("a"),
                CommandPayloadField::Text("b"),
            ]))),
            digest(
                CanonicalCommandPayload::new()
                    .field(CommandPayloadField::Text("a"))
                    .field(CommandPayloadField::Text("b"))
            )
        );
    }

    #[test]
    fn canonical_payload_preserves_whitespace_unicode_and_sequence_order() {
        let digest = |text, sequence| {
            ApprovalCommandIdentity::new(
                ApprovalCommandKind::UpgradeBinding,
                "approval.upgrade-binding",
                IdempotencyKey::parse("key-1").unwrap(),
                CanonicalCommandPayload::new().field(CommandPayloadField::Text("document-1")),
                CanonicalCommandPayload::new()
                    .field(CommandPayloadField::Text(text))
                    .field(CommandPayloadField::Sequence(sequence)),
            )
            .unwrap()
            .digest()
            .as_str()
            .to_string()
        };

        assert_ne!(
            digest(" 审批 ", vec![CommandPayloadField::Text("甲"), CommandPayloadField::Text("乙")]),
            digest("审批", vec![CommandPayloadField::Text("甲"), CommandPayloadField::Text("乙")])
        );
        assert_ne!(
            digest("审批", vec![CommandPayloadField::Text("甲"), CommandPayloadField::Text("乙")]),
            digest("审批", vec![CommandPayloadField::Text("乙"), CommandPayloadField::Text("甲")])
        );
    }

    #[test]
    fn v3_scope_and_digest_use_distinct_namespaces_and_stable_golden() {
        let identity = ApprovalCommandIdentity::new(
            ApprovalCommandKind::SubmitDecision,
            "approval.submit-decision",
            IdempotencyKey::parse("key-1").unwrap(),
            CanonicalCommandPayload::new()
                .field(CommandPayloadField::Text("instance-1"))
                .field(CommandPayloadField::U64(7)),
            CanonicalCommandPayload::new()
                .field(CommandPayloadField::Text("execution-1"))
                .field(CommandPayloadField::Text("APPROVE"))
                .field(CommandPayloadField::OptionalText(Some("同意")))
                .field(CommandPayloadField::U64(11))
                .field(CommandPayloadField::Text("actor-1")),
        )
        .unwrap();

        assert_eq!(identity.scope().as_str().len(), 67);
        assert_eq!(identity.digest().as_str().len(), 67);
        assert!(identity.scope().as_str().starts_with("v3:"));
        assert!(identity.digest().as_str().starts_with("v3:"));
        assert_ne!(identity.scope().as_str(), identity.digest().as_str());
        assert_eq!(
            identity.scope().as_str(),
            "v3:34861984440bd5616d9db6f015567756b53c9eccb21ea850d05e77996c803a3c"
        );
        assert_eq!(
            identity.digest().as_str(),
            "v3:e7665ff804b4fee3f41915a21b4f550910097700ee6a017272e8f7342bd59cf0"
        );
    }
}
