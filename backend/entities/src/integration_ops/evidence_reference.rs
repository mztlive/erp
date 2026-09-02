//! W29 受控证据引用的精确 grammar、主体关联与集合规范化（INT-E20）。
//!
//! 客户端记录 ID 只接受 `type:id`；已持久化事实引用兼容历史 `type://id`、
//! `type:id`、`type:id:status` 与 `type:id:vN:status`。关联只比较身份 ID，
//! 禁止把类型名、版本段或状态段当作命中。集合编码排序去重，总长不超过 512。
//! 原动作重放把 inbox canonical 与不透明 `business_fact_key` 分开持有，键内 `|`
//! 不得走 `parse_id` 或 [`EvidenceReferenceSet`]。

use std::fmt;

use crate::errors::{Error, Result};

/// 单条引用或集合编码的最大 UTF-8 字节数。
const ENCODED_MAX_LEN: usize = 512;
/// 对象类型 / 证据类型代码最大长度。
const KIND_MAX_LEN: usize = 64;
/// 身份 ID 最大长度。
const ID_MAX_LEN: usize = 256;
/// 状态段最大长度。
const STATUS_MAX_LEN: usize = 64;
/// 会切断或伪造引用边界的字符。
const DELIMITERS: &[char] = &[':', '/', ';', '|', ',', '='];

/// 客户端提交的证据记录引用：精确 `type:id`。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct EvidenceRecordRef {
    kind: String,
    id: String,
}

impl EvidenceRecordRef {
    /// 由已校验的对象类型与记录 ID 构造客户端引用。
    ///
    /// # 参数
    /// * `kind` - 对象类型（如 `inbox_message`）
    /// * `id` - 记录身份 ID
    ///
    /// # 返回
    /// 返回可编码为 `type:id` 的引用。
    ///
    /// # 错误
    /// 类型或 ID 为空、超长、含分隔符或嵌套冒号时返回领域校验错误。
    ///
    /// # 约束
    /// 不接受 `type://id` 或带版本/状态的 canonical 形态。
    pub fn new(kind: &str, id: &str) -> Result<Self> {
        Ok(Self {
            kind: parse_kind(kind.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            id: parse_id(id.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
        })
    }

    /// 解析客户端证据记录 ID。
    ///
    /// # 参数
    /// * `value` - 原始记录 ID 字符串
    ///
    /// # 返回
    /// 返回去除首尾空白后的 `type:id` 引用。
    ///
    /// # 错误
    /// 缺少冒号、空段、嵌套 ID 或分隔符注入时返回领域校验错误。
    ///
    /// # 约束
    /// 只允许恰好一段类型和一段 ID；额外 `:` 一律拒绝。
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (kind, id) = value
            .split_once(':')
            .ok_or_else(|| Error::from("证据记录 ID 必须使用 type:id 格式"))?;
        if id.contains(':') {
            return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式"));
        }
        Self::new(kind, id)
    }

    /// 返回对象类型。
    ///
    /// # 返回
    /// 返回已规范化的类型代码。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 只读访问，不重新解析。
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// 返回记录身份 ID。
    ///
    /// # 返回
    /// 返回已规范化的身份 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不包含类型、版本或状态段。
    pub fn id(&self) -> &str {
        &self.id
    }
}

impl fmt::Display for EvidenceRecordRef {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(formatter, "{}:{}", self.kind, self.id)
    }
}

/// 规范化证据引用：身份加上可选版本与状态。
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct CanonicalEvidenceReference {
    kind: String,
    id: String,
    version: Option<u64>,
    status: Option<String>,
}

