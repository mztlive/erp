//! 按固定清单分批生成或删除演示主数据。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::CatalogService;
use erp_identity::{AccessControlExt, AccountCoreRepositoryExt, SharedRbacService};
use erp_party::{PartyStatus, SensitiveDataCodec};
use erp_warehouse::WarehouseService;
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{self, DemoCounts, DemoKind, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle};
use crate::adapters::{party_service, scoped_catalog_service, warehouse_service};
use crate::{CustomerProfileService, Error, Result, SupplierProfileService};

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
    /// 一并删除的衍生单据条数。
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
    /// 已从列表删除、仍可恢复的条数。
    pub removed: DemoCounts,
}

impl DemoMasterDataService {
    /// 创建演示主数据服务。
    ///
    /// # 参数
    /// * `db` - 目标数据库
    /// * `sensitive` - 客户和供应商资料使用的密文编解码器
    /// * `rbac` - 当前授权快照
    /// * `enabled` - 配置是否允许写入
    ///
    /// # 返回
    /// 返回未执行任何写入的服务。
    pub fn new(
        db: mongodb::Database,
        sensitive: Arc<SensitiveDataCodec>,
        rbac: SharedRbacService,
        enabled: bool,
    ) -> Self {
        Self { db, sensitive, rbac, enabled }
    }

    /// 返回各类演示主数据的计划数量和当前数量。
    ///
    /// # 返回
    /// 环境关闭时仍返回数量，`enabled` 为 false。
    ///
    /// # 错误
    /// 清单查询失败时返回错误。
    pub async fn status(&self) -> Result<DemoStatus> {
        let records = record::load_all(&self.db).await?;
        let rows = records
            .iter()
            .filter_map(|record| record.kind().map(|kind| (kind, record.removed)))
            .collect::<Vec<_>>();
        let (active, removed) = plan::count_records(&rows);
        Ok(DemoStatus { enabled: self.enabled, planned: plan::planned_counts(), active, removed })
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
        let steps = plan::demo_steps();
        let range = plan::apply_window(cursor as usize, steps.len());
        let records = self.record_map().await?;
        let company_party_id = self.company_party_id().await?;
        let handler_user_id = self.warehouse_handler_id().await?;
        let mut report = empty_report(range.end, steps.len());
        for step in &steps[range] {
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
            tally(&mut report, outcome);
        }
        Ok(report)
    }

    /// 删除下一批仍在列表中的演示主数据。
    ///
    /// # 参数
    /// * `actor` - 当前操作人
    /// * `purge` - 为 true 时先按外键图删除衍生单据。一次删除只在第一批传入 true。
    ///
    /// # 返回
    /// 返回本批删除条数。没有可删除记录时 `done` 为 true。
    ///
    /// # 错误
    /// 环境未开放或删除失败时返回错误。
    pub async fn remove_chunk(&self, actor: &AuditActor, purge: bool) -> Result<DemoChunkReport> {
        self.ensure_enabled()?;
        let steps = plan::demo_steps();
        let records = record::load_all(&self.db).await?;
        let active_keys = records
            .iter()
            .filter(|record| !record.removed)
            .map(|record| record.key.clone())
            .collect::<Vec<_>>();
        let derived_removed = if purge {
            super::derived::purge_derived(&self.db, &super::derived::master_ids(&records)).await?
        } else {
            0
        };
        let batch = plan::removal_batch(&steps, &active_keys, plan::CHUNK_LEN);
        let by_key =
            records.iter().map(|record| (record.key.clone(), record.clone())).collect::<HashMap<_, _>>();
        for key in &batch {
            let Some(record) = by_key.get(key) else {
                continue;
            };
            self.delete_record(actor, record).await?;
            let mut removed = record.clone();
            removed.removed = true;
            record::save(&self.db, &removed).await?;
        }
        let mut report = empty_report(0, steps.len());
        report.removed = batch.len() as u32;
        report.derived_removed = u32::try_from(derived_removed)
            .map_err(|_| Error::Internal("衍生单据数量超出计数范围".to_string()))?;
        report.done = active_keys.len() == batch.len();
        Ok(report)
    }

    pub(super) fn ensure_enabled(&self) -> Result<()> {
        if self.enabled {
            Ok(())
        } else {
            Err(Error::Forbidden("当前环境未开放演示主数据".to_string()))
        }
    }

    async fn create_record(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        records: &HashMap<String, DemoMasterRecord>,
        company_party_id: Option<&str>,
        handler_user_id: Option<&str>,
        notices: &mut Vec<String>,
    ) -> Result<EnsureOutcome> {
        match step.kind {
            DemoKind::Unit => self.ensure_unit(actor, step).await,
            DemoKind::Brand => self.ensure_brand(actor, step).await,
            DemoKind::Category => self.ensure_category(actor, step).await,
            DemoKind::Warehouse => self.ensure_warehouse(actor, step, handler_user_id).await,
            DemoKind::Customer => self.ensure_customer(actor, step, notices).await,
            DemoKind::Supplier => self.ensure_supplier(actor, step, company_party_id, notices).await,
            DemoKind::Product => self.ensure_product(actor, step, records).await,
        }
    }

