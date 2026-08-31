//! 批量选择快照与后台任务的父子聚合工厂。

use crate::bulk_job::{
    BackgroundJob, BackgroundJobData, BackgroundJobItem, BackgroundJobItemData, BulkSelectionItem,
    BulkSelectionItemData, BulkSelectionSnapshot, BulkSelectionSnapshotData, JobType, SelectionType,
};
use crate::command::CommandFingerprint;
use crate::common::time::Instant;
use crate::errors::{Error, Result};
use crate::ids::{
    BackgroundJobId, BackgroundJobItemId, BulkSelectionItemId, BulkSelectionSnapshotId, FileAssetId,
};

/// 选择快照聚合创建数据；目标数由子项自动派生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkSelectionSnapshotAggregateData {
    pub selection_type: SelectionType,
    pub data_cutoff_at: Instant,
    pub created_by: String,
    pub expires_at: Instant,
}

/// 选择快照子项草稿；父身份由聚合工厂统一注入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkSelectionItemDraft {
    pub id: BulkSelectionItemId,
    pub object_type: String,
    pub object_id: String,
    pub expected_version: Option<String>,
    pub expected_hash: Option<String>,
}

/// 已满足父子归属与数量不变量的选择快照聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BulkSelectionSnapshotAggregate {
    snapshot: BulkSelectionSnapshot,
    items: Vec<BulkSelectionItem>,
}