impl CanonicalEvidenceReference {
    /// 构造已验证的 canonical 引用。
    ///
    /// # 参数
    /// * `kind` - 对象类型
    /// * `id` - 记录身份 ID
    /// * `version` - 正式版本；无版本形态传 `None`
    /// * `status` - 终态或核验状态段
    ///
    /// # 返回
    /// 返回可稳定编码的 canonical 引用。
    ///
    /// # 错误
    /// 任一段为空、超长或含分隔符时返回领域校验错误。
    ///
    /// # 约束
    /// 编码为 `type:id:status` 或 `type:id:vN:status`，不写历史 `://` 形态。
    pub fn verified(kind: &str, id: &str, version: Option<u64>, status: &str) -> Result<Self> {
        let reference = Self {
            kind: parse_kind(kind.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            id: parse_id(id.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?,
            version,
            status: Some(parse_status(status.trim())?),
        };
        ensure_encoded_len(&reference.encode())?;
        Ok(reference)
    }

    /// 解析已持久化或历史证据引用。
    ///
    /// # 参数
    /// * `value` - 存量 `type://id`、`type:id`、`type:id:status` 或 `type:id:vN:status`
    ///
    /// # 返回
    /// 返回按精确 grammar 解析的引用；身份 ID 不含版本或状态。
    ///
    /// # 错误
    /// 空值、超长、分隔符注入、无法识别的段数或非法版本段时返回领域校验错误。
    ///
    /// # 约束
    /// 不把 `://` 之后的内容再按冒号切开；嵌套 ID 失败关闭。
    pub fn parse_stored(value: &str) -> Result<Self> {
        let value = value.trim();
        if value.is_empty() {
            return Err(Error::from("证据记录 ID 必须使用 type:id 格式"));
        }
        ensure_encoded_len(value)?;
        if let Some((kind, id)) = value.split_once("://") {
            return Ok(Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: None,
            });
        }
        let parts = value.split(':').collect::<Vec<_>>();
        let reference = match parts.as_slice() {
            [kind, id] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: None,
            },
            [kind, id, status] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: None,
                status: Some(parse_status(status)?),
            },
            [kind, id, version, status] => Self {
                kind: parse_kind(kind, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                id: parse_id(id, "证据记录 ID 必须使用唯一的 type:id 格式")?,
                version: Some(parse_version_token(version)?),
                status: Some(parse_status(status)?),
            },
            _ => return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式")),
        };
        Ok(reference)
    }

    /// 返回对象类型。
    ///
    /// # 返回
    /// 返回已规范化的类型代码。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不包含 `://` 或版本前缀。
    pub fn kind(&self) -> &str {
        &self.kind
    }

    /// 返回身份 ID。
    ///
    /// # 返回
    /// 返回与主体关联比较使用的精确 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不含类型名、`vN` 或状态段，因此不会被错误 substring/token 命中。
    pub fn id(&self) -> &str {
        &self.id
    }

    /// 返回可选正式版本。
    ///
    /// # 返回
    /// 有版本段时返回版本号。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 历史 `://` 身份引用没有版本。
    pub fn version(&self) -> Option<u64> {
        self.version
    }

    /// 返回可选状态段。
    ///
    /// # 返回
    /// 有状态段时返回状态文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 状态不参与主体关联比较。
    pub fn status(&self) -> Option<&str> {
        self.status.as_deref()
    }

    /// 消费引用并返回 canonical 编码。
    ///
    /// # 返回
    /// 返回可写入终态证据字段的稳定字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不输出历史 `://` 形态；与 [`fmt::Display`] 编码规则相同。
    pub fn into_wire(self) -> String {
        self.encode()
    }

    /// 按冻结段序编码 canonical 文本。
    ///
    /// # 返回
    /// 返回 `type:id`、`type:id:status` 或 `type:id:vN:status`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 版本使用十进制无前导零（零本身为 `v0`）。
    fn encode(&self) -> String {
        match (self.version, self.status.as_deref()) {
            (Some(version), Some(status)) => {
                format!("{}:{}:v{}:{}", self.kind, self.id, version, status)
            }
            (None, Some(status)) => format!("{}:{}:{}", self.kind, self.id, status),
            (Some(version), None) => format!("{}:{}:v{}", self.kind, self.id, version),
            (None, None) => format!("{}:{}", self.kind, self.id),
        }
    }
}

impl fmt::Display for CanonicalEvidenceReference {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        formatter.write_str(&self.encode())
    }
}

/// 原动作重放结果：canonical inbox 引用加上不透明业务事实键。
///
/// 事实键可含 `|` 等 inbox 约定分隔符，因此不得作为证据 ID 解析，也不得
/// 作为 [`EvidenceReferenceSet`] 成员编码。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ReplayOriginalReference {
    inbox: CanonicalEvidenceReference,
    business_fact_key: String,
}

