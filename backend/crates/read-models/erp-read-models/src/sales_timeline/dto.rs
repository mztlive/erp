use serde::{Deserialize, Serialize};

use crate::{Error, Result};

/// 时间线按业务环节筛选；审批独立于单据类型。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TimelineCategory {
    Sales,
    Approval,
    Procurement,
    Fulfillment,
    Receivable,
    Payable,
    Returns,
}

/// 同一接口用于完整时间线和最近动态。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct TimelineQuery {
    pub cursor: Option<String>,
    pub category: Option<TimelineCategory>,
    pub page_size: Option<u32>,
    #[serde(default)]
    pub include_details: bool,
}

/// 不包含内部审计载荷的业务动态。
#[derive(Debug, Clone, Serialize)]
pub struct TimelineEvent {
    pub id: String,
    pub occurred_at: u64,
    pub category: TimelineCategory,
    pub title: String,
    pub actor_name: String,
    #[serde(skip)]
    pub(super) actor_id: Option<String>,
    pub document_type: String,
    pub document_id: String,
    pub document_number: String,
    pub document_label: String,
    pub href: Option<String>,
    pub details: Vec<String>,
    pub amount: Option<String>,
    pub amount_label: Option<String>,
    pub business_date: Option<String>,
    pub after_submission: bool,
}

/// 时间线分页；历史覆盖说明独立于空列表。
#[derive(Debug, Serialize)]
pub struct TimelinePage {
    pub items: Vec<TimelineEvent>,
    pub next_cursor: Option<String>,
    pub notice: String,
}

/// 工作台可以打开的销售单入口，全部经过销售详情授权。
#[derive(Debug, Serialize)]
pub struct TimelineSalesSource {
    pub id: String,
    pub number: String,
}

/// 工作台办理动态，不替换审批提交材料。
#[derive(Debug, Serialize)]
pub struct TaskTimeline {
    pub timeline: TimelinePage,
    pub sales_orders: Vec<TimelineSalesSource>,
    pub submitted_at: Option<u64>,
}

#[derive(Debug)]
pub(super) struct Cursor {
    pub time: u64,
    pub id: String,
}

impl TimelineQuery {
    pub(super) fn limit(&self) -> Result<u32> {
        let limit = self.page_size.unwrap_or(30);
        if !(1..=100).contains(&limit) {
            return Err(Error::ValidationError("每次读取条数须为 1 至 100".into()));
        }
        Ok(limit)
    }

    pub(super) fn cursor(&self) -> Result<Option<Cursor>> {
        self.cursor.as_deref().map(Cursor::parse).transpose()
    }
}

impl Cursor {
    fn parse(raw: &str) -> Result<Self> {
        let error = || Error::ValidationError("时间线位置无效，请刷新后重试".into());
        let (time, id) = raw.split_once(':').ok_or_else(error)?;
        let time = time.parse::<u64>().map_err(|_| error())?;
        if time > i64::MAX.unsigned_abs() || id.is_empty() || id.len() > 256 {
            return Err(error());
        }
        Ok(Self { time, id: id.into() })
    }

    pub(super) fn accepts(&self, event: &TimelineEvent) -> bool {
        (event.occurred_at, event.id.as_str()) < (self.time, self.id.as_str())
    }
}

pub(super) fn page(mut items: Vec<TimelineEvent>, query: &TimelineQuery) -> Result<TimelinePage> {
    let cursor = query.cursor()?;
    items.retain(|item| {
        query.category.is_none_or(|category| item.category == category)
            && cursor.as_ref().is_none_or(|cursor| cursor.accepts(item))
    });
    items.sort_by(|a, b| (b.occurred_at, &b.id).cmp(&(a.occurred_at, &a.id)));
    items.dedup_by(|a, b| a.id == b.id);
    let limit = usize::try_from(query.limit()?).expect("分页上限可转换为 usize");
    let more = items.len() > limit;
    items.truncate(limit);
    let next_cursor =
        more.then(|| items.last().map(|item| format!("{}:{}", item.occurred_at, item.id))).flatten();
    Ok(TimelinePage {
        items,
        next_cursor,
        notice: "仅展示当前有权查看且已有记录的动作；缺失历史不按当前状态补写。销售单关闭后，后续票款及调整继续记录。".into(),
    })
}
