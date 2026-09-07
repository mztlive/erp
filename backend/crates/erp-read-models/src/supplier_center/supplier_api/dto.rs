//! 连接后台任务展示；任务状态归 support。
use serde::Serialize;
/// 后台任务查询视图。
#[derive(Debug, Clone, Serialize, PartialEq, Eq)]
pub struct SupplierConnectionJobView {
    pub job_id: String,
    pub job_no: String,
    pub action: String,
    pub status: erp_support::JobStatus,
    pub total: u64,
    pub processed: u64,
    pub succeeded: u64,
    pub failed: u64,
    pub error_summary: Option<String>,
    pub created_at: u64,
    pub finished_at: Option<u64>,
}