impl ReplayOriginalReference {
    /// 由入站消息身份与已规范化业务事实键构造重放引用。
    ///
    /// # 参数
    /// * `inbox_id` - 入站消息 ID
    /// * `version` - 入站消息正式版本
    /// * `business_fact_key` - 不透明业务事实键，允许含 `|`
    ///
    /// # 返回
    /// 返回 inbox canonical 与事实键分离持有的重放引用。
    ///
    /// # 错误
    /// inbox 身份非法或事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 事实键只去首尾空白，不按证据 ID 解析；inbox 段由
    /// [`CanonicalEvidenceReference::verified`] 编码为 `inbox_message:id:vN:requeued`。
    pub fn new(inbox_id: &str, version: u64, business_fact_key: &str) -> Result<Self> {
        let inbox =
            CanonicalEvidenceReference::verified("inbox_message", inbox_id, Some(version), "requeued")?;
        let business_fact_key = business_fact_key.trim();
        if business_fact_key.is_empty() {
            return Err(Error::from("业务事实键不能为空"));
        }
        Ok(Self {
            inbox,
            business_fact_key: business_fact_key.to_string(),
        })
    }

    /// 解析重放引用的持久化形态。
    ///
    /// # 参数
    /// * `value` - `{inbox_canonical};business_fact_key:{opaque_key}`
    ///
    /// # 返回
    /// 返回 inbox canonical 与完整事实键。
    ///
    /// # 错误
    /// 缺少固定分隔、inbox grammar 非法或事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `split_once(";business_fact_key:")`，事实键可含 `|`、额外 `;` 或 `:`。
    pub fn parse(value: &str) -> Result<Self> {
        let value = value.trim();
        let (inbox, business_fact_key) = value
            .split_once(";business_fact_key:")
            .ok_or_else(|| Error::from("重放引用必须包含 business_fact_key 字段"))?;
        Self::from_parts(
            CanonicalEvidenceReference::parse_stored(inbox)?,
            business_fact_key,
        )
    }

    /// 返回 inbox canonical 引用。
    ///
    /// # 返回
    /// 返回 `inbox_message:id:vN:requeued` 对应的值对象。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 只读；不重新编码事实键。
    pub fn inbox(&self) -> &CanonicalEvidenceReference {
        &self.inbox
    }

    /// 返回不透明业务事实键。
    ///
    /// # 返回
    /// 返回构造时冻结的事实键，含 `|` 等原字符。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不得把返回值当作证据 ID 再解析。
    pub fn business_fact_key(&self) -> &str {
        &self.business_fact_key
    }

    /// 消费并返回可写入业务结果引用的编码。
    ///
    /// # 返回
    /// 返回 `{inbox};business_fact_key:{key}`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不把整串交给 [`EvidenceReferenceSet`]；事实键保持原文。
    pub fn into_wire(self) -> String {
        format!("{};business_fact_key:{}", self.inbox, self.business_fact_key)
    }

    /// 由已解析 inbox 与原文事实键组装重放引用。
    ///
    /// # 参数
    /// * `inbox` - 已校验的 inbox canonical
    /// * `business_fact_key` - 分隔符之后的原文
    ///
    /// # 返回
    /// 返回重放引用。
    ///
    /// # 错误
    /// 事实键为空时返回领域校验错误。
    ///
    /// # 约束
    /// 不把事实键按证据 ID 解析。
    fn from_parts(inbox: CanonicalEvidenceReference, business_fact_key: &str) -> Result<Self> {
        let business_fact_key = business_fact_key.trim();
        if business_fact_key.is_empty() {
            return Err(Error::from("业务事实键不能为空"));
        }
        Ok(Self {
            inbox,
            business_fact_key: business_fact_key.to_string(),
        })
    }
}

/// 终态证据引用集合：分号连接、排序、去重、512 字节上限。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceReferenceSet {
    encoded: String,
}

impl EvidenceReferenceSet {
    /// 由 canonical 引用构造可持久化终态集合。
    ///
    /// # 参数
    /// * `refs` - 已验证的 canonical 引用
    ///
    /// # 返回
    /// 返回排序去重后的集合。
    ///
    /// # 错误
    /// 空集合或编码超过 512 字节时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `;` 连接以兼容存量终态证据字段；成员 ID 不得含 `;`。
    pub fn try_from_canonical<I>(refs: I) -> Result<Self>
    where
        I: IntoIterator<Item = CanonicalEvidenceReference>,
    {
        let encoded = encode_set(
            refs.into_iter().map(CanonicalEvidenceReference::into_wire),
            ';',
            true,
            "终态证据引用为空或过长",
        )?;
        Ok(Self { encoded })
    }

