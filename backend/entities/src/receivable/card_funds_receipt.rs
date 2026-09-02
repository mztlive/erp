//! W13 卡券票款幂等回执值对象（FIN-E14）。
//!
//! `CardFundsCommandReceipt` 与 `CardFundsRegistrationReceipt` 唯一负责确定性
//! encode/decode、payload fingerprint、replay 相等性与 legacy version 兼容。
//! 当前写入固定为 9 字段 V2；7 字段 V1 仅只读解析。审计 I/O 与 legacy WorkItem
//! 补建仍由 Service 编排。本模块无 MongoDB、HTTP、全局时钟或原始密钥。

use serde::Serialize;
use sha2::{Digest, Sha256};

use super::card_funds_review_decision::{CardFundsReviewConclusion, CardFundsReviewResult};

/// 审计消息前缀；历史持久化形态，禁止变更。
const COMMAND_FINGERPRINT_PREFIX: &str = "command_sha256=";
/// 正式复核命令审计动作。
pub const CARD_FUNDS_REVIEW_ACTION: &str = "receivable_funds_review.complete";
/// 正式复核收据 ID 前缀。
const CARD_FUNDS_REVIEW_RECEIPT_PREFIX: &str = "card-funds-review-command-";
/// 历史回款登记审计动作。
pub const CARD_FUNDS_RECEIPT_REGISTRATION_ACTION: &str = "card_funds.receipt.register";
/// 历史发票登记审计动作。
pub const CARD_FUNDS_INVOICE_REGISTRATION_ACTION: &str = "card_funds.invoice.register";
/// 历史登记收据 ID 前缀。
const CARD_FUNDS_REGISTRATION_RECEIPT_PREFIX: &str = "card-funds-registration-";

/// 正式复核收据的显式 wire 版本。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsCommandReceiptVersion {
    /// 7 字段旧版：无后继任务槽。
    LegacyV1,
    /// 9 字段现行版：固定两个后继任务槽。
    V2,
}

/// 正式复核后继任务的类型化完整性。
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum CardFundsCommandFollowUp {
    /// 通过，或 7 字段旧版通过：无后继任务。
    None,
    /// 9 字段驳回：后继任务 ID 与类型均非空。
    Rejected {
        /// 后继开放任务 ID。
        work_item_id: String,
        /// 后继任务类型稳定代码。
        work_item_type: String,
    },
    /// 7 字段旧版驳回：缺后继，仅供 Service 补建。
    LegacyRejected,
}

/// 正式复核收据的业务载荷。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsCommandReceiptData {
    /// 新增复核事实 ID。
    pub receivable_funds_review_id: String,
    /// 同事务工作流动作 ID。
    pub workflow_action_id: String,
    /// 正式复核号。
    pub review_no: u32,
    /// 事务完成后的账户复核状态代码。
    pub account_review_status: String,
    /// 完成时间 Unix 秒。
    pub completed_at: i64,
    /// 正式复核结果。
    pub review_result: CardFundsReviewResult,
    /// 正式复核结论。
    pub conclusion: CardFundsReviewConclusion,
    /// 类型化后继任务。
    pub follow_up: CardFundsCommandFollowUp,
}

/// W13 正式复核幂等回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsCommandReceipt {
    fingerprint: String,
    version: CardFundsCommandReceiptVersion,
    data: CardFundsCommandReceiptData,
}

/// 正式复核回执编解码失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CardFundsCommandReceiptError {
    /// 同一幂等键已被不同命令载荷占用。
    #[error("幂等键已用于不同的卡券票款复核命令")]
    PayloadConflict,
    /// 缺少 `command_sha256=` 信封或 `;result=` 分隔。
    #[error("卡券票款复核幂等收据格式非法")]
    Malformed,
    /// 字段数量不是 7 或 9。
    #[error("卡券票款复核幂等收据结果非法")]
    ResultIllegal,
    /// 结果代码不是 `APPROVED` / `REJECTED`。
    #[error("卡券票款复核收据结果代码非法")]
    ResultCodeIllegal,
    /// 结论代码不是三种受控代码之一。
    #[error("卡券票款复核收据结论代码非法")]
    ConclusionCodeIllegal,
    /// 后继任务两槽与结果组合不完整。
    #[error("卡券票款复核后继任务收据不完整")]
    FollowUpIncomplete,
    /// 复核号不是十进制 `u32`。
    #[error("卡券票款复核收据复核号非法")]
    ReviewNoIllegal,
    /// 完成时间不是十进制 `i64`。
    #[error("卡券票款复核收据完成时间非法")]
    CompletedAtIllegal,
    /// 命令 JSON 无法序列化。
    #[error("卡券票款复核命令序列化失败: {0}")]
    SerializeFailed(String),
}

impl CardFundsCommandReceipt {
    /// 构造可写入审计的现行 9 字段回执。
    ///
    /// # 参数
    /// * `fingerprint` - 已排除明文幂等键的命令载荷指纹
    /// * `data` - 正式结果与类型化后继
    ///
    /// # 返回
    /// 后继完整性与结果一致时返回 V2 回执。
    ///
    /// # 错误
    /// 通过必须无后继、驳回必须带完整后继；`LegacyRejected` 禁止用于新写入。
    ///
    /// # 约束
    /// 不读写审计，不生成 ID 或时钟。
    pub fn new(
        fingerprint: impl Into<String>,
        data: CardFundsCommandReceiptData,
    ) -> Result<Self, CardFundsCommandReceiptError> {
        ensure_writable_follow_up(data.review_result, &data.follow_up)?;
        Ok(Self {
            fingerprint: fingerprint.into(),
            version: CardFundsCommandReceiptVersion::V2,
            data,
        })
    }

