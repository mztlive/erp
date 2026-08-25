//! 审批通知 outbox。BPM 只输出中性事件，本实体保存收件人与投递状态。

use entity_core::BaseModel;
use entity_macros::Entity;
use serde::{Deserialize, Serialize};

use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::ApprovalNotificationOutboxId;
use crate::validation::{normalize_optional_text, normalize_required_text};

const DEDUP_KEY_MAX_LEN: usize = 128;
const RECIPIENT_MAX_LEN: usize = 128;
const TEMPLATE_TEXT_MAX_LEN: usize = 128;
const WORKER_ID_MAX_LEN: usize = 128;
const ERROR_CLASS_MAX_LEN: usize = 64;
const REJECT_SUMMARY_MAX_LEN: usize = 256;

/// 首次投递加最多 5 次重试，合计最多 6 次尝试。
pub const MAX_DELIVERY_ATTEMPTS: u32 = 6;

/// 第 1 至 5 次失败后的退避秒数。
pub const RETRY_BACKOFF_SECS: [i64; 5] = [60, 300, 900, 3_600, 21_600];

/// 通知事件种类。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalNotificationEventKind {
    /// 审批已启动。
    Started,
    /// 进入节点。
    Entered,
    /// 节点通过。
    NodeApproved,
    /// 节点驳回。
    NodeRejected,
    /// 实例受阻。
    Blocked,
    /// 原审批人已恢复。
    Resumed,
    /// 已改派。
    Reassigned,
    /// 正常取消。
    Cancelled,
    /// 受阻取消。
    BlockedCancelled,
    /// 最终通过。
    Completed,
}

impl ApprovalNotificationEventKind {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回合同第 16.5 节事件代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Started => "STARTED",
            Self::Entered => "ENTERED",
            Self::NodeApproved => "NODE_APPROVED",
            Self::NodeRejected => "NODE_REJECTED",
            Self::Blocked => "BLOCKED",
            Self::Resumed => "RESUMED",
            Self::Reassigned => "REASSIGNED",
            Self::Cancelled => "CANCELLED",
            Self::BlockedCancelled => "BLOCKED_CANCELLED",
            Self::Completed => "COMPLETED",
        }
    }
}

/// 投递状态。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ApprovalNotificationDeliveryStatus {
    /// 待投递。
    Pending,
    /// 已取得租约。
    InFlight,
    /// 已投递。
    Delivered,
    /// 死信。
    DeadLetter,
}

impl ApprovalNotificationDeliveryStatus {
    /// 返回稳定代码。
    ///
    /// # 返回
    /// 返回投递状态代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Pending => "PENDING",
            Self::InFlight => "IN_FLIGHT",
            Self::Delivered => "DELIVERED",
            Self::DeadLetter => "DEAD_LETTER",
        }
    }
}

/// 模板参数。不得包含 token 或完整敏感单据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct ApprovalNotificationTemplateParams {
    /// 单据类型中文名。
    pub document_type_label: String,
    /// 单据业务编号。
    pub document_no: String,
    /// 当前节点名称。
    pub current_node_name: String,
    /// 当前审批人显示名。
    pub current_approver_display_name: String,
    /// 轮次号。
    pub round_no: u32,
    /// 驳回原因摘要。
    pub reject_reason_summary: Option<String>,
}

/// 审批通知 outbox 记录。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct ApprovalNotificationOutbox {
    #[serde(flatten)]
    pub base: BaseModel,
    /// 业务事件去重键。
    pub dedup_key: String,
    /// 事件种类。
    pub event_kind: ApprovalNotificationEventKind,
    /// 收件人账号。
    pub recipient_user_ids: Vec<String>,
    /// 有界模板参数。
    pub template_params: ApprovalNotificationTemplateParams,
    /// 投递状态。
    pub delivery_status: ApprovalNotificationDeliveryStatus,
    /// 已尝试次数。
    pub attempt_count: u32,
    /// 下次尝试时间。
    pub next_attempt_at: Instant,
    /// 租约持有者。
    pub lease_owner: Option<String>,
    /// 租约截止时间。
    pub lease_until: Option<Instant>,
    /// 最后错误分类。
    pub last_error_class: Option<String>,
    /// 进入死信时间。
    pub dead_lettered_at: Option<Instant>,
}

