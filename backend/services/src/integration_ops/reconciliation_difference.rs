//! 对账差异业务：创建、列表、详情、人工处理、终态解决。
//!
//! 差异本身不可变，以最新处理记录序号为并发令牌；处理记录只追加、不更新不删除；
//! 终结结论使用固定原因枚举 + 受控证据；所有业务写入与审计日志在同一 MongoDB
//! 事务原子提交（模板见 `super::transaction`）。

use database::{AccessControlExt, IntegrationOpsExt, NoTransaction};
use entities::common::time::Instant;
use entities::integration_ops::{
    ReconciliationDifference, ReconciliationDifferenceData, ReconciliationDifferenceId,
    ReconciliationDifferenceResolution, ReconciliationDifferenceResolutionData,
    ReconciliationDifferenceResolutionId, ResolutionAction, ResultingStatus,
};
use id_generator::next_id;
use validator::Validate;

use super::dto::SortDir;
use super::{
    CreateDifferenceRequest, DifferenceActionView, DifferenceConclusion, DifferenceDetailView,
    DifferenceFilter, DifferenceListParams, DifferenceProcessAction, DifferenceView, IntegrationOpsService,
    PageView, ProcessDifferenceRequest, ResolutionView, ResolveDifferenceRequest,
};
use crate::audit::AuditActor;
use crate::errors::{Error, Result};

impl IntegrationOpsService {
    /// 登记对账差异（正式差异事实，创建后不可修改）。
    ///
    /// 对象唯一键幂等由唯一索引保证，重复登记透出 409（对账任务不直接修改正式事实）。
    ///
    /// # 参数
    /// * `req` - 登记请求
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回新建差异的视图。
    ///
    /// # 错误
    /// * `ConflictError` - 同对象同分类差异已存在
    /// * `ValidationError` - 请求体校验失败或两侧证据都未提供
    pub async fn create_difference(
        &self,
        req: CreateDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceView> {
        req.validate()?;
        if req.left_fact_reference.is_none() && req.right_fact_reference.is_none() {
            return Err(Error::ValidationError(
                "差异必须至少提供一侧不可变证据引用".to_string(),
            ));
        }
        let difference = ReconciliationDifference::new(
            ReconciliationDifferenceId::new(next_id()),
            ReconciliationDifferenceData {
                business_object_type: req.business_object_type,
                business_object_id: req.business_object_id,
                difference_type: req.difference_type,
                left_fact_reference: req.left_fact_reference,
                right_fact_reference: req.right_fact_reference,
            },
        )?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.create",
            "reconciliation_difference",
            difference.base.id.clone(),
        )?;
        let stored = difference.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_differences().create(&stored, session).await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        Ok(difference.into())
    }

    /// 分页查询对账差异列表（`status` 由最新处理记录派生）。
    ///
    /// 每行派生状态按差异 ID 取最新处理记录（当前仓储无批量方法，页内逐行查询，
    /// 页大小 ≤ 100，走 `(reconciliation_difference_id, resolution_no)` 唯一索引）；
    /// 投影行类型按字段映射为响应视图（仓储私有子树不可命名）。
    ///
    /// # 参数
    /// * `params` - 查询参数
    ///
    /// # 返回
    /// 返回契约形状的分页视图。
    ///
    /// # 错误
    /// * `ValidationError` - 分页参数非法或排序字段不在白名单
    /// * `RepositoryError` - 数据库查询失败
    pub async fn difference_list(&self, params: &DifferenceListParams) -> Result<PageView<DifferenceView>> {
        params.validate()?;
        let query = params.normalized()?;
        let filter = DifferenceFilter {
            business_object_type: query.business_object_type,
            business_object_id: query.business_object_id,
            difference_type: query.difference_type,
            created_at_from: query.created_at_from,
            created_at_to: query.created_at_to,
            page: query.paging.page,
            page_size: query.paging.page_size,
            sort_by: Some(query.paging.sort_by.to_string()),
            sort_ascending: matches!(query.paging.sort_dir, SortDir::Asc),
        };
        let page = self
            .db
            .reconciliation_differences()
            .search_differences(&filter, &mut NoTransaction)
            .await?;
        let mut items = Vec::with_capacity(page.items.len());
        for row in page.items {
            let status = self.derived_difference_status(&row.id).await?;
            items.push(DifferenceView {
                id: row.id,
                business_object_type: row.business_object_type,
                business_object_id: row.business_object_id,
                difference_type: row.difference_type,
                left_fact_reference: row.left_fact_reference,
                right_fact_reference: row.right_fact_reference,
                status,
                version: row.version,
                created_at: row.created_at,
            });
        }

        Ok(PageView {
            items,
            total: page.total,
            page: filter.page,
            page_size: filter.page_size,
        })
    }

