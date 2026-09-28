//! 补齐演示用的岗位账号、部门、审批流程和默认责任规则。删除演示数据时保留这些。

use application_core::AuditActor;

use super::DemoMasterDataService;
use crate::Result;

/// 岗位账号、部门与审批流程的准备结果。
#[derive(Debug, serde::Serialize)]
pub struct DemoFoundationReport {
    /// 本次新建的岗位账号数。
    pub accounts_created: u32,
    /// 已经存在的岗位账号数。
    pub accounts_existing: u32,
    /// 本次新发布的审批流程数。
    pub approvals_published: u32,
    /// 已发布且节点与规格一致的审批流程数。
    pub approvals_existing: u32,
    /// 已发布但节点与规格不一致、因此未改动的审批流程数。
    pub approvals_mismatched: u32,
    /// 本次新建的默认采购或财务责任规则数。
    pub responsibilities_created: u32,
    /// 已有启用规则、因此保留现有负责人的数量。
    pub responsibilities_existing: u32,
    /// 需要告诉操作人的说明。
    pub notices: Vec<String>,
}

impl DemoMasterDataService {
    /// 补齐岗位账号、部门、尚未发布的审批流程，以及缺失的默认责任规则。
    ///
    /// # 参数
    /// * `actor` - 当前操作人，须能创建账号、调整组织、发布审批流程和维护责任规则
    ///
    /// # 返回
    /// 返回新建和已存在的数量。已有账号不改密码。已发布但审批人不同的流程只在说明里标出。
    /// 已启用的默认采购调度人或财务责任规则保留现有负责人。
    ///
    /// # 错误
    /// 环境未开放，或账号、部门、审批流程、责任规则写入失败时返回错误。
    pub async fn ensure_foundation(&self, actor: &AuditActor) -> Result<DemoFoundationReport> {
        self.ensure_enabled()?;
        let mut report = DemoFoundationReport {
            accounts_created: 0,
            accounts_existing: 0,
            approvals_published: 0,
            approvals_existing: 0,
            approvals_mismatched: 0,
            responsibilities_created: 0,
            responsibilities_existing: 0,
            notices: Vec::new(),
        };
        let accounts = self.ensure_accounts(actor, &mut report).await?;
        self.ensure_departments(actor, &accounts.by_login, &mut report.notices).await?;
        self.ensure_approvals(actor, &accounts, &mut report).await?;
        self.ensure_responsibilities(actor, &accounts, &mut report).await?;
        Ok(report)
    }
}
