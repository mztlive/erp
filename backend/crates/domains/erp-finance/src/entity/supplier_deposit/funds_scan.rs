//! 固定资金查询窗口及逐页收件检查点；收件完成不表示订单已付款。

use entity_core::BaseModel;
use entity_macros::Entity;
use erp_core::common::time::Instant;
use erp_core::{Error, Result};
use serde::{Deserialize, Serialize};

use super::FundsEvidenceInput;
use super::funds_evidence::optional_identity;

#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsScanWindow {
    pub since: Instant,
    pub through: Instant,
}

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsEvidencePage {
    pub window: FundsScanWindow,
    pub requested_cursor: Option<String>,
    pub next_cursor: Option<String>,
    pub complete: bool,
    pub observed_at: Instant,
    pub source_revision: Option<String>,
    pub observations: Vec<FundsEvidenceInput>,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct FundsSavedPage {
    pub requested_cursor: Option<String>,
    pub payload_fingerprint: String,
}

#[derive(Clone, Serialize, Deserialize, Entity, PartialEq, Eq)]
pub struct FundsScanCheckpoint {
    #[serde(flatten)]
    pub base: BaseModel,
    pub account_id: String,
    pub source_account_reference: String,
    pub scan_key: String,
    pub window: FundsScanWindow,
    /// 下一次必须原样传回来源的游标，重启不得重建或猜测。
    pub cursor: Option<String>,
    pub page_number: u64,
    pub read_complete: bool,
    pub finished: bool,
    /// 仅已可靠收件且本页无Received后推进；不表示资金已核销或不存在扣款。
    pub completed_through: Option<Instant>,
    pub pending_inbox_ids: Vec<String>,
    pub last_page: Option<FundsSavedPage>,
}

impl FundsScanWindow {
    pub(crate) fn validate(&self) -> Result<()> {
        if self.since >= self.through {
            return Err(Error::from("资金调查窗口起止时间无效"));
        }
        Ok(())
    }
}

impl FundsEvidencePage {
    pub(crate) fn validate(&self) -> Result<()> {
        self.window.validate()?;
        validate_cursor(self.requested_cursor.as_deref())?;
        validate_cursor(self.next_cursor.as_deref())?;
        optional_identity(self.source_revision.as_deref())?;
        if self.observations.len() > 1000
            || self.complete != self.next_cursor.is_none()
            || (!self.complete && self.next_cursor == self.requested_cursor)
        {
            return Err(Error::from("资金证据页超过上限或无法证明游标继续推进"));
        }
        for observation in &self.observations {
            observation.validate()?;
        }
        Ok(())
    }
}

impl FundsScanCheckpoint {
    pub(crate) fn ensure_new_window(previous: Option<&Self>, window: FundsScanWindow) -> Result<()> {
        if let Some(previous) = previous {
            if !previous.finished {
                return Err(Error::from("须先恢复原资金固定窗口，不能另起查询游标"));
            }
            if previous
                .completed_through
                .is_some_and(|through| window.since > through || window.through < through)
            {
                return Err(Error::from("资金查询窗口不能跳过未读取范围或倒退完成范围"));
            }
        }
        Ok(())
    }
    pub(crate) fn ensure_page(&self, page: &FundsEvidencePage) -> Result<()> {
        page.validate()?;
        if self.finished
            || self.read_complete
            || self.window != page.window
            || self.cursor != page.requested_cursor
        {
            return Err(Error::from("资金查询必须继承原固定窗口及游标"));
        }
        Ok(())
    }
    pub(crate) fn advance(
        &mut self,
        page: &FundsEvidencePage,
        fingerprint: String,
        inbox_ids: Vec<String>,
    ) -> Result<()> {
        self.ensure_page(page)?;
        self.page_number = self.page_number.checked_add(1).ok_or_else(|| Error::from("资金查询页数超限"))?;
        self.cursor = page.next_cursor.clone();
        self.read_complete = page.complete;
        self.pending_inbox_ids = inbox_ids;
        self.last_page = Some(FundsSavedPage {
            requested_cursor: page.requested_cursor.clone(),
            payload_fingerprint: fingerprint,
        });
        Ok(())
    }
}

fn validate_cursor(value: Option<&str>) -> Result<()> {
    if value.is_some_and(|value| value.trim().is_empty() || value.len() > 4096) {
        return Err(Error::from("资金查询游标无效"));
    }
    Ok(())
}