    /// 返回可持久化编码。
    ///
    /// # 返回
    /// 返回分号连接的 canonical 文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 编码已在构造时通过长度门禁。
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// 消费集合并返回可持久化编码。
    ///
    /// # 返回
    /// 返回所有权字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 与 [`Self::as_str`] 相同。
    pub fn into_wire(self) -> String {
        self.encoded
    }
}

/// 非终结动作摘要使用的紧凑证据集合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CompactEvidenceSet {
    encoded: String,
}

impl CompactEvidenceSet {
    /// 由证据类型代码与记录 ID 构造紧凑集合。
    ///
    /// # 参数
    /// * `members` - `(证据类型代码, 记录 ID)`；记录 ID 必须是 `type:id`
    ///
    /// # 返回
    /// 空输入返回 `Ok(None)`；否则返回排序去重后的集合。
    ///
    /// # 错误
    /// 成员 grammar 非法或编码超过 512 字节时返回领域校验错误。
    ///
    /// # 约束
    /// 使用 `,` 连接以保持既有动作摘要形态；成员不得含 `,`。
    pub fn try_from_pairs<'a, I>(members: I) -> Result<Option<Self>>
    where
        I: IntoIterator<Item = (&'a str, &'a str)>,
    {
        let mut encoded = Vec::new();
        for (kind_code, record_id) in members {
            encoded.push(encode_compact_member(kind_code, record_id)?);
        }
        if encoded.is_empty() {
            return Ok(None);
        }
        let encoded = encode_set(encoded, ',', false, "证据引用汇总过长")?;
        Ok(Some(Self { encoded }))
    }

    /// 返回紧凑编码。
    ///
    /// # 返回
    /// 返回逗号连接的摘要文本。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 编码已在构造时通过长度门禁。
    pub fn as_str(&self) -> &str {
        &self.encoded
    }

    /// 消费集合并返回紧凑编码。
    ///
    /// # 返回
    /// 返回所有权字符串。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 与 [`Self::as_str`] 相同。
    pub fn into_wire(self) -> String {
        self.encoded
    }
}

/// 当前业务项可参与授权关联的稳定身份。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct EvidenceSubjectBindings {
    message_id: Option<String>,
    business_object_id: Option<String>,
    fact_identities: Vec<CanonicalEvidenceReference>,
    unparsed_fact_references: Vec<String>,
}

impl EvidenceSubjectBindings {
    /// 由主体身份字段构造关联绑定。
    ///
    /// # 参数
    /// * `message_id` - 错误任务入站消息 ID
    /// * `business_object_id` - 任务或差异业务对象 ID
    /// * `fact_references` - 差异两侧已持久化事实引用
    ///
    /// # 返回
    /// 返回只读绑定；非法存量引用不会导致构造失败，但也不会按 token 命中。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 空白身份视为缺失；可解析引用只暴露身份 ID。
    pub fn new(
        message_id: Option<&str>,
        business_object_id: Option<&str>,
        fact_references: impl IntoIterator<Item = impl AsRef<str>>,
    ) -> Self {
        let mut fact_identities = Vec::new();
        let mut unparsed_fact_references = Vec::new();
        for reference in fact_references {
            let raw = reference.as_ref().trim();
            if raw.is_empty() {
                continue;
            }
            match CanonicalEvidenceReference::parse_stored(raw) {
                Ok(parsed) => fact_identities.push(parsed),
                Err(_) => unparsed_fact_references.push(raw.to_string()),
            }
        }
        Self {
            message_id: normalize_optional_identity(message_id),
            business_object_id: normalize_optional_identity(business_object_id),
            fact_identities,
            unparsed_fact_references,
        }
    }

    /// 判断候选 ID 是否与主体存在可验证的正式关联。
    ///
    /// # 参数
    /// * `ids` - 证据记录及其正式关联对象的候选 ID
    ///
    /// # 返回
    /// 任一非空候选精确等于消息 ID、业务对象 ID 或事实引用身份 ID 时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不匹配类型名、版本段、状态段或分隔后的子串；空候选永不命中。
    pub fn associates_any<'a, I>(&self, ids: I) -> bool
    where
        I: IntoIterator<Item = &'a str>,
    {
        ids.into_iter().any(|id| self.associates_id(id))
    }