    /// 计算覆盖完整命令 JSON 的稳定指纹。
    ///
    /// # 参数
    /// * `command` - 与存量指纹相同形态的可序列化命令
    ///
    /// # 返回
    /// 64 位小写十六进制 SHA-256。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 [`CardFundsCommandReceiptError::SerializeFailed`]。
    ///
    /// # 约束
    /// 必须使用 `serde_json::to_vec`；算法变更会破坏存量回放。
    pub fn payload_fingerprint<T: Serialize>(command: &T) -> Result<String, CardFundsCommandReceiptError> {
        let serialized = serde_json::to_vec(command)
            .map_err(|error| CardFundsCommandReceiptError::SerializeFailed(error.to_string()))?;
        Ok(hex::encode(Sha256::digest(serialized)))
    }

    /// 生成不泄漏原始幂等键的稳定审计主键。
    ///
    /// # 参数
    /// * `actor_id` - 已认证操作人
    /// * `key` - 原始幂等键，内部 trim
    ///
    /// # 返回
    /// `{prefix}{length-prefixed-sha256}`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 动作固定为 [`CARD_FUNDS_REVIEW_ACTION`]；原始键不得进入 ID 明文。
    pub fn audit_id(actor_id: &str, key: &str) -> String {
        let mut digest = Sha256::new();
        digest_part(&mut digest, CARD_FUNDS_REVIEW_ACTION);
        digest_part(&mut digest, actor_id);
        digest_part(&mut digest, key.trim());
        format!(
            "{CARD_FUNDS_REVIEW_RECEIPT_PREFIX}{}",
            hex::encode(digest.finalize())
        )
    }

    /// 编码为受审计消息长度约束的 9 字段 V2 收据。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `command_sha256={fingerprint};result=9 字段`。
    ///
    /// # 错误
    /// `LegacyRejected` 禁止重新编码，避免把缺后继写回审计。
    ///
    /// # 约束
    /// 新写入一律 9 字段；不得输出 7 字段。
    pub fn encode_message(&self) -> Result<String, CardFundsCommandReceiptError> {
        let (follow_up_id, follow_up_type) = match &self.data.follow_up {
            CardFundsCommandFollowUp::None => ("", ""),
            CardFundsCommandFollowUp::Rejected {
                work_item_id,
                work_item_type,
            } => (work_item_id.as_str(), work_item_type.as_str()),
            CardFundsCommandFollowUp::LegacyRejected => {
                return Err(CardFundsCommandReceiptError::FollowUpIncomplete);
            }
        };
        Ok(format!(
            "{COMMAND_FINGERPRINT_PREFIX}{};result={}|{}|{}|{}|{}|{}|{}|{}|{}",
            self.fingerprint,
            self.data.receivable_funds_review_id,
            self.data.workflow_action_id,
            self.data.review_no,
            self.data.account_review_status,
            self.data.completed_at,
            self.data.review_result.as_str(),
            self.data.conclusion.as_str(),
            follow_up_id,
            follow_up_type,
        ))
    }

    /// 解析 7 字段旧版或 9 字段现行收据，并拒绝同键载荷漂移。
    ///
    /// # 参数
    /// * `message` - 审计消息
    /// * `expected_fingerprint` - 当前命令指纹
    ///
    /// # 返回
    /// 指纹一致且字段合法时返回回执。
    ///
    /// # 错误
    /// 信封损坏为 [`Malformed`](CardFundsCommandReceiptError::Malformed)；
    /// 指纹不一致为 [`PayloadConflict`](CardFundsCommandReceiptError::PayloadConflict)。
    ///
    /// # 约束
    /// 先拆信封再比指纹，最后解码结果；不得在冲突时回放旧结果。
    pub fn parse(message: &str, expected_fingerprint: &str) -> Result<Self, CardFundsCommandReceiptError> {
        let (fingerprint, result) = message
            .strip_prefix(COMMAND_FINGERPRINT_PREFIX)
            .and_then(|value| value.split_once(";result="))
            .ok_or(CardFundsCommandReceiptError::Malformed)?;
        if fingerprint != expected_fingerprint {
            return Err(CardFundsCommandReceiptError::PayloadConflict);
        }
        let data = parse_command_result(result)?;
        let version = match &data.follow_up {
            CardFundsCommandFollowUp::LegacyRejected => CardFundsCommandReceiptVersion::LegacyV1,
            CardFundsCommandFollowUp::None | CardFundsCommandFollowUp::Rejected { .. } => {
                command_version_from_field_count(result)
            }
        };
        Ok(Self {
            fingerprint: fingerprint.to_string(),
            version,
            data,
        })
    }

    /// 判断七字段旧版驳回是否仍缺正式后继任务。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅 `LegacyRejected` 为 `true`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 9 字段空后继驳回不是 legacy，解析阶段已拒绝。
    pub fn requires_legacy_rejected_follow_up(&self) -> bool {
        matches!(self.data.follow_up, CardFundsCommandFollowUp::LegacyRejected)
    }

