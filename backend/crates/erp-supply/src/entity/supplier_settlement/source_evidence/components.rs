//! 结算来源证据的期间、金额三元组与取消补证。

use chrono::{FixedOffset, TimeZone};
use erp_core::common::time::{BusinessDate, Instant};
use erp_core::money::Amount;
use erp_core::validation::normalize_required_text;
use erp_core::{Error, Result};
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};

use super::{EVIDENCE_REFERENCE_MAX_LEN, ensure_non_negative, ensure_triple};

/// 当前结算期间策略支持的固定时区。
pub const SETTLEMENT_TIMEZONE: &str = "Asia/Shanghai";

/// 已被来源证据批次冻结的正式事实类别。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementSourceFactType {
    /// 供应商订单完成事实。
    FulfillmentCompleted,
    /// 供应商取消结果证据。
    CancelConfirmed,
    /// 供应商退款事实与分配。
    RefundConfirmed,
}

impl SettlementSourceFactType {
    /// 返回摘要与审计使用的稳定代码。
    ///
    /// # 返回
    /// 返回不会随展示文案变化的正式事实代码。
    pub fn as_str(self) -> &'static str {
        match self {
            Self::FulfillmentCompleted => "FULFILLMENT_COMPLETED",
            Self::CancelConfirmed => "CANCEL_CONFIRMED",
            Self::RefundConfirmed => "REFUND_CONFIRMED",
        }
    }
}

/// 已校验的供应商结算期间。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementPeriod {
    start: BusinessDate,
    end: BusinessDate,
}

impl SettlementPeriod {
    /// 创建当前策略支持的上海时区结算期间。
    ///
    /// # 参数
    /// * `start` - 期间开始日期（含）
    /// * `end` - 期间结束日期（含）
    /// * `timezone` - 期间策略时区，当前必须为 `Asia/Shanghai`
    ///
    /// # 返回
    /// 返回可用于正式事实归期判断的期间值对象。
    ///
    /// # 错误
    /// 期间倒挂或时区不是当前固定策略时返回领域错误。
    pub fn new(start: BusinessDate, end: BusinessDate, timezone: &str) -> Result<Self> {
        if end < start {
            return Err(Error::from("来源证据期间结束不得早于开始"));
        }
        if timezone.trim() != SETTLEMENT_TIMEZONE {
            return Err(Error::from("当前结算期间策略只支持 Asia/Shanghai 时区"));
        }
        Ok(Self { start, end })
    }

    /// 返回期间开始日期。
    ///
    /// # 返回
    /// 返回包含边界的开始日期。
    pub fn start(self) -> BusinessDate {
        self.start
    }

    /// 返回期间结束日期。
    ///
    /// # 返回
    /// 返回包含边界的结束日期。
    pub fn end(self) -> BusinessDate {
        self.end
    }

    /// 判断时间点按上海业务日期是否落在当前期间内。
    ///
    /// # 参数
    /// * `value` - 待归期的正式事实时间
    ///
    /// # 返回
    /// 业务日期位于开始和结束边界之间时返回 `true`。
    pub fn contains(self, value: Instant) -> bool {
        let offset = FixedOffset::east_opt(8 * 60 * 60).expect("上海时区偏移合法");
        let date = value.as_utc().with_timezone(&offset).date_naive();
        date >= self.start.as_naive_date() && date <= self.end.as_naive_date()
    }

    /// 计算上海业务日期区间的秒级边界。
    ///
    /// 与 [`SettlementPeriod::contains`] 同口径：开始日 `00:00`（+08:00）含，
    /// 结束日次日 `00:00`（+08:00）不含；返回可直接用于秒级时间戳比较的
    /// `$gte`/`$lt` 边界。仓储层的时间范围过滤必须复用本方法，禁止在
    /// Repository 复制第二份边界计算。
    ///
    /// # 参数
    /// * `start` - 期间开始日期（含）
    /// * `end` - 期间结束日期（含）
    ///
    /// # 返回
    /// 返回 `(开始秒级时间戳, 结束次日零点的秒级时间戳)`。
    pub fn secs_bounds(start: BusinessDate, end: BusinessDate) -> (i64, i64) {
        const SHANGHAI_OFFSET_SECS: i32 = 8 * 3600;
        let offset = FixedOffset::east_opt(SHANGHAI_OFFSET_SECS).expect("上海时区偏移合法");
        let start_secs = offset
            .from_local_datetime(&start.as_naive_date().and_hms_opt(0, 0, 0).expect("午夜时刻合法"))
            .single()
            .expect("固定时区本地时刻无歧义")
            .timestamp();
        let end_exclusive = end
            .as_naive_date()
            .succ_opt()
            .expect("业务日期存在次日")
            .and_hms_opt(0, 0, 0)
            .expect("午夜时刻合法");
        let end_secs =
            offset.from_local_datetime(&end_exclusive).single().expect("固定时区本地时刻无歧义").timestamp();
        (start_secs, end_secs)
    }
}

