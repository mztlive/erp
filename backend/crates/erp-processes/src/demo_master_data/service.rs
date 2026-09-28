//! 按固定清单分批生成或删除演示主数据。

use std::collections::HashMap;
use std::sync::Arc;

use application_core::AuditActor;
use erp_catalog::{CatalogExt, CatalogService};
use erp_identity::{AccessControlExt, AccountCoreRepositoryExt, SharedRbacService};
use erp_party::{PartyStatus, SensitiveDataCodec};
use erp_warehouse::WarehouseService;
use persistence_core::NoTransaction;

use super::ensure_dictionary::EnsureOutcome;
use super::plan::{self, DemoCounts, DemoKind, DemoStep};
use super::record::{self, DemoMasterRecord};
use super::seed::SeedRequest;
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
        Ok(DemoStatus {
            enabled: self.enabled,
            planned: plan::planned_counts(&plan::demo_steps()?),
            active,
            removed,
        })
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
        let steps = plan::demo_steps()?;
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
            tally(&mut report, outcome);
        }
        Ok(report)
    }

    /// 删除下一批仍在列表中的演示主数据。
    ///
    /// # 参数
    /// * `actor` - 当前操作人
    ///
    /// # 返回
    /// 返回本批删除条数。没有可删除记录时 `done` 为 true。
    ///
    /// # 错误
    /// 环境未开放或删除失败时返回错误。
    pub async fn remove_chunk(&self, actor: &AuditActor) -> Result<DemoChunkReport> {
        self.ensure_enabled()?;
        let steps = plan::demo_steps()?;
        let mut records = record::load_all(&self.db).await?;
        self.refresh_related_ids(&mut records).await?;
        let active_keys = records
            .iter()
            .filter(|record| !record.removed)
            .map(|record| record.key.clone())
            .collect::<Vec<_>>();
        let derived_removed =
            super::derived::purge_derived(&self.db, super::derived::master_ids(&records), actor).await?;
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

    /// 按登记的种类和实际 ID 删除主数据。
    async fn delete_record(&self, actor: &AuditActor, record: &DemoMasterRecord) -> Result<()> {
        let kind = record.kind().ok_or_else(|| Error::Internal("演示清单包含未知种类".into()))?;
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

    /// 按清单 ID 删除客户角色及关联主体。
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

    /// 按清单 ID 删除供应商角色及关联主体。
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

    /// 删除前读回商品当前及历史 SKU，并持久化清理根供重试使用。
    async fn refresh_related_ids(&self, records: &mut [DemoMasterRecord]) -> Result<()> {
        for record in records {
            if record.kind().is_none()
                || record.entity_id.trim().is_empty()
                || record.related_ids.iter().any(|id| id.trim().is_empty())
            {
                return Err(Error::Internal("演示清单包含未知种类".into()));
            }
            if record.kind() != Some(DemoKind::Product) {
                continue;
            }
            let skus = self
                .db
                .skus()
                .find_many_by_field_including_deleted(
                    "product_id",
                    record.entity_id.clone(),
                    &mut NoTransaction,
                )
                .await?;
            for sku in skus {
                if !record.related_ids.contains(&sku.base.id) {
                    record.related_ids.push(sku.base.id);
                }
            }
            record::save(&self.db, record).await?;
        }
        Ok(())
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
        if let Some(id) = self.active_company_id().await? {
            return Ok(Some(id));
        }
        match party_service(self.db.clone())
            .create_company(super::spec::foundation_spec().company.clone(), actor)
            .await
        {
            Ok(company) => Ok(Some(company.id)),
            Err(erp_party::Error::ConflictError(message) | erp_party::Error::ValidationError(message)) => {
                push_notice(notices, format!("未生成供应商：{message}"));
                Ok(None)
            },
            Err(error) => Err(error.into()),
        }
    }

    /// 读取已有启用公司主体。
    async fn active_company_id(&self) -> Result<Option<String>> {
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

    /// 解析基础 JSON 指定的仓储岗位账号。
    async fn warehouse_handler_id(&self) -> Result<Option<String>> {
        let login = super::spec::foundation_spec().warehouse_handler_account.as_str();
        let account = self.db.accounts().find_by_account(login, &mut NoTransaction).await?;
        Ok(account.map(|account| account.base.id))
    }

    /// 装配带权限策略的商品服务。
    ///
    /// # 参数
    /// 无；复用当前数据库与权限服务。
    ///
    /// # 返回
    /// 返回商品领域服务。
    ///
    /// # 错误
    /// 无。
    pub(super) fn catalog(&self) -> CatalogService {
        scoped_catalog_service(self.db.clone(), self.rbac.clone())
    }

    /// 装配带权限策略的仓库服务。
    ///
    /// # 参数
    /// 无；复用当前数据库与权限服务。
    ///
    /// # 返回
    /// 返回仓库领域服务。
    ///
    /// # 错误
    /// 无。
    pub(super) fn warehouses(&self) -> WarehouseService {
        warehouse_service(self.db.clone(), self.rbac.clone())
    }

    /// 装配客户资料写入流程。
    ///
    /// # 参数
    /// 无；复用当前数据库、密文编解码器与权限服务。
    ///
    /// # 返回
    /// 返回客户资料流程。
    ///
    /// # 错误
    /// 无。
    pub(super) fn customers(&self) -> CustomerProfileService {
        CustomerProfileService::new(self.db.clone(), Arc::clone(&self.sensitive)).with_rbac(self.rbac.clone())
    }

    /// 装配供应商资料写入流程。
    ///
    /// # 参数
    /// 无；复用当前数据库、密文编解码器与权限服务。
    ///
    /// # 返回
    /// 返回供应商资料流程。
    ///
    /// # 错误
    /// 无。
    pub(super) fn suppliers(&self) -> SupplierProfileService {
        SupplierProfileService::new(self.db.clone(), Arc::clone(&self.sensitive)).with_rbac(self.rbac.clone())
    }
}

/// 累计本批主数据创建或恢复结果。
fn tally(report: &mut DemoChunkReport, outcome: EnsureOutcome) {
    match outcome {
        EnsureOutcome::Created => report.created += 1,
        EnsureOutcome::Restored => report.restored += 1,
        EnsureOutcome::Skipped => report.skipped += 1,
        EnsureOutcome::Notice(text) => push_notice(&mut report.notices, text),
    }
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

/// 将已不存在的字典视为删除完成。
fn ignore_missing(result: erp_catalog::Result<()>) -> Result<()> {
    match result {
        Ok(()) => Ok(()),
        Err(erp_catalog::Error::NotFound(_)) => Ok(()),
        Err(error) => Err(error.into()),
    }
}
