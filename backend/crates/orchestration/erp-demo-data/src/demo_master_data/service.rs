//! 按固定清单分批生成演示主数据。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_identity::{
    AccessControlExt, AccessTarget, AccountCoreRepositoryExt, AuthorizationService, SharedRbacService,
};
use erp_party::PartyStatus;
use erp_permissions::demo_master_data;
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, ReadExecutor, Transactional};
use storage::S3Storage;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{self, DemoCounts, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::seed::SeedRequest;
use super::{DemoMasterDataProcess, offerings};
use crate::runtime_ports::DemoRuntimePorts;
use crate::{Error, Result};

/// 继续生成时提交的清单位置。
#[derive(Debug, Default, serde::Deserialize)]
pub struct ApplyDemoMasterDataRequest {
    /// 固定清单下标。第一次为 0。
    #[serde(default)]
    pub cursor: u32,
}

/// 一批演示主数据的处理结果。
#[derive(Debug, serde::Serialize)]
pub struct DemoChunkReport {
    /// 下一次生成应从这里继续。
    pub next_cursor: u32,
    /// 固定清单总条数。
    pub total_steps: u32,
    /// 本轮之后是否已经走完。
    pub done: bool,
    /// 新写入条数。
    pub created: u32,
    /// 恢复条数。
    pub restored: u32,
    /// 已存在而跳过的条数。
    pub skipped: u32,
    /// 删除的主数据条数。
    pub removed: u32,
    /// 一并硬删除的关联记录条数，含历史修订和创建命令。
    pub derived_removed: u32,
    /// 跳过某些主数据时的说明。
    pub notices: Vec<String>,
}

/// 演示主数据当前数量。
#[derive(Debug, serde::Serialize)]
pub struct DemoStatus {
    /// 当前环境是否允许生成和删除。
    pub enabled: bool,
    /// 固定清单条数。
    pub planned: DemoCounts,
    /// 仍在列表中的条数。
    pub active: DemoCounts,
    /// 旧版本已软删除、等待硬删除的登记条数。
    pub removed: DemoCounts,
}

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    /// 创建演示主数据服务。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    /// * `ports` - 演示入口实际消费的已装配领域与编排合同
    /// * `identity` - 账号、角色和演示数据重置的身份管理设施
    /// * `authorization` - 进程共享授权服务
    /// * `enabled` - 配置是否允许写入
    /// * `storage` - 演示商品图片使用的受控对象存储
    /// * `image_secret` - 内容指纹密钥
    ///
    /// # 返回
    /// 返回未执行任何写入的服务。
    ///
    /// # 错误
    /// 不返回错误。
    pub fn new(
        db: mongodb::Database,
        ports: DemoRuntimePorts<A>,
        identity: SharedRbacService,
        authorization: Arc<AuthorizationService>,
        enabled: bool,
        storage: S3Storage,
        image_secret: &[u8],
    ) -> Self {
        Self { db, ports, identity, authorization, enabled, storage, image_secret: image_secret.to_vec() }
    }

    /// 返回各类演示主数据的计划数量和当前数量。
    ///
    /// # 参数
    /// actor 为当前认证账号，必须具有演示状态读取权限。
    ///
    /// # 返回
    /// 环境关闭时仍返回数量，`enabled` 为 false。
    ///
    /// # 错误
    /// 身份或权限失效、授权交付检查失败、清单查询或种子校验失败时返回错误。
    pub async fn status(&self, actor: &AuditActor) -> Result<DemoStatus> {
        let db = self.db.clone();
        let authorization = self.authorization.clone();
        let enabled = self.enabled;
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { status_snapshot(&db, &authorization, enabled, &actor, executor).await })
            })
            .await
    }

    /// 生成或恢复下一批演示主数据。
    ///
    /// # 参数
    /// * `actor` - 当前操作人
    /// * `cursor` - 固定清单下标
    ///
    /// # 返回
    /// 返回本批结果和下一次下标。
    ///
    /// # 错误
    /// 环境未开放或某条主数据写入失败时返回错误。已写入的条数会留在清单里。
    pub async fn apply_chunk(&self, actor: &AuditActor, cursor: u32) -> Result<DemoChunkReport> {
        self.ensure_enabled()?;
        self.authorize_apply(actor).await?;
        let steps = plan::demo_steps()?;
        let offerings = offerings::load(&steps)?;
        let range = plan::apply_window(cursor as usize, steps.len());
        let mut report = empty_report(range.end, steps.len());
        let company_party_id = self.company_party_id(actor, &mut report.notices).await?;
        let handler_user_id = self.warehouse_handler_id().await?;
        for step in &steps[range] {
            let records = self.record_map().await?;
            let outcome = self
                .create_record(
                    actor,
                    step,
                    &records,
                    company_party_id.as_deref(),
                    handler_user_id.as_deref(),
                    &mut report.notices,
                )
                .await?;
            if step.kind == plan::DemoKind::Product {
                let records = self.record_map().await?;
                self.ensure_product_defaults(actor, step, &records).await?;
                self.ensure_sellable(step, &offerings, &records).await?;
            }
            tally(&mut report, outcome)?;
        }
        Ok(report)
    }

    /// 在生成步骤开始前核对演示入口资格，领域写入和恢复继续执行各自的事务授权。
    /// # 参数
    /// actor 为发起演示生成的当前账号。
    /// # 返回
    /// 当前快照中的账号具备演示生成权限时成功。
    /// # 错误
    /// 身份失效、缺少功能权限或事务取证失败时拒绝。
    pub(super) async fn authorize_apply(&self, actor: &AuditActor) -> Result<()> {
        let authorization = self.authorization.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    authorization
                        .require(&actor, &demo_master_data::APPLY, AccessTarget::Operation, executor)
                        .await?;
                    Ok(())
                })
            })
            .await
    }

    /// 拒绝在未开放演示功能的环境执行写入。
    ///
    /// # 参数
    /// 无；读取当前环境开关。
    ///
    /// # 返回
    /// 开放时返回空结果。
    ///
    /// # 错误
    /// 未开放时返回禁止操作错误。
    pub(super) fn ensure_enabled(&self) -> Result<()> {
        if self.enabled {
            Ok(())
        } else {
            Err(Error::Forbidden("当前环境未开放演示主数据".to_string()))
        }
    }

    /// 按 JSON 请求种类调用对应领域写入入口。
    async fn create_record(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        records: &HashMap<String, DemoMasterRecord>,
        company_party_id: Option<&str>,
        handler_user_id: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        match &step.request {
            SeedRequest::Unit(request) => self.ensure_unit(actor, step, request).await,
            SeedRequest::Brand(request) => self.ensure_brand(actor, step, request).await,
            SeedRequest::Category(request) => self.ensure_category(actor, step, request).await,
            SeedRequest::Warehouse(request) => {
                self.ensure_warehouse(actor, step, request, handler_user_id).await
            },
            SeedRequest::Customer(request) => self.ensure_customer(actor, step, request, notices).await,
            SeedRequest::Supplier(request) => {
                self.ensure_supplier(actor, step, request, company_party_id, notices).await
            },
            SeedRequest::Product(request) => self.ensure_product(actor, step, records, request).await,
        }
    }

    /// 按稳定种子键索引实际 ID 清单。
    async fn record_map(&self) -> Result<HashMap<String, DemoMasterRecord>> {
        let records = record::load_all(&self.db).await?;
        Ok(records.into_iter().map(|record| (record.key.clone(), record)).collect())
    }

    /// 复用公司主体，缺失时读取基础 JSON 创建。
    async fn company_party_id(
        &self,
        actor: &AuditActor,
        notices: &mut Vec<String>,
    ) -> Result<Option<String>> {
        if let Some(id) = self.active_company_id(actor).await? {
            return Ok(Some(id));
        }
        match self.ports.party.create_company(super::spec::foundation_spec().company.clone(), actor).await {
            Ok(company) => Ok(Some(company.id)),
            Err(erp_party::Error::ConflictError(message) | erp_party::Error::ValidationError(message)) => {
                push_notice(notices, format!("未生成供应商：{message}"));
                Ok(None)
            },
            Err(error) => Err(error.into()),
        }
    }

    /// 读取已有启用公司主体。
    async fn active_company_id(&self, actor: &AuditActor) -> Result<Option<String>> {
        let page = self
            .ports
            .party
            .company_list(
                actor,
                &erp_party::dto::company::CompanyListParams {
                    keyword: None,
                    status: Some(PartyStatus::Active),
                    page: Some(1),
                    page_size: Some(1),
                },
            )
            .await?;
        Ok(page.items.first().map(|company| company.id.clone()))
    }

    /// 解析基础 JSON 指定的仓储岗位账号。
    async fn warehouse_handler_id(&self) -> Result<Option<String>> {
        let login = super::spec::foundation_spec().warehouse_handler_account.as_str();
        let account = self.db.accounts().find_by_account(login, &mut ReadExecutor::default()).await?;
        Ok(account.map(|account| account.base.id))
    }
}