    /// 绑定已迁移形成的正式后继任务。
    ///
    /// # 参数
    /// * `work_item_id` - 后继任务 ID
    /// * `work_item_type` - 后继任务类型代码
    ///
    /// # 返回
    /// 内存中升级为完整驳回后继；不改写历史审计。
    ///
    /// # 错误
    /// 非 legacy 驳回或后继槽为空时返回
    /// [`FollowUpIncomplete`](CardFundsCommandReceiptError::FollowUpIncomplete)。
    ///
    /// # 约束
    /// Service 负责补建 WorkItem；本方法只收口类型完整性。
    pub fn attach_follow_up(
        &mut self,
        work_item_id: &str,
        work_item_type: &str,
    ) -> Result<(), CardFundsCommandReceiptError> {
        if !self.requires_legacy_rejected_follow_up() || work_item_id.is_empty() || work_item_type.is_empty()
        {
            return Err(CardFundsCommandReceiptError::FollowUpIncomplete);
        }
        self.data.follow_up = CardFundsCommandFollowUp::Rejected {
            work_item_id: work_item_id.to_string(),
            work_item_type: work_item_type.to_string(),
        };
        self.version = CardFundsCommandReceiptVersion::V2;
        Ok(())
    }

    /// 返回命令载荷指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 64 位小写十六进制摘要。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 指纹不得包含原始幂等键明文。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 返回显式收据版本。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 7 字段为 `LegacyV1`，现行 9 字段为 `V2`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 版本由字段数量与后继形态决定，不另存 wire 标记。
    pub fn version(&self) -> CardFundsCommandReceiptVersion {
        self.version
    }

    /// 返回业务载荷。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 正式结果与类型化后继。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不复制审计身份。
    pub fn data(&self) -> &CardFundsCommandReceiptData {
        &self.data
    }

    /// 返回完整后继任务身份。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 仅 9 字段完整驳回返回 `(id, type)`；legacy 与通过返回 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// HTTP View 映射仍由 Service 完成。
    pub fn follow_up_work_item(&self) -> Option<(&str, &str)> {
        match &self.data.follow_up {
            CardFundsCommandFollowUp::Rejected {
                work_item_id,
                work_item_type,
            } => Some((work_item_id.as_str(), work_item_type.as_str())),
            CardFundsCommandFollowUp::None | CardFundsCommandFollowUp::LegacyRejected => None,
        }
    }
}

/// 历史票款登记种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CardFundsRegistrationKind {
    /// 历史回款登记。
    Receipt,
    /// 历史销项发票登记。
    Invoice,
}

impl CardFundsRegistrationKind {
    /// 返回 wire 事实种类。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `receipt` 或 `invoice`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 与存量 `fact=` 字段一致。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Receipt => "receipt",
            Self::Invoice => "invoice",
        }
    }

    /// 按审计动作选择期望种类。
    ///
    /// # 参数
    /// * `action` - 登记审计动作
    ///
    /// # 返回
    /// 回款动作返回 `Receipt`，其余返回 `Invoice`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 与原 helper 对非回款动作一律视为发票的合同一致。
    pub fn from_expected_action(action: &str) -> Self {
        if action == CARD_FUNDS_RECEIPT_REGISTRATION_ACTION {
            Self::Receipt
        } else {
            Self::Invoice
        }
    }
}

/// W13 历史登记幂等回执。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CardFundsRegistrationReceipt {
    fingerprint: String,
    kind: CardFundsRegistrationKind,
    fact_id: String,
}

/// 历史登记回执编解码失败。
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum CardFundsRegistrationReceiptError {
    /// 同一幂等键已被不同登记载荷占用，或信封前缀不匹配。
    #[error("幂等键已用于不同的卡券票款登记命令")]
    PayloadConflict,
    /// `fact=` 后缺少 `|` 分隔。
    #[error("卡券票款登记幂等收据格式非法")]
    Malformed,
    /// 种类与动作不一致，或事实 ID 空白。
    #[error("卡券票款登记幂等收据事实非法")]
    FactIllegal,
    /// 登记命令 JSON 无法序列化。
    #[error("卡券票款登记命令序列化失败: {0}")]
    SerializeFailed(String),
}

impl CardFundsRegistrationReceipt {
    /// 构造可写入审计的登记回执。
    ///
    /// # 参数
    /// * `fingerprint` - 登记命令指纹
    /// * `kind` - 回款或发票
    /// * `fact_id` - 新建正式事实 ID
    ///
    /// # 返回
    /// 事实 ID 非空白时返回回执。
    ///
    /// # 错误
    /// 空白事实 ID 返回 [`FactIllegal`](CardFundsRegistrationReceiptError::FactIllegal)。
    ///
    /// # 约束
    /// 不读写审计。
    pub fn new(
        fingerprint: impl Into<String>,
        kind: CardFundsRegistrationKind,
        fact_id: impl Into<String>,
    ) -> Result<Self, CardFundsRegistrationReceiptError> {
        let fact_id = fact_id.into();
        if fact_id.trim().is_empty() {
            return Err(CardFundsRegistrationReceiptError::FactIllegal);
        }
        Ok(Self {
            fingerprint: fingerprint.into(),
            kind,
            fact_id,
        })
    }