    /// 按对象类型提取第一条事实引用的身份 ID。
    ///
    /// # 参数
    /// * `kind` - 期望的对象类型
    ///
    /// # 返回
    /// 返回首次匹配类型的身份 ID。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 不返回 `vN` 或状态后缀，避免把 canonical 文本整段当作查找键。
    pub fn referenced_id(&self, kind: &str) -> Option<&str> {
        self.fact_identities
            .iter()
            .find(|reference| reference.kind() == kind)
            .map(CanonicalEvidenceReference::id)
    }

    /// 精确比较单个候选 ID。
    ///
    /// # 参数
    /// * `id` - 候选身份
    ///
    /// # 返回
    /// 精确命中时返回 `true`。
    ///
    /// # 错误
    /// 无。
    ///
    /// # 约束
    /// 无法解析的存量引用只允许整串精确相等，禁止 delimiter split。
    fn associates_id(&self, id: &str) -> bool {
        let id = id.trim();
        if id.is_empty() {
            return false;
        }
        self.message_id.as_deref() == Some(id)
            || self.business_object_id.as_deref() == Some(id)
            || self.fact_identities.iter().any(|reference| reference.id() == id)
            || self
                .unparsed_fact_references
                .iter()
                .any(|reference| reference.as_str() == id)
    }
}

/// 规范化可选身份，空白视为缺失。
///
/// # 参数
/// * `value` - 原始可选身份
///
/// # 返回
/// 非空时返回去空白后的身份。
///
/// # 错误
/// 无。
///
/// # 约束
/// 不解析 grammar，只处理主体字段。
fn normalize_optional_identity(value: Option<&str>) -> Option<String> {
    value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string)
}

/// 编码紧凑集合的单个成员。
///
/// # 参数
/// * `kind_code` - 证据类型稳定代码
/// * `record_id` - 客户端 `type:id`
///
/// # 返回
/// 返回 `KIND:type:id`。
///
/// # 错误
/// 类型代码或记录 ID 非法时返回领域校验错误。
///
/// # 约束
/// 类型代码不得含分隔符，避免与记录 ID 抢段。
fn encode_compact_member(kind_code: &str, record_id: &str) -> Result<String> {
    let kind_code = parse_kind(kind_code.trim(), "证据记录 ID 必须使用唯一的 type:id 格式")?;
    let record = EvidenceRecordRef::parse(record_id)?;
    Ok(format!("{kind_code}:{record}"))
}

/// 对集合成员排序去重后按固定分隔符连接。
///
/// # 参数
/// * `members` - 已编码的成员
/// * `separator` - `;` 或 `,`
/// * `reject_empty` - 空集合是否失败
/// * `length_message` - 超长或空集合错误
///
/// # 返回
/// 返回规范化编码。
///
/// # 错误
/// 空集合被拒绝或超过 512 字节时返回领域校验错误。
///
/// # 约束
/// 排序使用字典序，重复成员只保留一次。
fn encode_set(
    members: impl IntoIterator<Item = String>,
    separator: char,
    reject_empty: bool,
    length_message: &str,
) -> Result<String> {
    let mut encoded = members.into_iter().collect::<Vec<_>>();
    encoded.sort();
    encoded.dedup();
    if encoded.is_empty() {
        if reject_empty {
            return Err(Error::from(length_message));
        }
        return Ok(String::new());
    }
    let value = encoded.join(&separator.to_string());
    if value.len() > ENCODED_MAX_LEN {
        return Err(Error::from(length_message));
    }
    Ok(value)
}

/// 校验单条编码未超过 512 字节。
///
/// # 参数
/// * `value` - 已编码文本
///
/// # 返回
/// 长度合法时返回 `Ok(())`。
///
/// # 错误
/// 超过 512 字节时返回领域校验错误。
///
/// # 约束
/// 按 UTF-8 字节计数，与既有 `.len()` 门禁一致。
fn ensure_encoded_len(value: &str) -> Result<()> {
    if value.len() > ENCODED_MAX_LEN {
        return Err(Error::from("终态证据引用为空或过长"));
    }
    Ok(())
}

/// 解析对象类型或证据类型代码。
///
/// # 参数
/// * `value` - 类型段
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验的类型代码。
///
/// # 错误
/// 空、超长、非标识或含分隔符时返回领域校验错误。
///
/// # 约束
/// 允许字母开头的 ASCII 字母数字、`_` 与 `-`，覆盖 `mall_order_fact` 与 `mall-snapshot`。
fn parse_kind(value: &str, message: &str) -> Result<String> {
    parse_identifier(value, KIND_MAX_LEN, message)
}