impl ApprovalNotificationOutbox {
    /// 追加一条待投递通知。
    ///
    /// # 参数
    /// * `id` - outbox 主键
    /// * `dedup_key` - 业务事件去重键
    /// * `event_kind` - 事件种类
    /// * `recipient_user_ids` - 收件人
    /// * `template_params` - 有界模板参数
    /// * `at` - 入队时间
    ///
    /// # 错误
    /// 去重键、收件人或模板字段非法时返回错误。
    pub fn enqueue(
        id: ApprovalNotificationOutboxId,
        dedup_key: impl Into<String>,
        event_kind: ApprovalNotificationEventKind,
        recipient_user_ids: Vec<String>,
        template_params: ApprovalNotificationTemplateParams,
        at: Instant,
    ) -> Result<Self> {
        Ok(Self {
            base: BaseModel::new(id.to_string()),
            dedup_key: normalize_required_text(
                dedup_key.into(),
                "去重键不能为空",
                DEDUP_KEY_MAX_LEN,
                "去重键过长",
            )?,
            event_kind,
            recipient_user_ids: normalize_recipients(recipient_user_ids)?,
            template_params: normalize_template_params(template_params)?,
            delivery_status: ApprovalNotificationDeliveryStatus::Pending,
            attempt_count: 0,
            next_attempt_at: at,
            lease_owner: None,
            lease_until: None,
            last_error_class: None,
            dead_lettered_at: None,
        })
    }

    /// 以原子租约条件取得投递权。
    ///
    /// # 参数
    /// * `worker_id` - 租约持有者
    /// * `now` - 当前时间
    /// * `lease_until` - 租约截止
    ///
    /// # 错误
    /// 已投递、已死信或租约未到期时返回错误。
    pub fn acquire_lease(
        &mut self,
        worker_id: impl Into<String>,
        now: Instant,
        lease_until: Instant,
    ) -> Result<()> {
        self.ensure_retryable(now)?;
        if lease_until.unix_secs() <= now.unix_secs() {
            return Err(Error::from("租约截止必须晚于当前时间"));
        }
        self.lease_owner = Some(normalize_required_text(
            worker_id.into(),
            "租约持有者不能为空",
            WORKER_ID_MAX_LEN,
            "租约持有者过长",
        )?);
        self.lease_until = Some(lease_until);
        self.delivery_status = ApprovalNotificationDeliveryStatus::InFlight;
        Ok(())
    }

    /// 标记投递成功。成功后不得再次取得该消息。
    ///
    /// # 错误
    /// 当前不是投递中时返回错误。
    pub fn mark_delivered(&mut self) -> Result<()> {
        if self.delivery_status != ApprovalNotificationDeliveryStatus::InFlight {
            return Err(Error::from("只有投递中的消息可以标记成功"));
        }
        self.delivery_status = ApprovalNotificationDeliveryStatus::Delivered;
        self.lease_owner = None;
        self.lease_until = None;
        Ok(())
    }

