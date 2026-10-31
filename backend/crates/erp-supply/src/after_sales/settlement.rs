use std::num::NonZeroU32;

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::money::Amount;
use serde::{Deserialize, Serialize};

use super::identity;
use crate::ports::connector::common::{Scan, ScanWindow, Snapshot};
use crate::ports::connector::settlement::{SettlementEntry, SettlementMovement};
use crate::{Error, Result};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum SettlementScanState {
    Ready,
    Unsupported,
    Blocked,
}

#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SettlementScan {
    #[serde(flatten)]
    pub base: BaseModel,
    pub supplier_id: String,
    pub query: Scan,
    pub complete: bool,
    pub state: SettlementScanState,
    pub blocker: Option<String>,
    pub next_run_at: Instant,
    pub seen_cursors: Vec<String>,
}

impl SettlementScan {
    /// 构建连接级固定日窗口；首次从连接建立时间开始。
    /// # 参数
    /// `id` 为连接；`supplier` 为归属；`since` 为可靠起点；`now` 为本轮时间。
    /// # 返回
    /// 原游标为空的有界窗口。
    /// # 错误
    /// 无；未来起点收缩到当前时间前一秒。
    pub fn new(id: String, supplier: String, since: Instant, now: Instant) -> Self {
        let start = since.unix_secs().min(now.unix_secs().saturating_sub(1));
        Self {
            base: BaseModel::new(id),
            supplier_id: supplier,
            query: Scan {
                window: ScanWindow::Changes {
                    since: Instant::from_unix_secs(start),
                    through: Instant::from_unix_secs(start.saturating_add(86400).min(now.unix_secs())),
                },
                cursor: None,
                limit: NonZeroU32::new(50).expect("非零批次"),
            },
            complete: false,
            state: SettlementScanState::Ready,
            blocker: None,
            next_run_at: now,
            seen_cursors: vec![],
        }
    }

    /// 完成旧窗口后开始下一窗口，保留五分钟重叠用于来源迟到去重。
    /// # 参数
    /// `now` 为本次采样时间。
    /// # 返回
    /// 未完成窗口和原游标保持不变。
    /// # 错误
    /// 全量窗口不能作为增量检查点。
    pub fn resume(&mut self, now: Instant) -> Result<()> {
        if self.complete {
            let ScanWindow::Changes { through, .. } = self.query.window else {
                return Err(Error::ConflictError("结算检查点窗口损坏".into()));
            };
            let fresh = Self::new(
                self.base.id.clone(),
                self.supplier_id.clone(),
                Instant::from_unix_secs(through.unix_secs().saturating_sub(300)),
                now,
            );
            self.query = fresh.query;
            self.seen_cursors.clear();
            self.complete = false;
        }
        self.state = SettlementScanState::Ready;
        self.blocker = None;
        Ok(())
    }

    /// 整页证据保存事务内推进游标，禁止不前进及循环游标。
    /// # 参数
    /// `next` 为供应商原样游标；`now` 为本地采样时间。
    /// # 返回
    /// 完成窗口仅表示已读取，不表示财务已结算。
    /// # 错误
    /// 循环、空游标、游标过长或窗口页数过多时拒绝。
    pub fn advance(&mut self, next: Option<String>, now: Instant) -> Result<()> {
        if let Some(cursor) = &next {
            if cursor.is_empty()
                || cursor.len() > 4096
                || self.query.cursor.as_ref() == Some(cursor)
                || self.seen_cursors.contains(cursor)
                || self.seen_cursors.len() >= 1000
            {
                return Err(Error::ConflictError("结算分页没有可靠前进，请核对原窗口".into()));
            }
            self.seen_cursors.push(cursor.clone());
        }
        self.complete = next.is_none();
        self.query.cursor = next;
        self.next_run_at =
            Instant::from_unix_secs(now.unix_secs().saturating_add(if self.complete { 300 } else { 5 }));
        self.state = SettlementScanState::Ready;
        self.blocker = None;
        Ok(())
    }
}

/// 不可变来源流水；允许登记来源关联，禁止直接生成付款或核销。
#[derive(Debug, Clone, Serialize, Deserialize, Entity)]
pub struct SettlementEvidence {
    #[serde(flatten)]
    pub base: BaseModel,
    pub connection_id: String,
    pub supplier_id: String,
    pub evidence: Snapshot<SettlementEntry>,
    pub payload_hash: String,
    pub source_evidence_id: Option<String>,
}

impl SettlementEvidence {
    /// 同一来源流水只允许完全相同的业务载荷重复接收。
    /// # 参数
    /// `candidate` 为本次读取的标准来源。
    /// # 返回
    /// 重复观察成功，保留原关联状态。
    /// # 错误
    /// 同号异载荷或跨账号时拒绝，不能按接收时间覆盖。
    pub fn ensure_replay(&self, candidate: &Self) -> Result<()> {
        if self.base.id != candidate.base.id
            || self.connection_id != candidate.connection_id
            || self.payload_hash != candidate.payload_hash
        {
            return Err(Error::ConflictError("同一结算流水出现不同内容，原检查点不得推进".into()));
        }
        Ok(())
    }

