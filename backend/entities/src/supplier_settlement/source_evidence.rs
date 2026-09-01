//! W27 结算来源证据批次。
//!
//! 当前 D32 已提供不可变履约明细成本与退款分配，但尚无可直接结算的外部账单、
//! 运费、服务费和关联到订单的取消事实。本实体把一次受控补证冻结为不可变批次；
//! W27 草稿创建与刷新只能消费该批次，不能接收客户端拼装的结算明细金额。

use std::collections::HashSet;

use chrono::{FixedOffset, TimeZone};
use entity_core::BaseModel;
use entity_macros::Entity;
use rust_decimal::Decimal;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::common::time::{BusinessDate, Instant};
use crate::errors::{Error, Result};
use crate::ids::{SupplierAccountId, SupplierFulfillmentItemId, SupplierFulfillmentOrderId};
use crate::money::{Amount, Quantity};
use crate::validation::normalize_required_text;

const COMMAND_ID_MAX_LEN: usize = 128;
const POLICY_VALUE_MAX_LEN: usize = 128;
const TIMEZONE_MAX_LEN: usize = 64;
const BILL_VALUE_MAX_LEN: usize = 128;
const EVIDENCE_REFERENCE_MAX_LEN: usize = 256;
const ACTOR_MAX_LEN: usize = 128;
const HASH_LEN: usize = 64;
const MAX_LINES: usize = 1_000;
const MAX_REFERENCES_PER_LINE: usize = 32;
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
        let end_secs = offset
            .from_local_datetime(&end_exclusive)
            .single()
            .expect("固定时区本地时刻无歧义")
            .timestamp();
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
        Self {
            gross: zero,
            net: zero,
            tax: zero,
        }
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
                Ok(Some(Self {
                    occurred_at,
                    reference_id,
                }))
            }
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

/// 供应商结算来源证据行构造数据。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceLineData {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 冻结数量。
    pub quantity: Quantity,
    /// 本行覆盖的正式事实类别。
    pub source_fact_types: Vec<SettlementSourceFactType>,
    /// 本行全部正式证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 订单成本金额。
    pub order: SettlementAmountComponents,
    /// 运费金额。
    pub freight: SettlementAmountComponents,
    /// 服务费金额。
    pub service_fee: SettlementAmountComponents,
    /// 退款金额。
    pub refund: SettlementAmountComponents,
    /// 供应商账单金额。
    pub supplier_billed: SettlementAmountComponents,
}

/// 一条来源证据的冻结金额和逐行身份。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceLine {
    /// 供应商子订单。
    pub supplier_fulfillment_order_id: SupplierFulfillmentOrderId,
    /// 供应商履约明细。
    pub supplier_fulfillment_item_id: SupplierFulfillmentItemId,
    /// 冻结数量。
    pub quantity: Quantity,
    /// 本行覆盖的正式事实类别。
    pub source_fact_types: Vec<SettlementSourceFactType>,
    /// 订单完成、取消、退款分配、费用及账单行的正式证据引用。
    pub evidence_reference_ids: Vec<String>,
    /// 订单成本含税金额。
    pub order_gross: Amount,
    /// 订单成本不含税金额。
    pub order_net: Amount,
    /// 订单成本税额。
    pub order_tax: Amount,
    /// 运费含税金额。
    pub freight_gross: Amount,
    /// 运费不含税金额。
    pub freight_net: Amount,
    /// 运费税额。
    pub freight_tax: Amount,
    /// 服务费含税金额。
    pub service_fee_gross: Amount,
    /// 服务费不含税金额。
    pub service_fee_net: Amount,
    /// 服务费税额。
    pub service_fee_tax: Amount,
    /// 退款含税金额。
    pub refund_gross: Amount,
    /// 退款不含税金额。
    pub refund_net: Amount,
    /// 退款税额。
    pub refund_tax: Amount,
    /// ERP 试算含税金额。
    pub erp_gross: Amount,
    /// ERP 试算不含税金额。
    pub erp_net: Amount,
    /// ERP 试算税额。
    pub erp_tax: Amount,
    /// 供应商账单行含税金额。
    pub supplier_billed_gross: Amount,
    /// 供应商账单行不含税金额。
    pub supplier_billed_net: Amount,
    /// 供应商账单行税额。
    pub supplier_billed_tax: Amount,
}