    /// 查询对账差异详情（含处理记录时间线）。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    ///
    /// # 返回
    /// 返回差异详情视图。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    pub async fn difference_detail(&self, id: &str) -> Result<DifferenceDetailView> {
        let difference = self
            .db
            .reconciliation_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("差异不存在".to_string()))?;
        let difference_id = ReconciliationDifferenceId::new(difference.base.id.clone());
        let status = self.derived_difference_status(&difference.base.id).await?;
        let history = self
            .db
            .reconciliation_difference_resolutions()
            .search_resolutions(&difference_id, &mut NoTransaction)
            .await?;
        let mut view: DifferenceView = difference.into();
        view.status = status;
        let resolutions = history
            .into_iter()
            .map(|row| ResolutionView {
                id: row.id,
                resolution_no: row.resolution_no,
                resolution_action: row.resolution_action,
                resulting_status: row.resulting_status,
                evidence_reference: row.evidence_reference,
                replacement_task_id: row.replacement_task_id.map(|id| id.to_string()),
                handled_by: row.handled_by,
                handled_at: row.handled_at.unix_secs(),
            })
            .collect();

        Ok(DifferenceDetailView {
            difference: view,
            resolutions,
        })
    }

    /// 人工处理对账差异（非终结动作，只追加处理记录）。
    ///
    /// 领取仅允许作为首条处理记录；处理中/补充证据追加处理记录并派生处理中状态。
    /// 处理记录不可更新或删除；差异已终结时拒绝继续处理。并发保护以处理记录
    /// 序号为乐观锁令牌：`version` 与最新序号不一致返回 409。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `req` - 处理请求（含期望的最新处理序号）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回追加后的处理记录视图与最新处理序号。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    /// * `ValidationError` - 领取动作在已有处理记录时被拒绝
    pub async fn process_difference(
        &self,
        id: &str,
        req: ProcessDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceActionView> {
        req.validate()?;
        let difference = self.load_active_difference(id, req.version).await?;
        let action = match req.action {
            DifferenceProcessAction::Claim => ResolutionAction::Claim,
            DifferenceProcessAction::Processing | DifferenceProcessAction::AddEvidence => {
                ResolutionAction::Processing
            }
        };
        let resolution = self
            .append_difference_resolution(&difference, action, req.evidence_reference, actor)
            .await?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.process",
            "reconciliation_difference",
            difference.base.id,
        )?;
        let stored = resolution.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_difference_resolutions()
                    .create(&stored, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        let resolution_no = resolution.resolution_no;
        Ok(DifferenceActionView {
            resolution: resolution.into(),
            version: u64::from(resolution_no),
        })
    }

    /// 解决对账差异（终态结论，只追加处理记录）。
    ///
    /// 结论必须是固定原因枚举（W29 §7，禁止自由文本）且提供受控证据；原因代码
    /// 与证据引用合并写入处理记录的证据引用。确认无误/确认有效差异均派生已解决，
    /// 差异已终结时拒绝再次解决。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `req` - 解决请求（含期望的最新处理序号）
    /// * `actor` - 已通过鉴权的审计操作人
    ///
    /// # 返回
    /// 返回追加的处理记录视图与最新处理序号。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    /// * `ValidationError` - 受控证据为空
    pub async fn resolve_difference(
        &self,
        id: &str,
        req: ResolveDifferenceRequest,
        actor: &AuditActor,
    ) -> Result<DifferenceActionView> {
        req.validate()?;
        let difference = self.load_active_difference(id, req.version).await?;
        let action = match req.conclusion {
            DifferenceConclusion::ConfirmNoError => ResolutionAction::Confirmed,
            DifferenceConclusion::ConfirmValidDifference => ResolutionAction::Resolved,
        };
        let evidence = format!(
            "reason_code={};{}",
            req.reason_code.as_str(),
            req.evidence_reference
        );
        let resolution = self
            .append_difference_resolution(&difference, action, Some(evidence), actor)
            .await?;
        let audit = actor.clone().resource_log(
            "reconciliation_difference.resolve",
            "reconciliation_difference",
            difference.base.id,
        )?;
        let stored = resolution.clone();
        self.run_audited(move |db, session| {
            Box::pin(async move {
                db.reconciliation_difference_resolutions()
                    .create(&stored, session)
                    .await?;
                db.audit_logs().create(&audit, session).await?;
                Ok(())
            })
        })
        .await?;

        let resolution_no = resolution.resolution_no;
        Ok(DifferenceActionView {
            resolution: resolution.into(),
            version: u64::from(resolution_no),
        })
    }
    // -----------------------------------------------------------------------
    // 私有辅助
    // -----------------------------------------------------------------------

    /// 按 ID 加载差异并校验期望处理序号与活跃状态。
    ///
    /// 差异本身不可变（锁版本永不变化），以最新处理记录序号为乐观锁令牌：
    /// `expected_version` 与最新序号不一致（含并发追加后序号前移）返回 409。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    /// * `expected_version` - 期望的最新处理序号（0 表示无处理记录）
    ///
    /// # 返回
    /// 返回未终结的差异实体。
    ///
    /// # 错误
    /// * `NotFound` - 差异不存在
    /// * `ConflictError` - 期望序号不一致或差异已终结
    async fn load_active_difference(
        &self,
        id: &str,
        expected_version: Option<u64>,
    ) -> Result<ReconciliationDifference> {
        let difference = self
            .db
            .reconciliation_differences()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("差异不存在".to_string()))?;
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(
                &ReconciliationDifferenceId::new(difference.base.id.clone()),
                &mut NoTransaction,
            )
            .await?;
        let latest_no = u64::from(latest.as_ref().map(|record| record.resolution_no).unwrap_or(0));
        if expected_version.unwrap_or(0) != latest_no {
            return Err(Error::ConflictError(
                "数据已被其他请求修改，请刷新后重试".to_string(),
            ));
        }
        if latest.is_some_and(|record| {
            matches!(
                record.resulting_status,
                ResultingStatus::Resolved | ResultingStatus::Closed
            )
        }) {
            return Err(Error::ConflictError("差异已终结，不允许再操作".to_string()));
        }
        Ok(difference)
    }

    /// 派生差异当前处理状态（由最后一条处理记录派生，§6.21）。
    ///
    /// # 参数
    /// * `id` - 差异 ID
    ///
    /// # 返回
    /// 返回派生状态；尚无处理记录时返回 `None`。
    ///
    /// # 错误
    /// 数据库查询失败时返回错误。
    async fn derived_difference_status(&self, id: &str) -> Result<Option<ResultingStatus>> {
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(
                &ReconciliationDifferenceId::new(id.to_string()),
                &mut NoTransaction,
            )
            .await?;
        Ok(latest.map(|resolution| resolution.resulting_status))
    }

    /// 追加一条差异处理记录（领取/处理中/终结动作）。
    ///
    /// 领取仅在无既有处理记录时允许；处理序号取最新序号 + 1（首条从 1 起）。
    ///
    /// # 参数
    /// * `difference` - 目标差异
    /// * `action` - 解决动作
    /// * `evidence_reference` - 终态证据引用（追加式）
    /// * `actor` - 处理人
    ///
    /// # 返回
    /// 返回构造完成的处理记录实体。
    ///
    /// # 错误
    /// * `ValidationError` - 领取动作在已有处理记录时被拒绝
    async fn append_difference_resolution(
        &self,
        difference: &ReconciliationDifference,
        action: ResolutionAction,
        evidence_reference: Option<String>,
        actor: &AuditActor,
    ) -> Result<ReconciliationDifferenceResolution> {
        let difference_id = ReconciliationDifferenceId::new(difference.base.id.clone());
        let latest = self
            .db
            .reconciliation_difference_resolutions()
            .find_latest_by_difference(&difference_id, &mut NoTransaction)
            .await?;
        if action == ResolutionAction::Claim && latest.is_some() {
            return Err(Error::ValidationError("领取仅允许作为首条处理记录".to_string()));
        }
        let resolution_no = latest.map(|record| record.resolution_no + 1).unwrap_or(1);
        Ok(ReconciliationDifferenceResolution::new(
            ReconciliationDifferenceResolutionId::new(next_id()),
            ReconciliationDifferenceResolutionData {
                reconciliation_difference_id: difference_id,
                resolution_no,
                resolution_action: action,
                resulting_status: action.derived_status(),
                evidence_reference,
                replacement_task_id: None,
                handled_by: actor.id().to_string(),
                handled_at: Instant::now(),
            },
        )?)
    }
}