/// 解析状态段。
///
/// # 参数
/// * `value` - 状态文本
///
/// # 返回
/// 返回已校验的状态段。
///
/// # 错误
/// 空、超长或非法标识时返回领域校验错误。
///
/// # 约束
/// 与类型段同一标识规则，避免状态被当成新的分隔字段。
fn parse_status(value: &str) -> Result<String> {
    parse_identifier(value, STATUS_MAX_LEN, "证据记录 ID 必须使用唯一的 type:id 格式")
}

/// 解析标识段。
///
/// # 参数
/// * `value` - 原始段
/// * `max_len` - 最大长度
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验文本。
///
/// # 错误
/// 不符合标识规则时返回领域校验错误。
///
/// # 约束
/// 首字符必须是 ASCII 字母。
fn parse_identifier(value: &str, max_len: usize, message: &str) -> Result<String> {
    if value.is_empty() || value.len() > max_len {
        return Err(Error::from(message));
    }
    let mut characters = value.chars();
    let Some(first) = characters.next() else {
        return Err(Error::from(message));
    };
    if !first.is_ascii_alphabetic()
        || !characters
            .all(|character| character.is_ascii_alphanumeric() || character == '_' || character == '-')
    {
        return Err(Error::from(message));
    }
    Ok(value.to_string())
}

/// 解析身份 ID。
///
/// # 参数
/// * `value` - 原始 ID
/// * `message` - 失败消息
///
/// # 返回
/// 返回已校验 ID。
///
/// # 错误
/// 空、超长或含分隔符/空白时返回领域校验错误。
///
/// # 约束
/// 允许 Unicode，但禁止 `:` `/` `;` `|` `,` `=` 与空白，杜绝集合拆分注入。
fn parse_id(value: &str, message: &str) -> Result<String> {
    if value.is_empty() || value.len() > ID_MAX_LEN || value.chars().any(is_delimiter) {
        return Err(Error::from(message));
    }
    Ok(value.to_string())
}

/// 解析 canonical 版本段。
///
/// # 参数
/// * `value` - `v` 前缀加十进制数字
///
/// # 返回
/// 返回版本号。
///
/// # 错误
/// 缺少 `v`、非数字、前导零或溢出时返回领域校验错误。
///
/// # 约束
/// `v0` 合法；`v01` 非法，避免同一版本多种写法。
fn parse_version_token(value: &str) -> Result<u64> {
    let digits = value
        .strip_prefix('v')
        .ok_or_else(|| Error::from("证据记录 ID 必须使用唯一的 type:id 格式"))?;
    if digits.is_empty()
        || !digits.bytes().all(|byte| byte.is_ascii_digit())
        || (digits.len() > 1 && digits.starts_with('0'))
    {
        return Err(Error::from("证据记录 ID 必须使用唯一的 type:id 格式"));
    }
    digits
        .parse()
        .map_err(|_| Error::from("证据记录 ID 必须使用唯一的 type:id 格式"))
}

/// 判断字符是否会切断引用边界。
///
/// # 参数
/// * `character` - 待检查字符
///
/// # 返回
/// 空白或冻结分隔符时返回 `true`。
///
/// # 错误
/// 无。
///
/// # 约束
/// 与历史 substring 拆分字符对齐，但只用于拒绝而不是命中。
fn is_delimiter(character: char) -> bool {
    character.is_whitespace() || DELIMITERS.contains(&character)
}

#[cfg(test)]
mod tests {
    use super::{
        CanonicalEvidenceReference, CompactEvidenceSet, EvidenceRecordRef, EvidenceReferenceSet,
        EvidenceSubjectBindings, ReplayOriginalReference, ENCODED_MAX_LEN,
    };

    fn bindings(facts: &[&str]) -> EvidenceSubjectBindings {
        EvidenceSubjectBindings::new(Some("message-1"), Some("object-1"), facts)
    }

    #[test]
    fn record_ref_accepts_type_id_and_rejects_nested_or_empty() {
        let parsed = EvidenceRecordRef::parse(" inbox_message:message-1 ").unwrap();
        assert_eq!(parsed.kind(), "inbox_message");
        assert_eq!(parsed.id(), "message-1");
        assert_eq!(parsed.to_string(), "inbox_message:message-1");
        assert!(EvidenceRecordRef::parse("message-1").is_err());
        assert!(EvidenceRecordRef::parse("inbox_message:").is_err());
        assert!(EvidenceRecordRef::parse(":message-1").is_err());
        assert!(EvidenceRecordRef::parse("   ").is_err());
        assert!(EvidenceRecordRef::parse("inbox_message:message-1:forged").is_err());
    }