    /// 记录一次失败。未达上限则按固定退避安排下次尝试，否则进入死信。
    ///
    /// # 参数
    /// * `error_class` - 错误分类
    /// * `failed_at` - 失败时间
    ///
    /// # 错误
    /// 当前不是投递中或错误分类非法时返回错误。
    pub fn mark_failure(&mut self, error_class: impl Into<String>, failed_at: Instant) -> Result<()> {
        if self.delivery_status != ApprovalNotificationDeliveryStatus::InFlight {
            return Err(Error::from("只有投递中的消息可以记录失败"));
        }
        self.last_error_class = Some(normalize_required_text(
            error_class.into(),
            "错误分类不能为空",
            ERROR_CLASS_MAX_LEN,
            "错误分类过长",
        )?);
        self.attempt_count = self
            .attempt_count
            .checked_add(1)
            .ok_or_else(|| Error::from("投递尝试次数溢出"))?;
        self.lease_owner = None;
        self.lease_until = None;
        if self.attempt_count >= MAX_DELIVERY_ATTEMPTS {
            self.delivery_status = ApprovalNotificationDeliveryStatus::DeadLetter;
            self.dead_lettered_at = Some(failed_at);
            return Ok(());
        }
        self.delivery_status = ApprovalNotificationDeliveryStatus::Pending;
        self.next_attempt_at = next_attempt_at(self.attempt_count, failed_at)?;
        Ok(())
    }
}

/// 规范化收件人列表。
///
/// # 错误
/// 列表为空或任一收件人非法时返回错误。
fn normalize_recipients(recipient_user_ids: Vec<String>) -> Result<Vec<String>> {
    if recipient_user_ids.is_empty() {
        return Err(Error::from("收件人不能为空"));
    }
    let mut normalized = Vec::with_capacity(recipient_user_ids.len());
    for recipient in recipient_user_ids {
        let recipient =
            normalize_required_text(recipient, "收件人不能为空", RECIPIENT_MAX_LEN, "收件人过长")?;
        if !normalized.iter().any(|item| item == &recipient) {
            normalized.push(recipient);
        }
    }
    Ok(normalized)
}

/// 规范化模板参数，拒绝超长敏感字段形态。
///
/// # 错误
/// 必填展示字段为空或超长时返回错误。
fn normalize_template_params(
    params: ApprovalNotificationTemplateParams,
) -> Result<ApprovalNotificationTemplateParams> {
    Ok(ApprovalNotificationTemplateParams {
        document_type_label: normalize_required_text(
            params.document_type_label,
            "单据类型名称不能为空",
            TEMPLATE_TEXT_MAX_LEN,
            "单据类型名称过长",
        )?,
        document_no: normalize_required_text(
            params.document_no,
            "单据编号不能为空",
            TEMPLATE_TEXT_MAX_LEN,
            "单据编号过长",
        )?,
        current_node_name: normalize_required_text(
            params.current_node_name,
            "节点名称不能为空",
            TEMPLATE_TEXT_MAX_LEN,
            "节点名称过长",
        )?,
        current_approver_display_name: normalize_required_text(
            params.current_approver_display_name,
            "审批人显示名不能为空",
            TEMPLATE_TEXT_MAX_LEN,
            "审批人显示名过长",
        )?,
        round_no: params.round_no,
        reject_reason_summary: normalize_optional_text(
            params.reject_reason_summary,
            "驳回原因摘要",
            REJECT_SUMMARY_MAX_LEN,
        )?,
    })
}

/// 按失败次数计算下次尝试时间。
///
/// # 错误
/// 失败次数超出退避表时返回错误。
fn next_attempt_at(attempt_count: u32, failed_at: Instant) -> Result<Instant> {
    let index = usize::try_from(attempt_count.saturating_sub(1)).unwrap_or(usize::MAX);
    let Some(delay) = RETRY_BACKOFF_SECS.get(index).copied() else {
        return Err(Error::from("没有更多退避间隔"));
    };
    Ok(Instant::from_unix_secs(
        failed_at.unix_secs().saturating_add(delay),
    ))
}

