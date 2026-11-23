//! 工作项查询阶段批量加载往来方名称与稳定 ID 的只读端口。

use std::collections::HashMap;

use async_trait::async_trait;
use persistence_core::Executor;

use crate::entity::work_item::{WorkItemLink, WorkItemType};
use crate::error::Result;

/// 已授权工作项的往来方解析键。
#[derive(Debug, Clone)]
pub struct WorkItemCounterpartyLookup {
    pub work_item_id: String,
    pub work_item_type: WorkItemType,
    pub business_object_type: String,
    pub business_object_id: String,
    pub subject_version: String,
    pub links: Vec<WorkItemLink>,
}

/// 已解析的任务往来方事实；仅用于任务队列与统计内的搜索和筛选。
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct WorkItemCounterpartyFact {
    pub counterparty_ids: Vec<String>,
    pub counterparty_label: Option<String>,
}

/// 批量解析已授权工作项的往来方名称和稳定 ID。
#[async_trait]
pub trait WorkItemCounterpartyPort: Send + Sync {
    async fn load_counterparties(
        &self,
        items: &[WorkItemCounterpartyLookup],
        executor: &mut dyn Executor,
    ) -> Result<HashMap<String, WorkItemCounterpartyFact>>;
}