    #[test]
    fn record_ref_rejects_delimiter_injection() {
        for injected in [
            "inbox_message:message-1;forged",
            "inbox_message:message-1,forged",
            "inbox_message:message-1|forged",
            "inbox_message:message-1/forged",
            "inbox_message:message-1=forged",
            "inbox_message:message 1",
        ] {
            assert!(
                EvidenceRecordRef::parse(injected).is_err(),
                "{injected} 应拒绝分隔符注入"
            );
        }
    }

    #[test]
    fn stored_reference_parses_historical_scheme_and_canonical_forms() {
        let scheme = CanonicalEvidenceReference::parse_stored(" mall_order_fact://f-1001 ").unwrap();
        assert_eq!(scheme.kind(), "mall_order_fact");
        assert_eq!(scheme.id(), "f-1001");
        assert_eq!(scheme.version(), None);
        assert_eq!(scheme.status(), None);
        let hyphenated = CanonicalEvidenceReference::parse_stored("mall-snapshot:7").unwrap();
        assert_eq!(hyphenated.kind(), "mall-snapshot");
        assert_eq!(hyphenated.id(), "7");

        let verified =
            CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v2:processed").unwrap();
        assert_eq!(verified.id(), "message-1");
        assert_eq!(verified.version(), Some(2));
        assert_eq!(verified.status(), Some("processed"));
        assert_eq!(verified.to_string(), "inbox_message:message-1:v2:processed");

        let reviewed =
            CanonicalEvidenceReference::parse_stored("reconciliation_difference_resolution:res-1:reviewed")
                .unwrap();
        assert_eq!(reviewed.id(), "res-1");
        assert_eq!(reviewed.status(), Some("reviewed"));
        assert_eq!(
            reviewed.to_string(),
            "reconciliation_difference_resolution:res-1:reviewed"
        );
    }

    #[test]
    fn stored_reference_rejects_nested_identity_and_leading_zero_version() {
        assert!(
            CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v1:processed:extra").is_err()
        );
        assert!(CanonicalEvidenceReference::parse_stored("mall_order_fact://f-1001:v1:attributed").is_err());
        assert!(CanonicalEvidenceReference::parse_stored("inbox_message:message-1:v01:processed").is_err());
        assert!(CanonicalEvidenceReference::parse_stored("").is_err());
    }

    #[test]
    fn referenced_id_returns_identity_not_version_suffix() {
        let bindings = EvidenceSubjectBindings::new(
            None,
            None,
            ["mall_order_fact:abc:v1:attributed", "invoice://inv-88"],
        );
        assert_eq!(bindings.referenced_id("mall_order_fact"), Some("abc"));
        assert_eq!(bindings.referenced_id("invoice"), Some("inv-88"));
        assert_eq!(bindings.referenced_id("missing"), None);
    }

    #[test]
    fn association_uses_exact_identity_and_ignores_false_substring_hits() {
        let subject = bindings(&["mall_order_fact:order-123:v1:attributed"]);
        assert!(subject.associates_any(["order-123"]));
        assert!(subject.associates_any(["message-1"]));
        assert!(subject.associates_any(["object-1"]));
        assert!(!subject.associates_any(["order-12"]));
        assert!(!subject.associates_any(["v1"]));
        assert!(!subject.associates_any(["attributed"]));
        assert!(!subject.associates_any(["mall_order_fact"]));
        assert!(!subject.associates_any(["processed"]));
        assert!(!subject.associates_any([""]));
        assert!(!subject.associates_any(["  "]));
        assert!(!subject.associates_any(["mall_order_fact:order-123:v1:attributed"]));
    }

    #[test]
    fn association_rejects_delimiter_injected_and_unparsed_token_splits() {
        let subject = bindings(&["mall_order_fact:abc;evil", "not-a-reference"]);
        assert!(!subject.associates_any(["abc"]));
        assert!(!subject.associates_any(["evil"]));
        assert!(subject.associates_any(["not-a-reference"]));
        assert!(!subject.associates_any(["not"]));
        assert!(!subject.associates_any(["a"]));
        assert!(!subject.associates_any(["reference"]));
    }

