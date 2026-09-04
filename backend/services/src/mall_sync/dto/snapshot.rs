use entities::common::time::Instant;
use entities::ids::{MallSalesSyncJobId, SourceSystemId};
use entities::mall_sync::{MallSalesOrderSnapshot, SnapshotMappingStatus};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 快照列表允许的排序字段白名单。
pub(crate) const MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS: &[&str] =
    &["created_at", "source_updated_at", "observed_at"];

/// 单条快照入参（来源单身份 + 商城侧值；来源商城与观察时间由服务端注入）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct SnapshotItemRequest {
    /// 一期来源单号原值。
    #[validate(custom(function = "non_blank", message = "来源单号不能为空"))]
    pub external_order_no: String,
    /// 商城更新时间（秒级时间戳）。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹（可选列，仅用于变更判断）。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    #[validate(custom(function = "non_blank", message = "商城状态码不能为空"))]
    pub source_status_code: String,
    /// 规范化外部快照归档。
    #[validate(custom(function = "non_blank", message = "规范化快照不能为空"))]
    pub normalized_snapshot: String,
    /// 可选的加密原始报文引用。
    pub raw_payload_reference: Option<String>,
}

/// 快照落盘请求（一次一页；`(来源商城, 比较键, 商城更新时间)` 唯一，重复推送按幂等跳过）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct IngestMallSalesOrderSnapshotsRequest {
    /// 来源同步作业。
    pub sync_job_id: MallSalesSyncJobId,
    /// 本页快照。
    #[validate(length(min = 1, max = 500, message = "快照数量必须在1-500之间"))]
    #[validate(nested)]
    pub items: Vec<SnapshotItemRequest>,
}

/// 快照落盘结果（重复/迟到快照计入 `skipped`，不产生重复事实）。
#[derive(Debug, Clone, Serialize)]
pub struct IngestMallSalesOrderSnapshotsResult {
    /// 本页实际落盘条数。
    pub accepted: u64,
    /// 本页跳过条数（事实键重复或早于最新快照）。
    pub skipped: u64,
    /// 已落盘快照 ID。
    pub snapshot_ids: Vec<String>,
}

/// 商城销售单快照响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesOrderSnapshotView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 一期来源单号原值。
    pub external_order_no: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商业事实投影指纹。
    pub content_hash: Option<String>,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// ERP 实际观察时间。
    pub observed_at: Instant,
    /// 映射状态。
    pub mapping_status: SnapshotMappingStatus,
    /// 成功形成的销售版本。
    pub applied_sales_order_revision_id: Option<String>,
    /// 来源任务。
    pub sync_job_id: String,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesOrderSnapshot> for MallSalesOrderSnapshotView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `snapshot` - 快照实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露比较键与原始报文引用）。
    fn from(snapshot: MallSalesOrderSnapshot) -> Self {
        Self {
            id: snapshot.base.id,
            source_system_id: snapshot.source_system_id.to_string(),
            external_order_no: snapshot.external_order_no,
            source_updated_at: snapshot.source_updated_at,
            content_hash: snapshot.content_hash,
            source_status_code: snapshot.source_status_code,
            observed_at: snapshot.observed_at,
            mapping_status: snapshot.mapping_status,
            applied_sales_order_revision_id: snapshot
                .applied_sales_order_revision_id
                .map(|id| id.to_string()),
            sync_job_id: snapshot.sync_job_id.to_string(),
            version: snapshot.base.version,
            created_at: snapshot.base.created_at,
        }
    }
}

/// 快照列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesOrderSnapshotListParams {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<SnapshotMappingStatus>,
    /// 观察时间起（含）。
    pub observed_at_from: Option<Instant>,
    /// 观察时间止（含）。
    pub observed_at_to: Option<Instant>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_updated_at`/`observed_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的快照列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesOrderSnapshotListQuery {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 映射状态筛选。
    pub mapping_status: Option<SnapshotMappingStatus>,
    /// 观察时间起（含）。
    pub observed_at_from: Option<Instant>,
    /// 观察时间止（含）。
    pub observed_at_to: Option<Instant>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesOrderSnapshotListParams {
    /// 归一化快照列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesOrderSnapshotListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_ORDER_SNAPSHOT_SORT_FIELDS,
        )?;
        Ok(MallSalesOrderSnapshotListQuery {
            source_system_id: self.source_system_id.clone(),
            mapping_status: self.mapping_status,
            observed_at_from: self.observed_at_from,
            observed_at_to: self.observed_at_to,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

#[cfg(test)]
mod tests {
    use super::{IngestMallSalesOrderSnapshotsRequest, SnapshotItemRequest};
    use entities::common::time::Instant;
    use entities::ids::MallSalesSyncJobId;
    use validator::Validate;

    fn item(order_no: &str, status: &str) -> SnapshotItemRequest {
        SnapshotItemRequest {
            external_order_no: order_no.to_string(),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            content_hash: None,
            source_status_code: status.to_string(),
            normalized_snapshot: "{}".to_string(),
            raw_payload_reference: None,
        }
    }

    fn request(items: Vec<SnapshotItemRequest>) -> IngestMallSalesOrderSnapshotsRequest {
        IngestMallSalesOrderSnapshotsRequest {
            sync_job_id: MallSalesSyncJobId::new("j-1"),
            items,
        }
    }

    #[test]
    fn ingest_request_rejects_empty_items() {
        let request: IngestMallSalesOrderSnapshotsRequest =
            serde_json::from_value(serde_json::json!({ "sync_job_id": "j-1", "items": [] })).unwrap();
        assert!(request.validate().is_err());
    }

    #[test]
    fn ingest_request_validates_each_item_nested() {
        assert!(request(vec![item("SO-1", "EFFECTIVE")]).validate().is_ok());
        let invalid_order = request(vec![item("SO-1", "EFFECTIVE"), item("   ", "EFFECTIVE")]);
        let error = invalid_order.validate().expect_err("空白来源单号必须失败");
        assert!(error.to_string().contains("items"), "错误必须定位到 items");
        let invalid_status = request(vec![item("SO-1", "   ")]);
        assert!(invalid_status.validate().is_err());
    }

    #[test]
    fn ingest_request_enforces_item_count_bounds() {
        let full = (0..500)
            .map(|index| item(&format!("SO-{index}"), "EFFECTIVE"))
            .collect();
        assert!(request(full).validate().is_ok());
        let overlong = (0..501)
            .map(|index| item(&format!("SO-{index}"), "EFFECTIVE"))
            .collect();
        assert!(request(overlong).validate().is_err());
    }
}
