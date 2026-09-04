//! W20 供应商连接治理后台任务工厂（INT-E30）。
//!
//! 健康检查与目录同步任务的编号前缀、稳定 hash 派生、`total=1` 与请求字段，原先
//! 由 Service 的 `build_job` 私有 helper 拼装。本模块独占这组确定性构造：任务 ID、
//! 发起人与幂等摘要由 Service 显式注入，外部 worker 编排与落库仍归 Service；
//! 通用 BPM 不拥有 ERP 任务类型。

use serde::{Deserialize, Serialize};

use super::background_job::{BackgroundJob, BackgroundJobData, JobType};
use crate::errors::Result;
use crate::ids::BackgroundJobId;

/// W20 健康检查任务的领域任务类型代码。
pub const SUPPLIER_HEALTH_CHECK_JOB_TYPE: &str = "SUPPLIER_HEALTH_CHECK";
/// W20 目录同步任务的领域任务类型代码。
pub const SUPPLIER_CATALOG_SYNC_JOB_TYPE: &str = "SUPPLIER_CATALOG_SYNC";
/// 任务编号携带的幂等摘要长度（与既有 `w20-*` 派生口径一致）。
const JOB_NO_HASH_LEN: usize = 16;

/// W20 连接治理后台任务种类（健康检查或目录同步）。
#[derive(Debug, Clone, Copy, Serialize, Deserialize, PartialEq, Eq, Hash)]
pub enum SupplierGovernanceJobKind {
    /// 技术健康检查。
    HealthCheck,
    /// 商品目录同步。
    CatalogSync,
}

impl SupplierGovernanceJobKind {
    /// 返回领域任务类型代码。
    ///
    /// # 返回
    /// 返回健康检查或目录同步的稳定类型代码。
    pub fn domain_job_type(self) -> &'static str {
        match self {
            Self::HealthCheck => SUPPLIER_HEALTH_CHECK_JOB_TYPE,
            Self::CatalogSync => SUPPLIER_CATALOG_SYNC_JOB_TYPE,
        }
    }

    /// 返回任务编号前缀。
    ///
    /// # 返回
    /// 返回健康检查 `W20-HC` 或目录同步 `W20-CS` 前缀。
    pub fn job_no_prefix(self) -> &'static str {
        match self {
            Self::HealthCheck => "W20-HC",
            Self::CatalogSync => "W20-CS",
        }
    }
}

/// W20 连接治理后台任务构造规格（ID、发起人与摘要全部由 Service 注入）。
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct SupplierGovernanceJobSpec {
    /// 后台任务主键（Service 经 ID 生成器注入）。
    pub job_id: BackgroundJobId,
    /// 目标供应商连接 ID。
    pub connection_id: String,
    /// 治理任务种类。
    pub kind: SupplierGovernanceJobKind,
    /// 任务发起人。
    pub requested_by: String,
    /// 命令幂等摘要（十六进制，至少携带 [`JOB_NO_HASH_LEN`] 个字符）。
    pub idempotency_hash: String,
}

impl BackgroundJob {
    /// 为 W20 连接治理构造后台任务（健康检查或目录同步）。
    ///
    /// 任务编号为 `{W20-HC|W20-CS}-{幂等摘要前 16 字符}`，请求身份为
    /// `w20:{完整幂等摘要}`，目标总数恒为 `1`；初始状态与计数由
    /// [`BackgroundJob::new`] 按统一口径形成。
    ///
    /// # 参数
    /// * `spec` - 任务构造规格（ID、连接、种类、发起人与幂等摘要均由调用方注入）
    ///
    /// # 返回
    /// 返回待持久化的后台任务实体。
    ///
    /// # 错误
    /// 当幂等摘要过短、连接 ID/发起人为空或超长时返回错误；旧 `build_job` 的
    /// 无检查切片改为失败关闭，不再 panic。
    ///
    /// # 约束
    /// 确定性构造；不访问 MongoDB、时钟、全局 ID 生成器、密钥或外部网关。
    pub fn for_supplier_governance(spec: SupplierGovernanceJobSpec) -> Result<Self> {
        if spec.connection_id.trim().is_empty() {
            return Err(crate::errors::Error::from("连接 ID 不能为空"));
        }
        if spec.requested_by.trim().is_empty() {
            return Err(crate::errors::Error::from("发起人不能为空"));
        }
        let hash_prefix = spec
            .idempotency_hash
            .get(..JOB_NO_HASH_LEN)
            .filter(|prefix| prefix.len() == JOB_NO_HASH_LEN)
            .ok_or_else(|| crate::errors::Error::from("幂等摘要长度不足以派生任务编号"))?;
        Self::new(
            spec.job_id,
            BackgroundJobData {
                job_no: format!("{}-{hash_prefix}", spec.kind.job_no_prefix()),
                job_type: JobType::Sync,
                domain_job_type: Some(spec.kind.domain_job_type().to_string()),
                domain_job_id: Some(spec.connection_id),
                selection_snapshot_id: None,
                requested_by: spec.requested_by,
                request_id: format!("w20:{}", spec.idempotency_hash),
                input_file_asset_id: None,
                result_file_asset_id: None,
                total_count: 1,
            },
        )
    }
}