    async fn delete_record(&self, actor: &AuditActor, record: &DemoMasterRecord) -> Result<()> {
        let Some(kind) = record.kind() else {
            return Ok(());
        };
        match kind {
            DemoKind::Unit => {
                ignore_missing(self.catalog().unit_of_measure_delete(&record.entity_id, actor).await)?
            },
            DemoKind::Brand => {
                ignore_missing(self.catalog().product_brand_delete(&record.entity_id, actor).await)?
            },
            DemoKind::Category => {
                ignore_missing(self.catalog().product_category_delete(&record.entity_id, actor).await)?
            },
            DemoKind::Warehouse => lifecycle::delete_warehouse(&self.db, actor, &record.entity_id).await?,
            DemoKind::Customer => self.delete_customer(actor, record).await?,
            DemoKind::Supplier => self.delete_supplier(actor, record).await?,
            DemoKind::Product => {
                lifecycle::delete_product_graph(&self.db, actor, &record.entity_id, &record.related_ids)
                    .await?
            },
        }
        Ok(())
    }

    async fn delete_customer(&self, actor: &AuditActor, record: &DemoMasterRecord) -> Result<()> {
        match crate::delete_customer(
            self.db.clone(),
            self.rbac.clone(),
            record.entity_id.clone(),
            actor.clone(),
        )
        .await
        {
            Ok(()) | Err(Error::NotFound(_)) => {},
            Err(error) => return Err(error),
        }
        if let Some(party_id) = record.related_ids.first() {
            match crate::delete_party(self.db.clone(), party_id.clone(), actor.clone()).await {
                Ok(()) | Err(Error::NotFound(_)) => {},
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn delete_supplier(&self, actor: &AuditActor, record: &DemoMasterRecord) -> Result<()> {
        match crate::delete_supplier(self.db.clone(), record.entity_id.clone(), actor.clone()).await {
            Ok(()) | Err(Error::NotFound(_)) => {},
            Err(error) => return Err(error),
        }
        if let Some(party_id) = record.related_ids.first() {
            match crate::delete_party(self.db.clone(), party_id.clone(), actor.clone()).await {
                Ok(()) | Err(Error::NotFound(_)) => {},
                Err(error) => return Err(error),
            }
        }
        Ok(())
    }

    async fn record_map(&self) -> Result<HashMap<String, DemoMasterRecord>> {
        let records = record::load_all(&self.db).await?;
        Ok(records.into_iter().map(|record| (record.key.clone(), record)).collect())
    }

    async fn company_party_id(&self) -> Result<Option<String>> {
        let page = party_service(self.db.clone())
            .company_list(&erp_party::dto::company::CompanyListParams {
                keyword: None,
                status: Some(PartyStatus::Active),
                page: Some(1),
                page_size: Some(1),
            })
            .await?;
        Ok(page.items.first().map(|company| company.id.clone()))
    }

    async fn warehouse_handler_id(&self) -> Result<Option<String>> {
        let login = super::spec::foundation_spec().warehouse_handler_account.as_str();
        let account = self.db.accounts().find_by_account(login, &mut NoTransaction).await?;
        Ok(account.map(|account| account.base.id))
    }

    pub(super) fn catalog(&self) -> CatalogService {
        scoped_catalog_service(self.db.clone(), self.rbac.clone())
    }

    pub(super) fn warehouses(&self) -> WarehouseService {
        warehouse_service(self.db.clone(), self.rbac.clone())
    }

    pub(super) fn customers(&self) -> CustomerProfileService {
        CustomerProfileService::new(self.db.clone(), Arc::clone(&self.sensitive)).with_rbac(self.rbac.clone())
    }

    pub(super) fn suppliers(&self) -> SupplierProfileService {
        SupplierProfileService::new(self.db.clone(), Arc::clone(&self.sensitive)).with_rbac(self.rbac.clone())
    }
}

fn tally(report: &mut DemoChunkReport, outcome: EnsureOutcome) {
    match outcome {
        EnsureOutcome::Created => report.created += 1,
        EnsureOutcome::Restored => report.restored += 1,
        EnsureOutcome::Skipped => report.skipped += 1,
        EnsureOutcome::Notice(text) => push_notice(&mut report.notices, text),
    }
}

fn push_notice(notices: &mut Vec<String>, text: String) {
    if !notices.iter().any(|item| item == &text) {
        notices.push(text);
    }
}

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

fn ignore_missing(result: erp_catalog::Result<()>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(erp_catalog::Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