impl SupplierSettlementSourceEvidenceLine {
    /// 由正式事实和金额三元组构造来源证据行。
    ///
    /// ERP 金额统一由订单、运费、服务费和退款三元组逐项派生，调用方不得自行
    /// 拼装 `erp_gross/net/tax`。
    ///
    /// # 参数
    /// * `data` - 已完成跨域事实查询的行构造数据
    ///
    /// # 返回
    /// 返回已规范化事实类别、证据引用和 ERP 金额的来源证据行。
    ///
    /// # 错误
    /// 退款超过订单加费用、数量/证据非法或金额恒等不成立时返回领域错误。
    pub fn from_components(data: SupplierSettlementSourceEvidenceLineData) -> Result<Self> {
        let erp = data
            .order
            .checked_add(data.freight, "ERP 金额")?
            .checked_add(data.service_fee, "ERP 金额")?
            .checked_sub(data.refund, "ERP 金额")?;
        let mut line = Self {
            supplier_fulfillment_order_id: data.supplier_fulfillment_order_id,
            supplier_fulfillment_item_id: data.supplier_fulfillment_item_id,
            quantity: data.quantity,
            source_fact_types: data.source_fact_types,
            evidence_reference_ids: data.evidence_reference_ids,
            order_gross: data.order.gross,
            order_net: data.order.net,
            order_tax: data.order.tax,
            freight_gross: data.freight.gross,
            freight_net: data.freight.net,
            freight_tax: data.freight.tax,
            service_fee_gross: data.service_fee.gross,
            service_fee_net: data.service_fee.net,
            service_fee_tax: data.service_fee.tax,
            refund_gross: data.refund.gross,
            refund_net: data.refund.net,
            refund_tax: data.refund.tax,
            erp_gross: erp.gross,
            erp_net: erp.net,
            erp_tax: erp.tax,
            supplier_billed_gross: data.supplier_billed.gross,
            supplier_billed_net: data.supplier_billed.net,
            supplier_billed_tax: data.supplier_billed.tax,
        };
        line.validate()?;
        Ok(line)
    }

    /// 校验逐行身份、证据与金额恒等式。
    ///
    /// # 错误
    /// 缺少事实/证据、金额为负或任一 `gross = net + tax` 与 ERP 构成恒等不成立时返回错误。
    pub fn validate(&mut self) -> Result<()> {
        if self.quantity.to_decimal() <= Decimal::ZERO {
            return Err(Error::from("来源证据行数量必须大于零"));
        }
        self.source_fact_types.sort_by_key(|value| value.as_str());
        self.source_fact_types.dedup();
        if self.source_fact_types.is_empty() {
            return Err(Error::from("来源证据行至少包含一种正式事实"));
        }
        normalize_references(&mut self.evidence_reference_ids, MAX_REFERENCES_PER_LINE)?;
        if self.evidence_reference_ids.is_empty() {
            return Err(Error::from("来源证据行至少包含一个正式证据引用"));
        }
        for (value, field) in [
            (self.order_gross, "订单含税金额"),
            (self.order_net, "订单不含税金额"),
            (self.order_tax, "订单税额"),
            (self.freight_gross, "运费含税金额"),
            (self.freight_net, "运费不含税金额"),
            (self.freight_tax, "运费税额"),
            (self.service_fee_gross, "服务费含税金额"),
            (self.service_fee_net, "服务费不含税金额"),
            (self.service_fee_tax, "服务费税额"),
            (self.refund_gross, "退款含税金额"),
            (self.refund_net, "退款不含税金额"),
            (self.refund_tax, "退款税额"),
            (self.erp_gross, "ERP 含税金额"),
            (self.erp_net, "ERP 不含税金额"),
            (self.erp_tax, "ERP 税额"),
            (self.supplier_billed_gross, "供应商账单含税金额"),
            (self.supplier_billed_net, "供应商账单不含税金额"),
            (self.supplier_billed_tax, "供应商账单税额"),
        ] {
            ensure_non_negative(value, field)?;
        }
        ensure_triple(self.order_gross, self.order_net, self.order_tax, "订单金额")?;
        ensure_triple(self.freight_gross, self.freight_net, self.freight_tax, "运费金额")?;
        ensure_triple(
            self.service_fee_gross,
            self.service_fee_net,
            self.service_fee_tax,
            "服务费金额",
        )?;
        ensure_triple(self.refund_gross, self.refund_net, self.refund_tax, "退款金额")?;
        ensure_triple(self.erp_gross, self.erp_net, self.erp_tax, "ERP 金额")?;
        ensure_triple(
            self.supplier_billed_gross,
            self.supplier_billed_net,
            self.supplier_billed_tax,
            "供应商账单金额",
        )?;
        let expected_gross = self
            .order_gross
            .checked_add(self.freight_gross)
            .checked_add(self.service_fee_gross)
            .checked_sub(self.refund_gross);
        let expected_net = self
            .order_net
            .checked_add(self.freight_net)
            .checked_add(self.service_fee_net)
            .checked_sub(self.refund_net);
        let expected_tax = self
            .order_tax
            .checked_add(self.freight_tax)
            .checked_add(self.service_fee_tax)
            .checked_sub(self.refund_tax);
        if (expected_gross, expected_net, expected_tax) != (self.erp_gross, self.erp_net, self.erp_tax) {
            return Err(Error::from("ERP 金额必须逐项等于订单加费用减退款"));
        }
        Ok(())
    }
}