    /// 校验流水身份、窗口、金额并固定连接作用域。
    /// # 参数
    /// `scan` 为原检查点；`evidence` 为真实标准来源。
    /// # 返回
    /// 同连接同流水生成相同主键；业务载荷独立摘要用于冲突检测。
    /// # 错误
    /// 缺少身份、时间越界、非法费用及序列化失败时拒绝。
    pub fn new(scan: &SettlementScan, evidence: Snapshot<SettlementEntry>) -> Result<Self> {
        let row = &evidence.value;
        let ScanWindow::Changes { since, through } = scan.query.window else {
            return Err(Error::ValidationError("结算必须使用固定时间窗口".into()));
        };
        if row.external_entry_id.trim().is_empty()
            || row.external_entry_id != row.external_entry_id.trim()
            || row.external_entry_id.len() > 512
            || row.order.merchant_order_no.trim().is_empty()
            || row.order.external_order_no.trim().is_empty()
            || row.order.external_order_no.len() > 512
            || row.order.merchant_order_no.len() > 128
            || row.occurred_at < since
            || row.occurred_at > through
        {
            return Err(Error::ValidationError("结算流水身份或时间窗口不匹配".into()));
        }
        match &row.movement {
            SettlementMovement::Charge { goods, shipping, service }
                if *goods >= Amount::zero() && *shipping >= Amount::zero() && *service >= Amount::zero() => {
            },
            SettlementMovement::Refund { external_refund_no, amount }
                if !external_refund_no.trim().is_empty()
                    && external_refund_no == external_refund_no.trim()
                    && external_refund_no.len() <= 128
                    && *amount > Amount::zero() => {},
            _ => return Err(Error::ValidationError("结算流水费用或退款身份无效".into())),
        }
        let payload =
            serde_json::to_string(row).map_err(|_| Error::ValidationError("结算来源无法冻结".into()))?;
        let id = identity(&[&scan.base.id, &row.external_entry_id]);
        Ok(Self {
            base: BaseModel::new(id),
            connection_id: scan.base.id.clone(),
            supplier_id: scan.supplier_id.clone(),
            evidence,
            payload_hash: identity(&[&payload]),
            source_evidence_id: None,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ports::connector::common::{SourceRevision, SourceStamp};
    use crate::ports::connector::order::OrderReference;
    fn scan() -> SettlementScan {
        SettlementScan::new(
            "connection".into(),
            "supplier".into(),
            Instant::from_unix_secs(100),
            Instant::from_unix_secs(200),
        )
    }
    fn evidence() -> Snapshot<SettlementEntry> {
        Snapshot {
            stamp: SourceStamp {
                revision: SourceRevision::Unversioned,
                changed_at: None,
                observed_at: Instant::from_unix_secs(201),
            },
            value: SettlementEntry {
                external_entry_id: "transaction".into(),
                order: OrderReference {
                    merchant_order_no: "order".into(),
                    external_order_no: "external".into(),
                },
                occurred_at: Instant::from_unix_secs(150),
                movement: SettlementMovement::Charge {
                    goods: "10".parse().unwrap(),
                    shipping: Amount::zero(),
                    service: Amount::zero(),
                },
            },
        }
    }
    #[test]
    fn fixed_window_survives_resume_and_empty_pages_must_advance() {
        let mut scan = scan();
        let window = scan.query.window.clone();
        scan.advance(Some("page-2".into()), Instant::now()).unwrap();
        scan.resume(Instant::now()).unwrap();
        assert_eq!(scan.query.window, window);
        assert_eq!(scan.query.cursor.as_deref(), Some("page-2"));
        assert!(scan.advance(Some("page-2".into()), Instant::now()).is_err());
        scan.advance(Some("page-3".into()), Instant::now()).unwrap();
        assert!(scan.advance(Some("page-2".into()), Instant::now()).is_err());
        scan.advance(None, Instant::from_unix_secs(201)).unwrap();
        assert!(scan.complete);
        scan.resume(Instant::from_unix_secs(600)).unwrap();
        assert!(!scan.complete);
        assert!(scan.query.cursor.is_none());
    }
    #[test]
    fn identity_is_connection_scoped_and_observation_time_is_not_business_payload() {
        let one = SettlementEvidence::new(&scan(), evidence()).unwrap();
        let mut repeated = evidence();
        repeated.stamp.observed_at = Instant::now();
        let repeated = SettlementEvidence::new(&scan(), repeated).unwrap();
        assert_eq!(one.base.id, repeated.base.id);
        assert_eq!(one.payload_hash, repeated.payload_hash);
        let mut other = scan();
        other.base.id = "other".into();
        assert_ne!(one.base.id, SettlementEvidence::new(&other, evidence()).unwrap().base.id);
        let mut changed = evidence();
        changed.value.movement =
            SettlementMovement::Refund { external_refund_no: "refund".into(), amount: "10".parse().unwrap() };
        let changed = SettlementEvidence::new(&scan(), changed).unwrap();
        assert_eq!(one.base.id, changed.base.id);
        assert_ne!(one.payload_hash, changed.payload_hash);
        assert!(one.ensure_replay(&changed).is_err());
        assert!(one.ensure_replay(&repeated).is_ok());
    }
    #[test]
    fn missing_identity_and_out_of_window_facts_are_rejected() {
        let mut row = evidence();
        row.value.external_entry_id.clear();
        assert!(SettlementEvidence::new(&scan(), row).is_err());
        let mut row = evidence();
        row.value.occurred_at = Instant::from_unix_secs(99);
        assert!(SettlementEvidence::new(&scan(), row).is_err());
        let mut row = evidence();
        row.value.movement =
            SettlementMovement::Refund { external_refund_no: String::new(), amount: "10".parse().unwrap() };
        assert!(SettlementEvidence::new(&scan(), row).is_err());
    }
}