    /// 计算 W13 登记命令指纹。
    ///
    /// # 参数
    /// * `command` - 与存量指纹相同形态的可序列化请求
    ///
    /// # 返回
    /// 64 位小写十六进制 SHA-256。
    ///
    /// # 错误
    /// JSON 序列化失败时返回 [`SerializeFailed`](CardFundsRegistrationReceiptError::SerializeFailed)。
    ///
    /// # 约束
    /// 必须使用 `serde_json::to_vec`。
    pub fn payload_fingerprint<T: Serialize>(
        command: &T,
    ) -> Result<String, CardFundsRegistrationReceiptError> {
        let serialized = serde_json::to_vec(command)
            .map_err(|error| CardFundsRegistrationReceiptError::SerializeFailed(error.to_string()))?;
        Ok(hex::encode(Sha256::digest(serialized)))
    }

    /// 生成 W13 登记命令的稳定审计主键。
    ///
    /// # 参数
    /// * `action` - 回款或发票登记动作
    /// * `actor_id` - 已认证操作人
    /// * `key` - 原始幂等键，内部 trim
    ///
    /// # 返回
    /// `{prefix}{sha256("{action}|{actor}|{key}")}`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 使用整串拼接摘要，与正式复核的长度前缀摘要不同；禁止改写。
    pub fn audit_id(action: &str, actor_id: &str, key: &str) -> String {
        let digest = hex::encode(Sha256::digest(
            format!("{action}|{actor_id}|{}", key.trim()).as_bytes(),
        ));
        format!("{CARD_FUNDS_REGISTRATION_RECEIPT_PREFIX}{digest}")
    }

    /// 归一化可选票款单号；空白由服务端生成。
    ///
    /// # 参数
    /// * `value` - 请求中的可选单号
    ///
    /// # 返回
    /// trim 后非空则返回该值，否则 `None`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 不生成稳定单号；调用方在 `None` 时使用 [`Self::stable_no`]。
    pub fn normalized_no(value: Option<&str>) -> Option<String> {
        value
            .map(str::trim)
            .filter(|value| !value.is_empty())
            .map(ToOwned::to_owned)
    }

    /// 按操作者与幂等键生成稳定且不泄漏原键的票款单号。
    ///
    /// # 参数
    /// * `prefix` - `SK` 或 `FP` 等业务前缀
    /// * `actor_id` - 已认证操作人
    /// * `key` - 原始幂等键，内部 trim
    ///
    /// # 返回
    /// `{prefix}-{sha256("{actor}|{key}")[..12]}`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 摘要算法与存量单号一致；原始键不得进入单号明文。
    pub fn stable_no(prefix: &str, actor_id: &str, key: &str) -> String {
        let digest = hex::encode(Sha256::digest(format!("{actor_id}|{}", key.trim()).as_bytes()));
        format!("{prefix}-{}", &digest[..12])
    }

    /// 编码登记事实收据。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// `command_sha256={fingerprint};fact={kind}|{fact_id}`。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 形态必须与存量登记收据一致。
    pub fn encode_message(&self) -> String {
        format!(
            "{COMMAND_FINGERPRINT_PREFIX}{};fact={}|{}",
            self.fingerprint,
            self.kind.as_str(),
            self.fact_id
        )
    }

    /// 解析登记收据并拒绝同键载荷漂移。
    ///
    /// # 参数
    /// * `message` - 审计消息
    /// * `expected_fingerprint` - 当前登记命令指纹
    /// * `expected_kind` - 由审计动作决定的事实种类
    ///
    /// # 返回
    /// 前缀、种类与事实 ID 均合法时返回回执。
    ///
    /// # 错误
    /// 前缀不匹配为 [`PayloadConflict`](CardFundsRegistrationReceiptError::PayloadConflict)；
    /// 无 `|` 为 [`Malformed`](CardFundsRegistrationReceiptError::Malformed)。
    ///
    /// # 约束
    /// 缺少消息或指纹漂移一律冲突，不得回放旧事实。
    pub fn parse(
        message: &str,
        expected_fingerprint: &str,
        expected_kind: CardFundsRegistrationKind,
    ) -> Result<Self, CardFundsRegistrationReceiptError> {
        let expected_prefix = format!("{COMMAND_FINGERPRINT_PREFIX}{expected_fingerprint};fact=");
        let fact = message
            .strip_prefix(&expected_prefix)
            .ok_or(CardFundsRegistrationReceiptError::PayloadConflict)?;
        let (kind, fact_id) = fact
            .split_once('|')
            .ok_or(CardFundsRegistrationReceiptError::Malformed)?;
        if kind != expected_kind.as_str() || fact_id.trim().is_empty() {
            return Err(CardFundsRegistrationReceiptError::FactIllegal);
        }
        Ok(Self {
            fingerprint: expected_fingerprint.to_string(),
            kind: expected_kind,
            fact_id: fact_id.to_string(),
        })
    }

    /// 返回登记命令指纹。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 64 位小写十六进制摘要。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 指纹不得包含原始幂等键明文。
    pub fn fingerprint(&self) -> &str {
        &self.fingerprint
    }

    /// 返回事实种类。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 回款或发票。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 与解析时的期望种类相同。
    pub fn kind(&self) -> CardFundsRegistrationKind {
        self.kind
    }