/// 状态资格和完整清单统计使用公开入口提供的同一事务快照。
async fn status_snapshot(
    db: &mongodb::Database,
    authorization: &AuthorizationService,
    enabled: bool,
    actor: &AuditActor,
    executor: &mut dyn Executor,
) -> Result<DemoStatus> {
    authorization.require(actor, &demo_master_data::STATUS, AccessTarget::Operation, executor).await?;
    let records = record::load(db, executor).await?;
    let rows = records
        .iter()
        .filter_map(|record| record.kind().map(|kind| (kind, record.removed)))
        .collect::<Vec<_>>();
    let (active, removed) = plan::count_records(&rows);
    let view = DemoStatus { enabled, planned: plan::planned_counts(&plan::demo_steps()?), active, removed };
    Ok(view)
}

/// 累计本批主数据创建或恢复结果。
fn tally(report: &mut DemoChunkReport, outcome: EnsureOutcome) -> Result<()> {
    match outcome {
        EnsureOutcome::Created => report.created += 1,
        EnsureOutcome::Restored => report.restored += 1,
        EnsureOutcome::Skipped => report.skipped += 1,
        EnsureOutcome::Notice(text) => return Err(Error::ValidationError(text)),
    }
    if !report.notices.is_empty() {
        return Err(Error::ValidationError(report.notices.join("；")));
    }
    Ok(())
}