/// 不可变结算来源证据批次创建数据。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidenceData {
    /// 客户端稳定请求 ID。
    pub request_id: String,
    /// 供应商。
    pub supplier_id: SupplierAccountId,
    /// 结算期间开始（含）。
    pub period_start: BusinessDate,
    /// 结算期间结束（含）。
    pub period_end: BusinessDate,
    /// 供应商结算期间策略。
    pub period_policy_id: String,
    /// 期间策略冻结版本。
    pub period_policy_version: String,
    /// 期间策略时区。
    pub timezone: String,
    /// 同一供应商、期间和策略下单调递增的来源版本。
    pub source_version: u64,
    /// 外部账单号。
    pub external_bill_no: String,
    /// 外部账单版本。
    pub external_bill_version: String,
    /// 外部账单头证据引用。
    pub external_bill_evidence_reference_id: String,
    /// 逐行来源证据。
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    /// 本批次覆盖的正式事实水位。
    pub source_as_of: Instant,
    /// 记录人。
    pub recorded_by: String,
    /// 覆盖本批次全部输入与服务端派生金额的摘要。
    pub source_hash: String,
    /// 仅覆盖客户端强命令的幂等指纹。
    pub request_hash: String,
}

impl SupplierSettlementSourceEvidenceData {
    /// 计算覆盖批次身份、策略、账单与全部冻结行的稳定来源摘要。
    ///
    /// 摘要排除请求 ID、幂等键、记录人和记录时间，只覆盖可复核的业务事实；行、
    /// 事实类别和证据引用均按稳定顺序编码，调用顺序不影响结果。
    ///
    /// # 返回
    /// 返回 64 位小写 SHA-256 十六进制摘要。
    pub fn canonical_source_hash(&self) -> String {
        let mut parts = vec![
            "supplier-settlement-authoritative-source-v1".to_string(),
            self.supplier_id.to_string(),
            self.period_start.to_string(),
            self.period_end.to_string(),
            self.period_policy_id.trim().to_string(),
            self.period_policy_version.trim().to_string(),
            self.timezone.trim().to_string(),
            self.source_version.to_string(),
            self.external_bill_no.trim().to_string(),
            self.external_bill_version.trim().to_string(),
            self.external_bill_evidence_reference_id.trim().to_string(),
        ];
        let mut lines = self.lines.iter().collect::<Vec<_>>();
        lines.sort_by(|left, right| {
            left.supplier_fulfillment_item_id
                .as_ref()
                .cmp(right.supplier_fulfillment_item_id.as_ref())
        });
        for line in lines {
            append_line_digest_parts(&mut parts, line);
        }
        digest_parts(&parts)
    }
}

/// 不可变结算来源证据批次。
#[derive(Debug, Serialize, Deserialize, Clone, Entity, PartialEq, Eq)]
pub struct SupplierSettlementSourceEvidence {
    #[serde(flatten)]
    pub base: BaseModel,
    pub request_id: String,
    pub supplier_id: SupplierAccountId,
    pub period_start: BusinessDate,
    pub period_end: BusinessDate,
    pub period_policy_id: String,
    pub period_policy_version: String,
    pub timezone: String,
    pub source_version: u64,
    pub external_bill_no: String,
    pub external_bill_version: String,
    pub external_bill_evidence_reference_id: String,
    pub lines: Vec<SupplierSettlementSourceEvidenceLine>,
    pub source_as_of: Instant,
    pub recorded_by: String,
    pub source_hash: String,
    pub request_hash: String,
}