    /// 返回正式事实 ID。
    ///
    /// # 参数
    /// 无。
    ///
    /// # 返回
    /// 回款单或发票 ID。
    ///
    /// # 错误
    /// 不返回错误。
    ///
    /// # 约束
    /// 保留解析时的原始文本，不额外 trim。
    pub fn fact_id(&self) -> &str {
        &self.fact_id
    }
}

/// 校验新写入后继与结果组合。
///
/// # 参数
/// * `review_result` - 正式结果
/// * `follow_up` - 类型化后继
///
/// # 返回
/// 组合合法时返回 `Ok(())`。
///
/// # 错误
/// 不完整或误用 `LegacyRejected` 时返回 `FollowUpIncomplete`。
///
/// # 约束
/// 仅用于 `new`；解析走 `parse_follow_up`。
fn ensure_writable_follow_up(
    review_result: CardFundsReviewResult,
    follow_up: &CardFundsCommandFollowUp,
) -> Result<(), CardFundsCommandReceiptError> {
    match (review_result, follow_up) {
        (CardFundsReviewResult::Approved, CardFundsCommandFollowUp::None) => Ok(()),
        (
            CardFundsReviewResult::Rejected,
            CardFundsCommandFollowUp::Rejected {
                work_item_id,
                work_item_type,
            },
        ) if !work_item_id.is_empty() && !work_item_type.is_empty() => Ok(()),
        _ => Err(CardFundsCommandReceiptError::FollowUpIncomplete),
    }
}

/// 按 `|` 字段数量判定显式版本。
///
/// # 参数
/// * `result` - `;result=` 之后的文本
///
/// # 返回
/// 7 字段为 `LegacyV1`，否则 `V2`。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 调用方已保证字段数为 7 或 9。
fn command_version_from_field_count(result: &str) -> CardFundsCommandReceiptVersion {
    if result.split('|').count() == 7 {
        CardFundsCommandReceiptVersion::LegacyV1
    } else {
        CardFundsCommandReceiptVersion::V2
    }
}

/// 解码正式复核结果字段。
///
/// # 参数
/// * `result` - `;result=` 之后的文本
///
/// # 返回
/// 7 或 9 字段均合法时返回业务载荷。
///
/// # 错误
/// 字段数、代码或后继完整性失败时返回对应错误。
///
/// # 约束
/// 不比较指纹。
fn parse_command_result(result: &str) -> Result<CardFundsCommandReceiptData, CardFundsCommandReceiptError> {
    let fields = result.split('|').collect::<Vec<_>>();
    let [review_id, workflow_id, review_no, account_status, completed_at, result_code, conclusion, follow_up @ ..] =
        fields.as_slice()
    else {
        return Err(CardFundsCommandReceiptError::ResultIllegal);
    };
    let review_result = parse_review_result(result_code)?;
    let conclusion = parse_conclusion(conclusion)?;
    let follow_up = parse_follow_up(review_result, follow_up)?;
    Ok(CardFundsCommandReceiptData {
        receivable_funds_review_id: (*review_id).to_string(),
        workflow_action_id: (*workflow_id).to_string(),
        review_no: review_no
            .parse()
            .map_err(|_| CardFundsCommandReceiptError::ReviewNoIllegal)?,
        account_review_status: (*account_status).to_string(),
        completed_at: completed_at
            .parse()
            .map_err(|_| CardFundsCommandReceiptError::CompletedAtIllegal)?,
        review_result,
        conclusion,
        follow_up,
    })
}

/// 解析结果代码。
///
/// # 参数
/// * `code` - wire 结果代码
///
/// # 返回
/// 受控结果枚举。
///
/// # 错误
/// 未知代码返回 `ResultCodeIllegal`。
///
/// # 约束
/// 大小写敏感，与历史收据一致。
fn parse_review_result(code: &str) -> Result<CardFundsReviewResult, CardFundsCommandReceiptError> {
    match code {
        "APPROVED" => Ok(CardFundsReviewResult::Approved),
        "REJECTED" => Ok(CardFundsReviewResult::Rejected),
        _ => Err(CardFundsCommandReceiptError::ResultCodeIllegal),
    }
}

/// 解析结论代码。
///
/// # 参数
/// * `code` - wire 结论代码
///
/// # 返回
/// 受控结论枚举。
///
/// # 错误
/// 未知代码返回 `ConclusionCodeIllegal`。
///
/// # 约束
/// 大小写敏感。
fn parse_conclusion(code: &str) -> Result<CardFundsReviewConclusion, CardFundsCommandReceiptError> {
    match code {
        "NO_HISTORY_FROM_ZERO" => Ok(CardFundsReviewConclusion::NoHistoryFromZero),
        "RECORDED_FACTS_RECONCILED" => Ok(CardFundsReviewConclusion::RecordedFactsReconciled),
        "REJECTED" => Ok(CardFundsReviewConclusion::Rejected),
        _ => Err(CardFundsCommandReceiptError::ConclusionCodeIllegal),
    }
}