impl BulkSelectionSnapshotAggregate {
    /// 构造选择快照及全部冻结目标。
    ///
    /// # 错误
    /// 空目标、目标数超出 `u32`，或任一父子实体校验失败时返回错误。
    pub fn new(
        id: BulkSelectionSnapshotId,
        data: BulkSelectionSnapshotAggregateData,
        drafts: Vec<BulkSelectionItemDraft>,
    ) -> Result<Self> {
        let item_count = u32::try_from(drafts.len()).map_err(|_| Error::from("冻结目标数超出范围"))?;
        if item_count == 0 {
            return Err(Error::from("冻结目标不能为空"));
        }
        let items = drafts
            .into_iter()
            .map(|draft| {
                BulkSelectionItem::new(
                    draft.id,
                    BulkSelectionItemData {
                        selection_snapshot_id: id.clone(),
                        object_type: draft.object_type,
                        object_id: draft.object_id,
                        expected_version: draft.expected_version,
                        expected_hash: draft.expected_hash,
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let snapshot = BulkSelectionSnapshot::new(
            id,
            BulkSelectionSnapshotData {
                selection_type: data.selection_type,
                data_cutoff_at: data.data_cutoff_at,
                item_count,
                created_by: data.created_by,
                expires_at: data.expires_at,
            },
        )?;
        Ok(Self { snapshot, items })
    }

    /// 消费聚合并返回父实体与按输入顺序构造的子项。
    pub fn into_parts(self) -> (BulkSelectionSnapshot, Vec<BulkSelectionItem>) {
        (self.snapshot, self.items)
    }
}

/// 后台任务聚合创建数据；目标总数由逐项草稿自动派生。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobAggregateData {
    pub job_no: String,
    pub job_type: JobType,
    pub domain_job_type: Option<String>,
    pub domain_job_id: Option<String>,
    pub selection_snapshot_id: Option<BulkSelectionSnapshotId>,
    pub requested_by: String,
    pub request_id: String,
    pub input_file_asset_id: Option<FileAssetId>,
    pub result_file_asset_id: Option<FileAssetId>,
    pub declared_total_count: u64,
}

/// 后台任务子项草稿；父身份与连续序号由聚合工厂统一注入。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobItemDraft {
    pub id: BackgroundJobItemId,
    pub object_type: Option<String>,
    pub object_id: Option<String>,
    pub expected_version: Option<String>,
    pub expected_hash: Option<String>,
    pub worksheet_name: Option<String>,
    pub source_row_no: Option<u32>,
    pub source_column_name: Option<String>,
}

/// 已满足父子归属、连续序号与总数不变量的后台任务聚合。
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct BackgroundJobAggregate {
    job: BackgroundJob,
    items: Vec<BackgroundJobItem>,
}

impl BackgroundJobAggregate {
    /// 构造后台任务及全部逐项记录。
    ///
    /// # 错误
    /// 空目标、目标数超出 `u32`，或任一父子实体校验失败时返回错误。
    pub fn new(
        id: BackgroundJobId,
        data: BackgroundJobAggregateData,
        drafts: Vec<BackgroundJobItemDraft>,
    ) -> Result<Self> {
        if drafts.is_empty() {
            return Err(Error::from("逐项结果不能为空"));
        }
        let total_count = u64::try_from(drafts.len()).map_err(|_| Error::from("目标总数超出范围"))?;
        if data.declared_total_count != total_count {
            return Err(Error::from("目标总数必须与逐项结果数量一致"));
        }
        let items = drafts
            .into_iter()
            .enumerate()
            .map(|(index, draft)| {
                let item_no = u32::try_from(index + 1).map_err(|_| Error::from("逐项序号超出范围"))?;
                BackgroundJobItem::new(
                    draft.id,
                    BackgroundJobItemData {
                        background_job_id: id.clone(),
                        item_no,
                        object_type: draft.object_type,
                        object_id: draft.object_id,
                        expected_version: draft.expected_version,
                        expected_hash: draft.expected_hash,
                        worksheet_name: draft.worksheet_name,
                        source_row_no: draft.source_row_no,
                        source_column_name: draft.source_column_name,
                    },
                )
            })
            .collect::<Result<Vec<_>>>()?;
        let mut job = BackgroundJob::new(
            id,
            BackgroundJobData {
                job_no: data.job_no,
                job_type: data.job_type,
                domain_job_type: data.domain_job_type,
                domain_job_id: data.domain_job_id,
                selection_snapshot_id: data.selection_snapshot_id,
                requested_by: data.requested_by,
                request_id: data.request_id,
                input_file_asset_id: data.input_file_asset_id,
                result_file_asset_id: data.result_file_asset_id,
                total_count,
            },
        )?;
        job.attach_request_fingerprint(background_job_fingerprint(&job, &items))?;
        Ok(Self { job, items })
    }

    /// 消费聚合并返回父实体与严格按 `1..=N` 排列的子项。
    pub fn into_parts(self) -> (BackgroundJob, Vec<BackgroundJobItem>) {
        (self.job, self.items)
    }
}

/// 以规范化父子实体形成字段顺序固定的请求指纹。
fn background_job_fingerprint(job: &BackgroundJob, items: &[BackgroundJobItem]) -> CommandFingerprint {
    let mut parts = vec![
        "background-job-registration".to_string(),
        job.job_no.clone(),
        job.job_type.as_str().to_string(),
        job.requested_by.clone(),
        job.request_id.clone(),
        job.total_count.to_string(),
    ];
    push_optional(&mut parts, job.domain_job_type.as_deref());
    push_optional(&mut parts, job.domain_job_id.as_deref());
    push_optional(
        &mut parts,
        job.selection_snapshot_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
    );
    push_optional(
        &mut parts,
        job.input_file_asset_id
            .as_ref()
            .map(ToString::to_string)
            .as_deref(),
    );
    for item in items {
        parts.push("item".to_string());
        parts.push(item.item_no.to_string());
        push_optional(&mut parts, item.object_type.as_deref());
        push_optional(&mut parts, item.object_id.as_deref());
        push_optional(&mut parts, item.expected_version.as_deref());
        push_optional(&mut parts, item.expected_hash.as_deref());
        push_optional(&mut parts, item.worksheet_name.as_deref());
        push_optional(
            &mut parts,
            item.source_row_no.map(|value| value.to_string()).as_deref(),
        );
        push_optional(&mut parts, item.source_column_name.as_deref());
    }
    CommandFingerprint::from_parts(parts)
}

fn push_optional(parts: &mut Vec<String>, value: Option<&str>) {
    match value {
        Some(value) => {
            parts.push("some".to_string());
            parts.push(value.to_string());
        }
        None => parts.push("none".to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        BackgroundJobAggregate, BackgroundJobAggregateData, BackgroundJobItemDraft, BulkSelectionItemDraft,
        BulkSelectionSnapshotAggregate, BulkSelectionSnapshotAggregateData,
    };
    use crate::bulk_job::{JobType, SelectionType};
    use crate::common::time::Instant;
    use crate::ids::{BackgroundJobId, BackgroundJobItemId, BulkSelectionItemId, BulkSelectionSnapshotId};

    #[test]
    fn selection_aggregate_rejects_empty_and_derives_parent_and_count() {
        let data = BulkSelectionSnapshotAggregateData {
            selection_type: SelectionType::Export,
            data_cutoff_at: Instant::from_unix_secs(1),
            created_by: "actor-1".to_string(),
            expires_at: Instant::from_unix_secs(2),
        };
        assert!(BulkSelectionSnapshotAggregate::new(
            BulkSelectionSnapshotId::new("snapshot-empty"),
            data.clone(),
            vec![],
        )
        .is_err());

        let aggregate = BulkSelectionSnapshotAggregate::new(
            BulkSelectionSnapshotId::new("snapshot-1"),
            data,
            vec![BulkSelectionItemDraft {
                id: BulkSelectionItemId::new("item-1"),
                object_type: "sales_order".to_string(),
                object_id: "order-1".to_string(),
                expected_version: None,
                expected_hash: None,
            }],
        )
        .unwrap();
        let (snapshot, items) = aggregate.into_parts();
        assert_eq!(snapshot.item_count, 1);
        assert_eq!(
            items[0].selection_snapshot_id,
            BulkSelectionSnapshotId::new("snapshot-1")
        );
    }

    #[test]
    fn background_aggregate_derives_total_parent_and_continuous_sequence() {
        let data = BackgroundJobAggregateData {
            job_no: "JOB-1".to_string(),
            job_type: JobType::Batch,
            domain_job_type: None,
            domain_job_id: None,
            selection_snapshot_id: None,
            requested_by: "actor-1".to_string(),
            request_id: "request-1".to_string(),
            input_file_asset_id: None,
            result_file_asset_id: None,
            declared_total_count: 2,
        };
        let draft = |id| BackgroundJobItemDraft {
            id: BackgroundJobItemId::new(id),
            object_type: None,
            object_id: None,
            expected_version: None,
            expected_hash: None,
            worksheet_name: None,
            source_row_no: None,
            source_column_name: None,
        };
        assert!(
            BackgroundJobAggregate::new(BackgroundJobId::new("job-empty"), data.clone(), vec![]).is_err()
        );
        let mut mismatched = data.clone();
        mismatched.declared_total_count = 1;
        assert!(BackgroundJobAggregate::new(
            BackgroundJobId::new("job-mismatch"),
            mismatched,
            vec![draft("item-1"), draft("item-2")],
        )
        .is_err());
        let aggregate = BackgroundJobAggregate::new(
            BackgroundJobId::new("job-1"),
            data,
            vec![draft("item-1"), draft("item-2")],
        )
        .unwrap();
        let (job, items) = aggregate.into_parts();
        assert_eq!(job.total_count, 2);
        assert_eq!(
            items.iter().map(|item| item.item_no).collect::<Vec<_>>(),
            vec![1, 2]
        );
        assert!(items
            .iter()
            .all(|item| item.background_job_id == BackgroundJobId::new("job-1")));
    }
}