/// 去重合并生成提示。
fn push_notice(notices: &mut Vec<String>, text: String) {
    if !notices.iter().any(|item| item == &text) {
        notices.push(text);
    }
}

/// 初始化分批处理结果。
fn empty_report(next_cursor: usize, total: usize) -> DemoChunkReport {
    DemoChunkReport {
        next_cursor: next_cursor as u32,
        total_steps: total as u32,
        done: next_cursor == total,
        created: 0,
        restored: 0,
        skipped: 0,
        removed: 0,
        derived_removed: 0,
        notices: Vec::new(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn status_rejects_missing_authorization_before_reading_demo_records() {
        use erp_core::AccountKind;
        use erp_identity::testing::unavailable_authorization;
        use persistence_core::testing::SnapshotExecutor;

        let db = record::testing::database("demo_status_authorization");
        let actor = AuditActor::new("reader".into(), "reader".into(), AccountKind::Admin);
        let result = status_snapshot(
            &db,
            &unavailable_authorization(),
            true,
            &actor,
            &mut SnapshotExecutor::default(),
        )
        .await;
        assert!(matches!(result, Err(Error::Internal(message)) if message == "测试授权设施不可用"));
    }

    #[test]
    fn prerequisites_and_owner_handover_failures_cannot_report_completion() {
        let mut report = empty_report(8, 8);
        assert!(tally(&mut report, EnsureOutcome::Notice("供应商无创建权限".into())).is_err());
        report.notices.push("客户负责人移交失败".into());
        assert!(tally(&mut report, EnsureOutcome::Skipped).is_err());
        report.notices.clear();
        tally(&mut report, EnsureOutcome::Created).unwrap();
        assert_eq!(report.created, 1);
    }
}