/// 按字段数量与结果解析后继完整性。
///
/// # 参数
/// * `review_result` - 已解析结果
/// * `follow_up` - 结果字段之后的剩余槽
///
/// # 返回
/// 7 字段空槽或 9 字段完整组合。
///
/// # 错误
/// 槽数量不是 0/2，或 9 字段组合与结果不一致。
///
/// # 约束
/// 7 字段驳回标记为 `LegacyRejected`，不得当作 9 字段空槽。
fn parse_follow_up(
    review_result: CardFundsReviewResult,
    follow_up: &[&str],
) -> Result<CardFundsCommandFollowUp, CardFundsCommandReceiptError> {
    match follow_up {
        [] => match review_result {
            CardFundsReviewResult::Approved => Ok(CardFundsCommandFollowUp::None),
            CardFundsReviewResult::Rejected => Ok(CardFundsCommandFollowUp::LegacyRejected),
        },
        [follow_up_work_item_id, follow_up_work_item_type] => match (
            review_result,
            follow_up_work_item_id.is_empty(),
            follow_up_work_item_type.is_empty(),
        ) {
            (CardFundsReviewResult::Approved, true, true) => Ok(CardFundsCommandFollowUp::None),
            (CardFundsReviewResult::Rejected, false, false) => Ok(CardFundsCommandFollowUp::Rejected {
                work_item_id: (*follow_up_work_item_id).to_string(),
                work_item_type: (*follow_up_work_item_type).to_string(),
            }),
            _ => Err(CardFundsCommandReceiptError::FollowUpIncomplete),
        },
        _ => Err(CardFundsCommandReceiptError::ResultIllegal),
    }
}

/// 向摘要写入无拼接歧义的长度前缀字段。
///
/// # 参数
/// * `digest` - SHA-256 累加器
/// * `value` - 字段文本
///
/// # 返回
/// 无。
///
/// # 错误
/// 不返回错误。
///
/// # 约束
/// 与原 Service `digest_part` 及复核链/快照编码一致。
fn digest_part(digest: &mut Sha256, value: &str) {
    digest.update((value.len() as u64).to_be_bytes());
    digest.update(value.as_bytes());
}

#[cfg(test)]
mod tests {
    use serde::Serialize;

    use super::{
        CardFundsCommandFollowUp, CardFundsCommandReceipt, CardFundsCommandReceiptData,
        CardFundsCommandReceiptError, CardFundsCommandReceiptVersion, CardFundsRegistrationKind,
        CardFundsRegistrationReceipt, CardFundsRegistrationReceiptError,
        CARD_FUNDS_INVOICE_REGISTRATION_ACTION, CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
        CARD_FUNDS_REVIEW_ACTION,
    };
    use crate::receivable::card_funds_review_decision::{CardFundsReviewConclusion, CardFundsReviewResult};

    #[derive(Serialize)]
    struct SampleCommand {
        account_id: &'static str,
        review_no: u32,
    }

    fn fingerprint() -> String {
        "a".repeat(64)
    }

    fn rejected_data() -> CardFundsCommandReceiptData {
        CardFundsCommandReceiptData {
            receivable_funds_review_id: "review-1".to_string(),
            workflow_action_id: "workflow-1".to_string(),
            review_no: 1,
            account_review_status: "opening_pending".to_string(),
            completed_at: 1_700_000_000,
            review_result: CardFundsReviewResult::Rejected,
            conclusion: CardFundsReviewConclusion::Rejected,
            follow_up: CardFundsCommandFollowUp::Rejected {
                work_item_id: "wi-2".to_string(),
                work_item_type: "CARD_FUNDS_REVIEW".to_string(),
            },
        }
    }

    fn approved_data() -> CardFundsCommandReceiptData {
        CardFundsCommandReceiptData {
            receivable_funds_review_id: "review-1".to_string(),
            workflow_action_id: "workflow-1".to_string(),
            review_no: 1,
            account_review_status: "reviewed".to_string(),
            completed_at: 1_700_000_000,
            review_result: CardFundsReviewResult::Approved,
            conclusion: CardFundsReviewConclusion::NoHistoryFromZero,
            follow_up: CardFundsCommandFollowUp::None,
        }
    }

    #[test]
    fn same_payload_fingerprint_is_deterministic() {
        let command = SampleCommand {
            account_id: "ra-1",
            review_no: 1,
        };
        let first = CardFundsCommandReceipt::payload_fingerprint(&command).unwrap();
        let second = CardFundsCommandReceipt::payload_fingerprint(&command).unwrap();
        assert_eq!(first, second);
        assert_eq!(first.len(), 64);
        assert_ne!(
            first,
            CardFundsCommandReceipt::payload_fingerprint(&SampleCommand {
                account_id: "ra-1",
                review_no: 2,
            })
            .unwrap()
        );
    }

    #[test]
    fn current_nine_field_receipt_round_trips() {
        let receipt = CardFundsCommandReceipt::new(fingerprint(), rejected_data()).unwrap();
        let message = receipt.encode_message().unwrap();
        let parsed = CardFundsCommandReceipt::parse(&message, &fingerprint()).unwrap();
        assert_eq!(parsed, receipt);
        assert_eq!(parsed.version(), CardFundsCommandReceiptVersion::V2);
        assert_eq!(parsed.follow_up_work_item(), Some(("wi-2", "CARD_FUNDS_REVIEW")));
    }

    #[test]
    fn different_payload_same_key_conflicts() {
        let receipt = CardFundsCommandReceipt::new(fingerprint(), rejected_data()).unwrap();
        let message = receipt.encode_message().unwrap();
        assert_eq!(
            CardFundsCommandReceipt::parse(&message, &"b".repeat(64)).unwrap_err(),
            CardFundsCommandReceiptError::PayloadConflict
        );
    }