impl SupplierSettlementSourceEvidence {
    /// 校验来源命令没有重复提交同一履约明细。
    ///
    /// # 参数
    /// * `item_ids` - 客户端来源行中的履约明细主键
    ///
    /// # 返回
    /// 全部明细主键唯一时返回 `Ok(())`。
    ///
    /// # 错误
    /// 存在重复履约明细时返回领域错误。
    pub fn ensure_unique_item_ids(item_ids: &[SupplierFulfillmentItemId]) -> Result<()> {
        let mut unique = HashSet::with_capacity(item_ids.len());
        if item_ids.iter().any(|item_id| !unique.insert(item_id)) {
            return Err(Error::from("来源证据不得重复同一供应商履约明细"));
        }
        Ok(())
    }

    /// 校验候选来源版本严格高于当前批次。
    ///
    /// # 参数
    /// * `candidate` - 新来源批次声明的版本
    ///
    /// # 返回
    /// 候选版本更高时返回 `Ok(())`。
    ///
    /// # 错误
    /// 候选版本小于等于当前版本时返回领域错误。
    pub fn ensure_newer_source_version(&self, candidate: u64) -> Result<()> {
        if candidate <= self.source_version {
            return Err(Error::from(format!(
                "来源版本必须高于当前版本 {}",
                self.source_version
            )));
        }
        Ok(())
    }

    /// 判断幂等请求指纹是否与当前不可变批次一致。
    ///
    /// # 参数
    /// * `request_hash` - 当前命令计算出的稳定指纹
    ///
    /// # 返回
    /// 指纹一致时返回 `true`。
    pub fn matches_request_hash(&self, request_hash: &str) -> bool {
        self.request_hash == request_hash
    }

    /// 创建并完整校验一个不可变来源证据批次。
    ///
    /// # 错误
    /// 身份、周期、策略、账单、行数、逐行配对唯一性或摘要不合法时返回错误。
    pub fn new(id: impl Into<String>, data: SupplierSettlementSourceEvidenceData) -> Result<Self> {
        SettlementPeriod::new(data.period_start, data.period_end, &data.timezone)?;
        if data.source_version == 0 {
            return Err(Error::from("来源版本必须大于零"));
        }
        if data.lines.is_empty() || data.lines.len() > MAX_LINES {
            return Err(Error::from("来源证据行数必须在 1-1000 之间"));
        }
        let request_id = normalize_required_text(
            data.request_id,
            "来源证据请求ID不能为空",
            COMMAND_ID_MAX_LEN,
            "来源证据请求ID过长",
        )?;
        let period_policy_id = normalize_required_text(
            data.period_policy_id,
            "结算期间策略不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略过长",
        )?;
        let period_policy_version = normalize_required_text(
            data.period_policy_version,
            "结算期间策略版本不能为空",
            POLICY_VALUE_MAX_LEN,
            "结算期间策略版本过长",
        )?;
        let timezone = normalize_required_text(
            data.timezone,
            "结算期间策略时区不能为空",
            TIMEZONE_MAX_LEN,
            "结算期间策略时区过长",
        )?;
        let external_bill_no = normalize_required_text(
            data.external_bill_no,
            "外部账单号不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单号过长",
        )?;
        let external_bill_version = normalize_required_text(
            data.external_bill_version,
            "外部账单版本不能为空",
            BILL_VALUE_MAX_LEN,
            "外部账单版本过长",
        )?;
        let external_bill_evidence_reference_id = normalize_required_text(
            data.external_bill_evidence_reference_id,
            "外部账单证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "外部账单证据引用过长",
        )?;
        let recorded_by = normalize_required_text(
            data.recorded_by,
            "来源证据记录人不能为空",
            ACTOR_MAX_LEN,
            "来源证据记录人过长",
        )?;
        let source_hash = normalize_hash(data.source_hash)?;
        let request_hash = normalize_hash(data.request_hash)?;
        let mut lines = data.lines;
        let mut identities = HashSet::with_capacity(lines.len());
        for line in &mut lines {
            line.validate()?;
            let identity = (
                line.supplier_fulfillment_order_id.to_string(),
                line.supplier_fulfillment_item_id.to_string(),
            );
            if !identities.insert(identity) {
                return Err(Error::from("来源证据不得重复同一供应商订单明细"));
            }
        }
        lines.sort_by(|left, right| {
            left.supplier_fulfillment_item_id
                .as_ref()
                .cmp(right.supplier_fulfillment_item_id.as_ref())
        });

        Ok(Self {
            base: BaseModel::new(id.into()),
            request_id,
            supplier_id: data.supplier_id,
            period_start: data.period_start,
            period_end: data.period_end,
            period_policy_id,
            period_policy_version,
            timezone,
            source_version: data.source_version,
            external_bill_no,
            external_bill_version,
            external_bill_evidence_reference_id,
            lines,
            source_as_of: data.source_as_of,
            recorded_by,
            source_hash,
            request_hash,
        })
    }
}

