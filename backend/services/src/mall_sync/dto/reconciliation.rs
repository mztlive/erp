use entities::common::time::Instant;
use entities::ids::{MallSalesReconciliationJobId, SalesOrderId, SalesOrderRevisionId, SourceSystemId};
use entities::mall_sync::{
    MallSalesReconciliationItem, MallSalesReconciliationJob, ReconciliationDifferenceType,
    ReconciliationItemStatus, ReconciliationJobStatus,
};
use serde::{Deserialize, Serialize};
use validator::Validate;

use crate::errors::Result;
use crate::query::{page_or_default, page_size_or_default};

use super::common::{non_blank, normalize_sort, PageParams};

/// 核对作业列表允许的排序字段白名单。
pub(crate) const MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS: &[&str] = &["created_at", "source_list_as_of"];
/// 核对差异明细列表允许的排序字段白名单。
pub(crate) const MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS: &[&str] = &["created_at", "source_updated_at"];

/// 核对差异明细入参（差异类型与 ERP 侧存在性一致性由实体层校验）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ReconciliationItemRequest {
    /// 来源单号。
    #[validate(custom(function = "non_blank", message = "来源单号不能为空"))]
    pub external_order_no: String,
    /// 商城当前状态码。
    #[validate(custom(function = "non_blank", message = "商城状态码不能为空"))]
    pub source_status_code: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 商城内容指纹。
    pub source_content_hash: Option<String>,
    /// ERP 当前正式销售单 ID（`ERP 缺失` 不得携带）。
    pub sales_order_id: Option<SalesOrderId>,
    /// ERP 当前正式销售版本 ID。
    pub erp_revision_id: Option<SalesOrderRevisionId>,
    /// ERP 内容指纹。
    pub erp_content_hash: Option<String>,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
}

/// 核对作业创建请求（`job_no` 唯一，重复提交按幂等返回既有作业）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct CreateMallSalesReconciliationJobRequest {
    /// 来源商城。
    pub source_system_id: SourceSystemId,
    /// 核对批次号（唯一）。
    #[validate(custom(function = "non_blank", message = "核对批次号不能为空"))]
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 商城清单数量。
    pub source_count: u64,
    /// ERP 数量。
    pub erp_count: u64,
    /// 差异明细（差异数量 = 明细条数）。
    #[validate(length(min = 1, max = 1000, message = "差异明细数量必须在1-1000之间"))]
    #[validate(nested)]
    pub items: Vec<ReconciliationItemRequest>,
}

/// 核对作业响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesReconciliationJobView {
    /// 实体主键。
    pub id: String,
    /// 来源商城。
    pub source_system_id: String,
    /// 核对批次号。
    pub job_no: String,
    /// 商城全量清单边界。
    pub source_list_as_of: Instant,
    /// 商城清单数量。
    pub source_count: u64,
    /// ERP 数量。
    pub erp_count: u64,
    /// 差异数量。
    pub difference_count: u64,
    /// 作业状态。
    pub status: ReconciliationJobStatus,
    /// 任务开始时间。
    pub started_at: Instant,
    /// 任务结束时间。
    pub finished_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesReconciliationJob> for MallSalesReconciliationJobView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `job` - 核对作业实体
    ///
    /// # 返回
    /// 返回响应视图。
    fn from(job: MallSalesReconciliationJob) -> Self {
        Self {
            id: job.base.id,
            source_system_id: job.source_system_id.to_string(),
            job_no: job.job_no,
            source_list_as_of: job.source_list_as_of,
            source_count: job.source_count,
            erp_count: job.erp_count,
            difference_count: job.difference_count,
            status: job.status,
            started_at: job.started_at,
            finished_at: job.finished_at,
            version: job.base.version,
            created_at: job.base.created_at,
        }
    }
}