/// 当前消息是否允许被领取。
impl ApprovalNotificationOutbox {
    fn ensure_retryable(&self, now: Instant) -> Result<()> {
        match self.delivery_status {
            ApprovalNotificationDeliveryStatus::Delivered => Err(Error::from("已投递消息不得再次取得")),
            ApprovalNotificationDeliveryStatus::DeadLetter => Err(Error::from("死信消息不得再次取得")),
            ApprovalNotificationDeliveryStatus::Pending => {
                if self.next_attempt_at.unix_secs() > now.unix_secs() {
                    return Err(Error::from("尚未到达下次尝试时间"));
                }
                Ok(())
            }
            ApprovalNotificationDeliveryStatus::InFlight => match self.lease_until {
                Some(until) if until.unix_secs() <= now.unix_secs() => Ok(()),
                _ => Err(Error::from("租约尚未到期")),
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        ApprovalNotificationDeliveryStatus, ApprovalNotificationEventKind, ApprovalNotificationOutbox,
        ApprovalNotificationTemplateParams, MAX_DELIVERY_ATTEMPTS, RETRY_BACKOFF_SECS,
    };
    use crate::common::time::Instant;
    use crate::ids::ApprovalNotificationOutboxId;

    fn params() -> ApprovalNotificationTemplateParams {
        ApprovalNotificationTemplateParams {
            document_type_label: "库存调整单".into(),
            document_no: "ADJ-1".into(),
            current_node_name: "仓储复核".into(),
            current_approver_display_name: "张三".into(),
            round_no: 1,
            reject_reason_summary: None,
        }
    }

    fn pending() -> ApprovalNotificationOutbox {
        ApprovalNotificationOutbox::enqueue(
            ApprovalNotificationOutboxId::new("ob-1"),
            "started:inst-1",
            ApprovalNotificationEventKind::Started,
            vec!["user-1".into(), " user-1 ".into()],
            params(),
            Instant::from_unix_secs(1_000),
        )
        .unwrap()
    }

    /// 入队去重收件人，初始待投递。
    #[test]
    fn enqueue_deduplicates_recipients() {
        let item = pending();
        assert_eq!(item.recipient_user_ids, vec!["user-1".to_string()]);
        assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::Pending);
        assert_eq!(item.attempt_count, 0);
    }

    /// 成功后不得再取租约。
    #[test]
    fn delivered_message_cannot_be_leased() {
        let mut item = pending();
        item.acquire_lease(
            "worker-a",
            Instant::from_unix_secs(1_000),
            Instant::from_unix_secs(1_060),
        )
        .unwrap();
        item.mark_delivered().unwrap();
        assert_eq!(
            item.delivery_status,
            ApprovalNotificationDeliveryStatus::Delivered
        );
        assert!(item
            .acquire_lease(
                "worker-b",
                Instant::from_unix_secs(2_000),
                Instant::from_unix_secs(2_060)
            )
            .is_err());
    }

    /// 第 6 次失败进入死信；此前按固定退避。
    #[test]
    fn sixth_failure_dead_letters() {
        let mut item = pending();
        let mut now = 1_000_i64;
        for attempt in 1..=MAX_DELIVERY_ATTEMPTS {
            item.acquire_lease(
                "worker-a",
                Instant::from_unix_secs(now),
                Instant::from_unix_secs(now + 30),
            )
            .unwrap();
            item.mark_failure("TIMEOUT", Instant::from_unix_secs(now))
                .unwrap();
            if attempt < MAX_DELIVERY_ATTEMPTS {
                assert_eq!(item.delivery_status, ApprovalNotificationDeliveryStatus::Pending);
                let delay = RETRY_BACKOFF_SECS[(attempt - 1) as usize];
                assert_eq!(item.next_attempt_at.unix_secs(), now + delay);
                now += delay;
            }
        }
        assert_eq!(
            item.delivery_status,
            ApprovalNotificationDeliveryStatus::DeadLetter
        );
        assert!(item.dead_lettered_at.is_some());
        assert!(item
            .acquire_lease(
                "worker-b",
                Instant::from_unix_secs(now + 10),
                Instant::from_unix_secs(now + 40)
            )
            .is_err());
    }

    /// BSON 往返保持投递字段。
    #[test]
    fn outbox_roundtrips_through_bson() {
        let item = pending();
        let roundtrip: ApprovalNotificationOutbox =
            bson::deserialize_from_document(bson::serialize_to_document(&item).unwrap()).unwrap();
        assert_eq!(roundtrip, item);
    }
}