/// 将一条冻结来源行按稳定字段顺序加入摘要输入。
///
/// # 参数
/// * `parts` - 待追加的长度前缀摘要字段
/// * `line` - 冻结来源证据行
///
/// # 返回
/// 无；事实类别和证据引用会先排序，保证摘要与集合输入顺序无关。
fn append_line_digest_parts(parts: &mut Vec<String>, line: &SupplierSettlementSourceEvidenceLine) {
    let mut fact_types = line
        .source_fact_types
        .iter()
        .map(|value| value.as_str())
        .collect::<Vec<_>>();
    fact_types.sort_unstable();
    fact_types.dedup();
    let mut references = line
        .evidence_reference_ids
        .iter()
        .map(String::as_str)
        .collect::<Vec<_>>();
    references.sort_unstable();
    references.dedup();
    parts.extend([
        line.supplier_fulfillment_order_id.to_string(),
        line.supplier_fulfillment_item_id.to_string(),
        line.quantity.to_string(),
        fact_types.join(","),
        references.join(","),
        line.order_gross.to_string(),
        line.order_net.to_string(),
        line.order_tax.to_string(),
        line.freight_gross.to_string(),
        line.freight_net.to_string(),
        line.freight_tax.to_string(),
        line.service_fee_gross.to_string(),
        line.service_fee_net.to_string(),
        line.service_fee_tax.to_string(),
        line.refund_gross.to_string(),
        line.refund_net.to_string(),
        line.refund_tax.to_string(),
        line.erp_gross.to_string(),
        line.erp_net.to_string(),
        line.erp_tax.to_string(),
        line.supplier_billed_gross.to_string(),
        line.supplier_billed_net.to_string(),
        line.supplier_billed_tax.to_string(),
    ]);
}