/// 核对作业列表查询参数。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesReconciliationJobListParams {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业状态筛选。
    pub status: Option<ReconciliationJobStatus>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_list_as_of`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的核对作业列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesReconciliationJobListQuery {
    /// 来源商城筛选。
    pub source_system_id: Option<SourceSystemId>,
    /// 作业状态筛选。
    pub status: Option<ReconciliationJobStatus>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesReconciliationJobListParams {
    /// 归一化核对作业列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesReconciliationJobListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_RECONCILIATION_JOB_SORT_FIELDS,
        )?;
        Ok(MallSalesReconciliationJobListQuery {
            source_system_id: self.source_system_id.clone(),
            status: self.status,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 核对差异明细响应视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct MallSalesReconciliationItemView {
    /// 实体主键。
    pub id: String,
    /// 所属核对作业。
    pub reconciliation_job_id: String,
    /// 来源单号。
    pub external_order_no: String,
    /// 商城当前状态码。
    pub source_status_code: String,
    /// 商城更新时间。
    pub source_updated_at: Instant,
    /// 差异类型。
    pub difference_type: ReconciliationDifferenceType,
    /// 明细状态。
    pub status: ReconciliationItemStatus,
    /// 按单号补拉任务。
    pub single_order_sync_job_id: Option<String>,
    /// 人工处理结论。
    pub resolution: Option<String>,
    /// 处理人。
    pub resolved_by: Option<String>,
    /// 处理时间。
    pub resolved_at: Option<Instant>,
    /// 乐观锁版本。
    pub version: u64,
    /// 创建时间（秒级时间戳）。
    pub created_at: u64,
}

impl From<MallSalesReconciliationItem> for MallSalesReconciliationItemView {
    /// 从实体构造响应视图。
    ///
    /// # 参数
    /// * `item` - 差异明细实体
    ///
    /// # 返回
    /// 返回响应视图（不暴露比较键）。
    fn from(item: MallSalesReconciliationItem) -> Self {
        Self {
            id: item.base.id,
            reconciliation_job_id: item.reconciliation_job_id.to_string(),
            external_order_no: item.external_order_no,
            source_status_code: item.source_status_code,
            source_updated_at: item.source_updated_at,
            difference_type: item.difference_type,
            status: item.status,
            single_order_sync_job_id: item.single_order_sync_job_id.map(|id| id.to_string()),
            resolution: item.resolution,
            resolved_by: item.resolved_by,
            resolved_at: item.resolved_at,
            version: item.base.version,
            created_at: item.base.created_at,
        }
    }
}

/// 核对差异明细列表查询参数（按核对作业查询为主）。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct MallSalesReconciliationItemListParams {
    /// 所属核对作业。
    pub reconciliation_job_id: Option<MallSalesReconciliationJobId>,
    /// 明细状态筛选。
    pub status: Option<ReconciliationItemStatus>,
    /// 差异类型筛选。
    pub difference_type: Option<ReconciliationDifferenceType>,
    /// 页码（1 起）。
    #[validate(range(min = 1, message = "页码必须大于0"))]
    pub page: Option<u64>,
    /// 单页条数（1–100）。
    #[validate(range(min = 1, max = 100, message = "分页大小必须在1-100之间"))]
    pub page_size: Option<u32>,
    /// 排序字段（白名单：`created_at`/`source_updated_at`）。
    pub sort_by: Option<String>,
    /// 排序方向（`asc`/`desc`）。
    pub sort_dir: Option<String>,
}

/// 归一化后的核对差异明细列表查询参数。
#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct MallSalesReconciliationItemListQuery {
    /// 所属核对作业。
    pub reconciliation_job_id: Option<MallSalesReconciliationJobId>,
    /// 明细状态筛选。
    pub status: Option<ReconciliationItemStatus>,
    /// 差异类型筛选。
    pub difference_type: Option<ReconciliationDifferenceType>,
    /// 分页与排序参数。
    pub paging: PageParams,
}

impl MallSalesReconciliationItemListParams {
    /// 归一化核对差异明细列表查询参数。
    ///
    /// # 返回
    /// 返回不依赖仓储类型的规范化查询参数。
    ///
    /// # 错误
    /// 排序字段不在白名单或排序方向非法时返回 `ValidationError`。
    pub(crate) fn normalized(&self) -> Result<MallSalesReconciliationItemListQuery> {
        let (sort_by, sort_dir) = normalize_sort(
            &self.sort_by,
            &self.sort_dir,
            MALL_SALES_RECONCILIATION_ITEM_SORT_FIELDS,
        )?;
        Ok(MallSalesReconciliationItemListQuery {
            reconciliation_job_id: self.reconciliation_job_id.clone(),
            status: self.status,
            difference_type: self.difference_type,
            paging: PageParams {
                page: page_or_default(self.page),
                page_size: page_size_or_default(self.page_size),
                sort_by,
                sort_dir,
            },
        })
    }
}

/// 差异明细处理方式。
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ResolveItemKind {
    /// 人工解决（必须携带处理结论）。
    Resolve,
    /// 补拉后确认无误（不要求处理结论）。
    ConfirmNoDifference,
}

/// 差异明细处理请求。
#[derive(Debug, Clone, Serialize, Deserialize, Validate)]
pub struct ResolveMallSalesReconciliationItemRequest {
    /// 处理方式。
    pub kind: ResolveItemKind,
    /// 人工处理结论（`resolve` 必填）。
    pub resolution: Option<String>,
}

#[cfg(test)]
mod tests {
    use super::{
        CreateMallSalesReconciliationJobRequest, MallSalesReconciliationItemListParams,
        MallSalesReconciliationJobListParams, ReconciliationDifferenceType, ReconciliationItemRequest,
    };
    use crate::mall_sync::dto::common::SortDir;
    use entities::common::time::Instant;
    use entities::ids::SourceSystemId;
    use validator::Validate;

    fn item(order_no: &str) -> ReconciliationItemRequest {
        ReconciliationItemRequest {
            external_order_no: order_no.to_string(),
            source_status_code: "EFFECTIVE".to_string(),
            source_updated_at: Instant::from_unix_secs(1_700_000_000),
            source_content_hash: None,
            sales_order_id: None,
            erp_revision_id: None,
            erp_content_hash: None,
            difference_type: ReconciliationDifferenceType::ErpMissing,
        }
    }

    fn request(items: Vec<ReconciliationItemRequest>) -> CreateMallSalesReconciliationJobRequest {
        CreateMallSalesReconciliationJobRequest {
            source_system_id: SourceSystemId::new("mall-1"),
            job_no: "REC-1".to_string(),
            source_list_as_of: Instant::from_unix_secs(1_700_000_000),
            source_count: 1,
            erp_count: 0,
            items,
        }
    }

    #[test]
    fn reconciliation_queries_normalize_default_paging_and_sort() {
        let job_query = MallSalesReconciliationJobListParams {
            source_system_id: None,
            status: None,
            page: None,
            page_size: None,
            sort_by: None,
            sort_dir: None,
        }
        .normalized()
        .unwrap();
        assert_eq!(job_query.paging.page, 1);
        assert_eq!(job_query.paging.page_size, 20);
        assert_eq!(job_query.paging.sort_by, "created_at");
        assert_eq!(job_query.paging.sort_dir, SortDir::Desc);

        let item_query = MallSalesReconciliationItemListParams {
            reconciliation_job_id: None,
            status: None,
            difference_type: None,
            page: Some(3),
            page_size: Some(40),
            sort_by: Some("source_updated_at".to_string()),
            sort_dir: Some("asc".to_string()),
        }
        .normalized()
        .unwrap();
        assert_eq!(item_query.paging.page, 3);
        assert_eq!(item_query.paging.page_size, 40);
        assert_eq!(item_query.paging.sort_by, "source_updated_at");
        assert_eq!(item_query.paging.sort_dir, SortDir::Asc);
    }

    #[test]
    fn create_request_validates_each_difference_item_nested() {
        assert!(request(vec![item("SO-1")]).validate().is_ok());
        let invalid = request(vec![item("SO-1"), item("   ")]);
        let error = invalid.validate().expect_err("空白来源单号必须失败");
        assert!(error.to_string().contains("items"), "错误必须定位到 items");
    }

    #[test]
    fn create_request_enforces_item_count_bounds() {
        let full = (0..1000).map(|index| item(&format!("SO-{index}"))).collect();
        assert!(request(full).validate().is_ok());
        let overlong = (0..1001).map(|index| item(&format!("SO-{index}"))).collect();
        assert!(request(overlong).validate().is_err());
        assert!(request(Vec::new()).validate().is_err());
    }
}