#[cfg(test)]
mod tests {
    use super::{
        SupplierGovernanceJobKind, SupplierGovernanceJobSpec, SUPPLIER_CATALOG_SYNC_JOB_TYPE,
        SUPPLIER_HEALTH_CHECK_JOB_TYPE,
    };
    use crate::bulk_job::{BackgroundJob, JobStatus, JobType};
    use crate::ids::BackgroundJobId;

    /// 构造 W20 任务规格测试夹具。
    fn job_spec(kind: SupplierGovernanceJobKind, hash: &str) -> SupplierGovernanceJobSpec {
        SupplierGovernanceJobSpec {
            job_id: BackgroundJobId::new("job-1"),
            connection_id: "conn-1".to_string(),
            kind,
            requested_by: "actor-1".to_string(),
            idempotency_hash: hash.to_string(),
        }
    }

    #[test]
    fn governance_job_fixes_prefix_hash_total_and_request_fields() {
        let hash = "0123456789abcdef0123456789abcdef";
        let health =
            BackgroundJob::for_supplier_governance(job_spec(SupplierGovernanceJobKind::HealthCheck, hash))
                .unwrap();
        assert_eq!(health.job_no, "W20-HC-0123456789abcdef");
        assert_eq!(health.job_type, JobType::Sync);
        assert_eq!(
            health.domain_job_type.as_deref(),
            Some(SUPPLIER_HEALTH_CHECK_JOB_TYPE)
        );
        assert_eq!(health.domain_job_id.as_deref(), Some("conn-1"));
        assert_eq!(health.request_id, format!("w20:{hash}"));
        assert_eq!(health.total_count, 1);
        assert_eq!(health.status, JobStatus::Pending);
        assert_eq!(health.requested_by, "actor-1");

        let catalog =
            BackgroundJob::for_supplier_governance(job_spec(SupplierGovernanceJobKind::CatalogSync, hash))
                .unwrap();
        assert_eq!(catalog.job_no, "W20-CS-0123456789abcdef");
        assert_eq!(
            catalog.domain_job_type.as_deref(),
            Some(SUPPLIER_CATALOG_SYNC_JOB_TYPE)
        );
        assert_eq!(catalog.total_count, 1);
    }

    #[test]
    fn governance_job_rejects_short_hash_without_panic() {
        assert!(BackgroundJob::for_supplier_governance(job_spec(
            SupplierGovernanceJobKind::HealthCheck,
            "short"
        ))
        .is_err());
        assert!(BackgroundJob::for_supplier_governance(job_spec(
            SupplierGovernanceJobKind::HealthCheck,
            "中文摘要长度不足以派生任务编号"
        ))
        .is_err());
    }

    #[test]
    fn governance_job_rejects_empty_connection_and_requester() {
        let mut spec = job_spec(
            SupplierGovernanceJobKind::HealthCheck,
            "0123456789abcdef0123456789abcdef",
        );
        spec.connection_id = "   ".to_string();
        assert!(BackgroundJob::for_supplier_governance(spec).is_err());

        let mut spec = job_spec(
            SupplierGovernanceJobKind::CatalogSync,
            "0123456789abcdef0123456789abcdef",
        );
        spec.requested_by = String::new();
        assert!(BackgroundJob::for_supplier_governance(spec).is_err());
    }

    #[test]
    fn governance_job_kind_exposes_stable_codes() {
        assert_eq!(
            SupplierGovernanceJobKind::HealthCheck.domain_job_type(),
            "SUPPLIER_HEALTH_CHECK"
        );
        assert_eq!(SupplierGovernanceJobKind::CatalogSync.job_no_prefix(), "W20-CS");
    }
}