/// 对字段逐项加入长度前缀后计算稳定摘要。
///
/// # 参数
/// * `parts` - 按业务语义排序后的字段集合
///
/// # 返回
/// 返回 64 位小写 SHA-256 十六进制摘要。
fn digest_parts(parts: &[String]) -> String {
    let mut digest = Sha256::new();
    for part in parts {
        digest.update((part.len() as u64).to_be_bytes());
        digest.update(part.as_bytes());
    }
    digest
        .finalize()
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

/// 校验金额非负。
///
/// # 参数
/// * `value` - 待校验金额
/// * `field` - 错误消息使用的业务字段名称
///
/// # 返回
/// 金额非负时返回 `Ok(())`。
///
/// # 错误
/// 金额为负时返回领域错误。
fn ensure_non_negative(value: Amount, field: &str) -> Result<()> {
    if value.to_decimal() < Decimal::ZERO {
        return Err(Error::from(format!("{field}不得为负")));
    }
    Ok(())
}

fn ensure_triple(gross: Amount, net: Amount, tax: Amount, field: &str) -> Result<()> {
    if net.checked_add(tax) != gross {
        return Err(Error::from(format!("{field}必须满足含税等于不含税加税额")));
    }
    Ok(())
}

fn normalize_references(values: &mut Vec<String>, max: usize) -> Result<()> {
    if values.len() > max {
        return Err(Error::from("来源证据引用数量超限"));
    }
    for value in values.iter_mut() {
        *value = normalize_required_text(
            std::mem::take(value),
            "来源证据引用不能为空",
            EVIDENCE_REFERENCE_MAX_LEN,
            "来源证据引用过长",
        )?;
    }
    values.sort();
    values.dedup();
    Ok(())
}

fn normalize_hash(value: String) -> Result<String> {
    let value = value.trim().to_ascii_lowercase();
    if value.len() != HASH_LEN || !value.bytes().all(|byte| byte.is_ascii_hexdigit()) {
        return Err(Error::from("来源证据摘要必须是64位SHA-256十六进制值"));
    }
    Ok(value)
}

#[cfg(test)]
mod tests {
    use std::str::FromStr;

    use super::*;

    fn amount(value: &str) -> Amount {
        Amount::from_str(value).unwrap()
    }

    fn line() -> SupplierSettlementSourceEvidenceLine {
        SupplierSettlementSourceEvidenceLine {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("1.000000").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec!["fulfillment://order-1/item-1".to_string()],
            order_gross: amount("100.00"),
            order_net: amount("87.00"),
            order_tax: amount("13.00"),
            freight_gross: amount("10.00"),
            freight_net: amount("8.70"),
            freight_tax: amount("1.30"),
            service_fee_gross: amount("0.00"),
            service_fee_net: amount("0.00"),
            service_fee_tax: amount("0.00"),
            refund_gross: amount("5.00"),
            refund_net: amount("4.35"),
            refund_tax: amount("0.65"),
            erp_gross: amount("105.00"),
            erp_net: amount("91.35"),
            erp_tax: amount("13.65"),
            supplier_billed_gross: amount("105.00"),
            supplier_billed_net: amount("91.35"),
            supplier_billed_tax: amount("13.65"),
        }
    }

    fn data() -> SupplierSettlementSourceEvidenceData {
        SupplierSettlementSourceEvidenceData {
            request_id: "source-1".to_string(),
            supplier_id: SupplierAccountId::new("supplier-1"),
            period_start: BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            period_end: BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            period_policy_id: "monthly".to_string(),
            period_policy_version: "1".to_string(),
            timezone: "Asia/Shanghai".to_string(),
            source_version: 1,
            external_bill_no: "BILL-1".to_string(),
            external_bill_version: "1".to_string(),
            external_bill_evidence_reference_id: "bill://BILL-1/1".to_string(),
            lines: vec![line()],
            source_as_of: Instant::from_unix_secs(1_700_000_000),
            recorded_by: "finance-1".to_string(),
            source_hash: "a".repeat(64),
            request_hash: "b".repeat(64),
        }
    }

    #[test]
    fn source_evidence_accepts_complete_batch() {
        let evidence = SupplierSettlementSourceEvidence::new("source-1", data()).unwrap();
        assert_eq!(evidence.lines.len(), 1);
        assert_eq!(evidence.external_bill_no, "BILL-1");
    }

    #[test]
    fn source_evidence_rejects_duplicate_line_pair() {
        let mut input = data();
        input.lines.push(line());
        assert!(SupplierSettlementSourceEvidence::new("source-2", input).is_err());
    }

    #[test]
    fn source_line_rejects_guessed_tax_or_missing_reference() {
        let mut invalid = line();
        invalid.supplier_billed_tax = amount("0.00");
        assert!(invalid.validate().is_err());

        let mut missing = line();
        missing.evidence_reference_ids.clear();
        assert!(missing.validate().is_err());
    }

    #[test]
    fn period_and_cancel_evidence_own_pairing_and_timezone_rules() {
        let period = SettlementPeriod::new(
            BusinessDate::from_ymd(2026, 7, 1).unwrap(),
            BusinessDate::from_ymd(2026, 7, 31).unwrap(),
            SETTLEMENT_TIMEZONE,
        )
        .unwrap();
        let occurred_at = Instant::from_unix_secs(
            chrono::DateTime::parse_from_rfc3339("2026-07-01T00:00:00+08:00")
                .unwrap()
                .timestamp(),
        );
        assert!(period.contains(occurred_at));
        let cancel =
            SettlementCancelEvidence::from_optional(Some(occurred_at), Some(" proof-1 ".to_string()), period)
                .unwrap()
                .unwrap();
        assert_eq!(cancel.reference_id(), "proof-1");
        assert!(SettlementCancelEvidence::from_optional(Some(occurred_at), None, period).is_err());
        assert!(SettlementPeriod::new(period.end(), period.start(), SETTLEMENT_TIMEZONE).is_err());
        assert!(SettlementPeriod::new(period.start(), period.end(), "UTC").is_err());
    }

    #[test]
    fn period_secs_bounds_match_contains_across_dense_samples() {
        let start = BusinessDate::from_ymd(2026, 7, 1).unwrap();
        let end = BusinessDate::from_ymd(2026, 7, 31).unwrap();
        let period = SettlementPeriod::new(start, end, SETTLEMENT_TIMEZONE).unwrap();
        let (start_secs, end_secs) = SettlementPeriod::secs_bounds(start, end);
        // 以小时为步长覆盖期间前后各一天，秒级区间判定必须与上海业务日期
        // 判定完全一致（开始日 00:00 含、结束日次日 00:00 不含）。
        let first = chrono::DateTime::parse_from_rfc3339("2026-06-30T00:00:00+08:00")
            .unwrap()
            .timestamp();
        let last = chrono::DateTime::parse_from_rfc3339("2026-08-01T00:00:00+08:00")
            .unwrap()
            .timestamp();
        let mut cursor = first;
        while cursor <= last {
            let in_interval = (start_secs..end_secs).contains(&cursor);
            assert_eq!(
                period.contains(Instant::from_unix_secs(cursor)),
                in_interval,
                "秒级边界与业务日期判定在 {cursor} 不一致"
            );
            cursor += 3600;
        }
        assert!(start_secs < end_secs, "边界必须单调");
    }

    #[test]
    fn source_line_factory_derives_erp_components_and_rejects_excess_refund() {
        let data = SupplierSettlementSourceEvidenceLineData {
            supplier_fulfillment_order_id: SupplierFulfillmentOrderId::new("order-1"),
            supplier_fulfillment_item_id: SupplierFulfillmentItemId::new("item-1"),
            quantity: Quantity::from_str("1").unwrap(),
            source_fact_types: vec![SettlementSourceFactType::FulfillmentCompleted],
            evidence_reference_ids: vec!["proof-1".to_string()],
            order: SettlementAmountComponents::new(
                amount("100.00"),
                amount("87.00"),
                amount("13.00"),
                "订单金额",
            )
            .unwrap(),
            freight: SettlementAmountComponents::new(
                amount("10.00"),
                amount("8.70"),
                amount("1.30"),
                "运费金额",
            )
            .unwrap(),
            service_fee: SettlementAmountComponents::zero(),
            refund: SettlementAmountComponents::new(
                amount("5.00"),
                amount("4.35"),
                amount("0.65"),
                "退款金额",
            )
            .unwrap(),
            supplier_billed: SettlementAmountComponents::new(
                amount("105.00"),
                amount("91.35"),
                amount("13.65"),
                "供应商账单金额",
            )
            .unwrap(),
        };
        let built = SupplierSettlementSourceEvidenceLine::from_components(data.clone()).unwrap();
        assert_eq!(built.erp_gross, amount("105.00"));
        assert_eq!(built.erp_net, amount("91.35"));
        assert_eq!(built.erp_tax, amount("13.65"));

        let excess_refund = SupplierSettlementSourceEvidenceLineData {
            refund: SettlementAmountComponents::new(
                amount("111.00"),
                amount("96.57"),
                amount("14.43"),
                "退款金额",
            )
            .unwrap(),
            ..data
        };
        assert!(SupplierSettlementSourceEvidenceLine::from_components(excess_refund).is_err());
    }

    #[test]
    fn source_hash_and_source_version_rules_are_deterministic() {
        let mut first = data();
        let mut second_line = line();
        second_line.supplier_fulfillment_item_id = SupplierFulfillmentItemId::new("item-2");
        second_line.evidence_reference_ids = vec!["proof-2".to_string(), "proof-1".to_string()];
        first.lines.push(second_line);
        let first_hash = first.canonical_source_hash();

        let mut second = first.clone();
        second.lines.reverse();
        second.lines[0].evidence_reference_ids.reverse();
        assert_eq!(first_hash, second.canonical_source_hash());

        first.source_hash = first_hash;
        let evidence = SupplierSettlementSourceEvidence::new("source-1", first).unwrap();
        assert!(evidence.ensure_newer_source_version(2).is_ok());
        assert!(evidence.ensure_newer_source_version(1).is_err());
        assert!(evidence.matches_request_hash(&"b".repeat(64)));
        assert!(SupplierSettlementSourceEvidence::ensure_unique_item_ids(&[
            SupplierFulfillmentItemId::new("item-1"),
            SupplierFulfillmentItemId::new("item-1"),
        ])
        .is_err());
    }
}