/// 已校验的含税、不含税和税额三元组。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SettlementAmountComponents {
    /// 含税金额。
    pub gross: Amount,
    /// 不含税金额。
    pub net: Amount,
    /// 税额。
    pub tax: Amount,
}

impl SettlementAmountComponents {
    /// 创建非负且满足 `gross = net + tax` 的金额三元组。
    ///
    /// # 参数
    /// * `gross` - 含税金额
    /// * `net` - 不含税金额
    /// * `tax` - 税额
    /// * `field` - 错误消息使用的业务字段名称
    ///
    /// # 返回
    /// 返回已校验金额三元组。
    ///
    /// # 错误
    /// 任一金额为负或三元组恒等不成立时返回领域错误。
    pub fn new(gross: Amount, net: Amount, tax: Amount, field: &str) -> Result<Self> {
        ensure_non_negative(gross, &format!("{field}含税金额"))?;
        ensure_non_negative(net, &format!("{field}不含税金额"))?;
        ensure_non_negative(tax, &format!("{field}税额"))?;
        ensure_triple(gross, net, tax, field)?;
        Ok(Self { gross, net, tax })
    }

    /// 返回三项均为零的金额三元组。
    ///
    /// # 返回
    /// 返回合法零金额组合。
    pub fn zero() -> Self {
        let zero = Amount::try_from(Decimal::ZERO).expect("零是合法金额");
        Self { gross: zero, net: zero, tax: zero }
    }

    /// 将两组三元组逐项相加并校验结果。
    ///
    /// # 参数
    /// * `other` - 待累加金额三元组
    /// * `field` - 错误消息使用的业务字段名称
    ///
    /// # 返回
    /// 返回逐项相加后的合法金额三元组。
    ///
    /// # 错误
    /// 结果为负或恒等不成立时返回领域错误。
    pub fn checked_add(self, other: Self, field: &str) -> Result<Self> {
        Self::new(
            self.gross.checked_add(other.gross),
            self.net.checked_add(other.net),
            self.tax.checked_add(other.tax),
            field,
        )
    }

    /// 将两组三元组逐项相减并校验结果。
    ///
    /// # 参数
    /// * `other` - 待扣减金额三元组
    /// * `field` - 错误消息使用的业务字段名称
    ///
    /// # 返回
    /// 返回逐项相减后的合法金额三元组。
    ///
    /// # 错误
    /// 结果为负或恒等不成立时返回领域错误。
    pub fn checked_sub(self, other: Self, field: &str) -> Result<Self> {
        Self::new(
            self.gross.checked_sub(other.gross),
            self.net.checked_sub(other.net),
            self.tax.checked_sub(other.tax),
            field,
        )
    }
}

/// 已配对并规范化的取消正式证据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SettlementCancelEvidence {
    occurred_at: Instant,
    reference_id: String,
}

impl SettlementCancelEvidence {
    /// 从可选取消时间和证据引用构造取消证据。
    ///
    /// # 参数
    /// * `occurred_at` - 客户端补证的取消发生时间
    /// * `reference_id` - 与发生时间配对的正式证据引用
    /// * `period` - 当前结算期间
    ///
    /// # 返回
    /// 两项均省略时返回 `None`，两项完整且合法时返回规范化证据。
    ///
    /// # 错误
    /// 两项未成对、时间不在期间或证据引用为空/超长时返回领域错误。
    pub fn from_optional(
        occurred_at: Option<Instant>,
        reference_id: Option<String>,
        period: SettlementPeriod,
    ) -> Result<Option<Self>> {
        match (occurred_at, reference_id) {
            (None, None) => Ok(None),
            (Some(occurred_at), Some(reference_id)) => {
                if !period.contains(occurred_at) {
                    return Err(Error::from("取消补证发生时间不在结算期间"));
                }
                let reference_id = normalize_required_text(
                    reference_id,
                    "取消证据引用不能为空",
                    EVIDENCE_REFERENCE_MAX_LEN,
                    "取消证据引用过长",
                )?;
                Ok(Some(Self { occurred_at, reference_id }))
            },
            _ => Err(Error::from("取消发生时间与取消证据引用必须同时提供或同时省略")),
        }
    }

    /// 返回取消发生时间。
    ///
    /// # 返回
    /// 返回已确认落在结算期间内的时间点。
    pub fn occurred_at(&self) -> Instant {
        self.occurred_at
    }

    /// 返回规范化证据引用。
    ///
    /// # 返回
    /// 返回非空正式证据引用。
    pub fn reference_id(&self) -> &str {
        &self.reference_id
    }
}