    #[test]
    fn terminal_set_sorts_dedups_and_enforces_512_boundary() {
        let first = CanonicalEvidenceReference::verified("inbox_message", "b", Some(1), "processed").unwrap();
        let second =
            CanonicalEvidenceReference::verified("inbox_message", "a", Some(1), "processed").unwrap();
        let duplicate = second.clone();
        let encoded = EvidenceReferenceSet::try_from_canonical([first, second, duplicate])
            .unwrap()
            .into_wire();
        assert_eq!(
            encoded,
            "inbox_message:a:v1:processed;inbox_message:b:v1:processed"
        );
        assert!(EvidenceReferenceSet::try_from_canonical(Vec::new()).is_err());

        let left_ok = "a".repeat(251);
        let right = "b".repeat(256);
        let at_limit =
            EvidenceReferenceSet::try_from_canonical([identity_ref(&left_ok), identity_ref(&right)]).unwrap();
        assert_eq!(at_limit.as_str().len(), ENCODED_MAX_LEN);

        let left_over = "a".repeat(252);
        assert!(
            EvidenceReferenceSet::try_from_canonical([identity_ref(&left_over), identity_ref(&right),])
                .is_err()
        );
    }

    #[test]
    fn compact_set_sorts_dedups_empty_and_512_boundary() {
        assert!(CompactEvidenceSet::try_from_pairs(Vec::<(&str, &str)>::new())
            .unwrap()
            .is_none());
        let encoded = CompactEvidenceSet::try_from_pairs([
            ("EXTERNAL_CASE_RESULT", "inbox_message:b"),
            ("BUSINESS_OBJECT_VERIFICATION", "mall_order_fact:a"),
            ("EXTERNAL_CASE_RESULT", "inbox_message:b"),
        ])
        .unwrap()
        .unwrap()
        .into_wire();
        assert_eq!(
            encoded,
            "BUSINESS_OBJECT_VERIFICATION:mall_order_fact:a,EXTERNAL_CASE_RESULT:inbox_message:b"
        );
        assert!(CompactEvidenceSet::try_from_pairs([("EXTERNAL_CASE_RESULT", "inbox_message:a:b")]).is_err());

        let left_ok = format!("t:{}", "a".repeat(247));
        let right = format!("t:{}", "b".repeat(256));
        let at_limit = CompactEvidenceSet::try_from_pairs([("K", left_ok.as_str()), ("K", right.as_str())])
            .unwrap()
            .unwrap();
        assert_eq!(at_limit.as_str().len(), ENCODED_MAX_LEN);

        let left_over = format!("t:{}", "a".repeat(248));
        assert!(
            CompactEvidenceSet::try_from_pairs([("K", left_over.as_str()), ("K", right.as_str())]).is_err()
        );
    }

    fn identity_ref(id: &str) -> CanonicalEvidenceReference {
        CanonicalEvidenceReference::parse_stored(&format!("k:{id}")).unwrap()
    }

    #[test]
    fn replay_original_preserves_pipe_bearing_fact_key_without_set_encoding() {
        const KEY: &str = "mall-1|PAYMENT_SUCCEEDED|SO-2026-001|v3";
        assert!(
            CanonicalEvidenceReference::parse_stored(&format!("business_fact_key:{KEY}")).is_err(),
            "含 | 的事实键不得当作证据 ID"
        );
        assert!(
            CanonicalEvidenceReference::verified("inbox_message", KEY, Some(1), "requeued").is_err(),
            "含 | 的事实键不得走 parse_id"
        );

        let replay = ReplayOriginalReference::new("msg-1", 2, &format!(" {KEY} ")).unwrap();
        assert_eq!(replay.inbox().to_string(), "inbox_message:msg-1:v2:requeued");
        assert_eq!(replay.business_fact_key(), KEY);
        let wire = replay.into_wire();
        assert_eq!(
            wire,
            format!("inbox_message:msg-1:v2:requeued;business_fact_key:{KEY}")
        );
        let parsed = ReplayOriginalReference::parse(&wire).unwrap();
        assert_eq!(parsed.business_fact_key(), KEY);
        assert_eq!(parsed.inbox().id(), "msg-1");
        assert_eq!(parsed.inbox().version(), Some(2));
        assert!(ReplayOriginalReference::parse("inbox_message:msg-1:v2:requeued").is_err());
        assert!(ReplayOriginalReference::new("msg-1", 2, "  ").is_err());
    }
}