    #[test]
    fn malformed_envelopes_are_rejected() {
        assert_eq!(
            CardFundsCommandReceipt::parse("garbage", &fingerprint()).unwrap_err(),
            CardFundsCommandReceiptError::Malformed
        );
        assert_eq!(
            CardFundsCommandReceipt::parse("command_sha256=abc;fact=x|y", &fingerprint()).unwrap_err(),
            CardFundsCommandReceiptError::Malformed
        );
        let six = format!(
            "command_sha256={};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED",
            fingerprint()
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&six, &fingerprint()).unwrap_err(),
            CardFundsCommandReceiptError::ResultIllegal
        );
        let eight = format!(
            "command_sha256={};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO|only-one",
            fingerprint()
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&eight, &fingerprint()).unwrap_err(),
            CardFundsCommandReceiptError::ResultIllegal
        );
    }

    #[test]
    fn legacy_seven_field_receipts_decode_with_explicit_version() {
        let fingerprint = "c".repeat(64);
        let approved_message = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO"
        );
        let approved = CardFundsCommandReceipt::parse(&approved_message, &fingerprint).unwrap();
        assert_eq!(approved.version(), CardFundsCommandReceiptVersion::LegacyV1);
        assert!(!approved.requires_legacy_rejected_follow_up());
        assert!(approved.follow_up_work_item().is_none());

        let rejected_message = format!(
            "command_sha256={fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED"
        );
        let rejected = CardFundsCommandReceipt::parse(&rejected_message, &fingerprint).unwrap();
        assert_eq!(rejected.version(), CardFundsCommandReceiptVersion::LegacyV1);
        assert!(rejected.requires_legacy_rejected_follow_up());
        assert_eq!(rejected.data().review_result, CardFundsReviewResult::Rejected);
    }

    #[test]
    fn nine_field_follow_up_completeness_is_typed() {
        let fingerprint = fingerprint();
        let approved_empty = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO||"
        );
        let approved = CardFundsCommandReceipt::parse(&approved_empty, &fingerprint).unwrap();
        assert_eq!(approved.version(), CardFundsCommandReceiptVersion::V2);
        assert_eq!(approved.data().follow_up, CardFundsCommandFollowUp::None);

        let rejected_empty = format!(
            "command_sha256={fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED||"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&rejected_empty, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );

        let approved_with_follow_up = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO|wi-2|CARD_FUNDS_REVIEW"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&approved_with_follow_up, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );
    }

    #[test]
    fn new_rejects_inconsistent_follow_up() {
        let mut data = approved_data();
        data.follow_up = CardFundsCommandFollowUp::LegacyRejected;
        assert_eq!(
            CardFundsCommandReceipt::new(fingerprint(), data).unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );
        let mut data = rejected_data();
        data.follow_up = CardFundsCommandFollowUp::None;
        assert_eq!(
            CardFundsCommandReceipt::new(fingerprint(), data).unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );
    }

    #[test]
    fn attach_follow_up_only_upgrades_legacy_rejected() {
        let fingerprint = "c".repeat(64);
        let rejected_message = format!(
            "command_sha256={fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED"
        );
        let mut rejected = CardFundsCommandReceipt::parse(&rejected_message, &fingerprint).unwrap();
        rejected.attach_follow_up("wi-2", "CARD_FUNDS_REVIEW").unwrap();
        assert_eq!(
            rejected.follow_up_work_item(),
            Some(("wi-2", "CARD_FUNDS_REVIEW"))
        );
        assert!(!rejected.requires_legacy_rejected_follow_up());
        assert_eq!(rejected.version(), CardFundsCommandReceiptVersion::V2);

        let mut approved = CardFundsCommandReceipt::new(fingerprint.clone(), approved_data()).unwrap();
        assert_eq!(
            approved
                .attach_follow_up("wi-2", "CARD_FUNDS_REVIEW")
                .unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );
    }

    #[test]
    fn illegal_codes_and_numbers_are_rejected() {
        let fingerprint = fingerprint();
        let bad_result = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|PASSED|NO_HISTORY_FROM_ZERO"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&bad_result, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::ResultCodeIllegal
        );
        let bad_conclusion = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|1700000000|APPROVED|OTHER"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&bad_conclusion, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::ConclusionCodeIllegal
        );
        let bad_no = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|x|reviewed|1700000000|APPROVED|NO_HISTORY_FROM_ZERO"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&bad_no, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::ReviewNoIllegal
        );
        let bad_at = format!(
            "command_sha256={fingerprint};result=review-1|workflow-1|1|reviewed|now|APPROVED|NO_HISTORY_FROM_ZERO"
        );
        assert_eq!(
            CardFundsCommandReceipt::parse(&bad_at, &fingerprint).unwrap_err(),
            CardFundsCommandReceiptError::CompletedAtIllegal
        );
    }

    #[test]
    fn command_audit_id_is_stable_without_exposing_raw_key() {
        let id = CardFundsCommandReceipt::audit_id("actor-1", "secret-idempotency-key");
        assert_eq!(
            id,
            CardFundsCommandReceipt::audit_id("actor-1", "  secret-idempotency-key  ")
        );
        assert_eq!(
            id,
            "card-funds-review-command-968740315241e8548f3b5013a341a334b4a47ab6fcfcbfdcfeda3d4f04a9c76d"
        );
        assert_ne!(
            id,
            CardFundsCommandReceipt::audit_id("actor-2", "secret-idempotency-key")
        );
        assert!(!id.contains("secret-idempotency-key"));
        assert!(id.starts_with("card-funds-review-command-"));
        assert_eq!(CARD_FUNDS_REVIEW_ACTION, "receivable_funds_review.complete");
    }

    #[test]
    fn registration_receipt_round_trips_and_conflicts_on_payload_drift() {
        let receipt =
            CardFundsRegistrationReceipt::new(fingerprint(), CardFundsRegistrationKind::Receipt, "cr-1")
                .unwrap();
        let message = receipt.encode_message();
        let parsed =
            CardFundsRegistrationReceipt::parse(&message, &fingerprint(), CardFundsRegistrationKind::Receipt)
                .unwrap();
        assert_eq!(parsed, receipt);

        assert_eq!(
            CardFundsRegistrationReceipt::parse(
                &message,
                &"b".repeat(64),
                CardFundsRegistrationKind::Receipt,
            )
            .unwrap_err(),
            CardFundsRegistrationReceiptError::PayloadConflict
        );
        assert_eq!(
            CardFundsRegistrationReceipt::parse(
                "garbage",
                &fingerprint(),
                CardFundsRegistrationKind::Receipt,
            )
            .unwrap_err(),
            CardFundsRegistrationReceiptError::PayloadConflict
        );
    }

    #[test]
    fn registration_malformed_and_kind_mismatch_are_rejected() {
        let message = format!("command_sha256={};fact=receipt", fingerprint());
        assert_eq!(
            CardFundsRegistrationReceipt::parse(
                &message,
                &fingerprint(),
                CardFundsRegistrationKind::Receipt,
            )
            .unwrap_err(),
            CardFundsRegistrationReceiptError::Malformed
        );
        let message = format!("command_sha256={};fact=invoice|inv-1", fingerprint());
        assert_eq!(
            CardFundsRegistrationReceipt::parse(
                &message,
                &fingerprint(),
                CardFundsRegistrationKind::Receipt,
            )
            .unwrap_err(),
            CardFundsRegistrationReceiptError::FactIllegal
        );
        let message = format!("command_sha256={};fact=receipt|   ", fingerprint());
        assert_eq!(
            CardFundsRegistrationReceipt::parse(
                &message,
                &fingerprint(),
                CardFundsRegistrationKind::Receipt,
            )
            .unwrap_err(),
            CardFundsRegistrationReceiptError::FactIllegal
        );
    }

    #[test]
    fn registration_identity_and_stable_no_are_deterministic() {
        let command = SampleCommand {
            account_id: "ra-1",
            review_no: 1,
        };
        let first = CardFundsRegistrationReceipt::payload_fingerprint(&command).unwrap();
        assert_eq!(
            first,
            CardFundsRegistrationReceipt::payload_fingerprint(&command).unwrap()
        );
        let id = CardFundsRegistrationReceipt::audit_id(
            CARD_FUNDS_RECEIPT_REGISTRATION_ACTION,
            "actor-1",
            "secret-idempotency-key",
        );
        assert_eq!(
            id,
            "card-funds-registration-fe01b4f8e810761339620df50da661114ec3f8bd5f403fdb50186c0880e6aee5"
        );
        assert_ne!(
            id,
            CardFundsRegistrationReceipt::audit_id(
                CARD_FUNDS_INVOICE_REGISTRATION_ACTION,
                "actor-1",
                "secret-idempotency-key",
            )
        );
        assert!(!id.contains("secret-idempotency-key"));
        assert_eq!(
            CardFundsRegistrationReceipt::stable_no("SK", "actor-1", "secret-idempotency-key"),
            "SK-40b2343d6dcd"
        );
        assert_eq!(
            CardFundsRegistrationReceipt::stable_no("FP", "actor-1", "  secret-idempotency-key"),
            "FP-40b2343d6dcd"
        );
        assert_eq!(
            CardFundsRegistrationReceipt::normalized_no(Some("  SK-1  ")).as_deref(),
            Some("SK-1")
        );
        assert!(CardFundsRegistrationReceipt::normalized_no(Some("   ")).is_none());
        assert!(CardFundsRegistrationReceipt::normalized_no(None).is_none());
        assert_eq!(
            CardFundsRegistrationKind::from_expected_action(CARD_FUNDS_RECEIPT_REGISTRATION_ACTION),
            CardFundsRegistrationKind::Receipt
        );
        assert_eq!(
            CardFundsRegistrationKind::from_expected_action(CARD_FUNDS_INVOICE_REGISTRATION_ACTION),
            CardFundsRegistrationKind::Invoice
        );
    }

    #[test]
    fn legacy_rejected_cannot_be_reencoded() {
        let fingerprint = "c".repeat(64);
        let rejected_message = format!(
            "command_sha256={fingerprint};result=review-2|workflow-2|2|opening_pending|1700000001|REJECTED|REJECTED"
        );
        let rejected = CardFundsCommandReceipt::parse(&rejected_message, &fingerprint).unwrap();
        assert_eq!(
            rejected.encode_message().unwrap_err(),
            CardFundsCommandReceiptError::FollowUpIncomplete
        );
    }
}
