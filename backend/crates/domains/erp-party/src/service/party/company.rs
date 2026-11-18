//! 公司维护用例；所有写入复用主体名称修订事务。
use super::*;
use crate::dto::company::{CompanyListParams, CompanyView, SaveCompanyRequest};
use crate::repository::prelude::*;

impl PartyService {
    /// 分页查询我方公司，直接返回可选择的完整名称。
    ///
    /// 响应中的页码与每页条数按请求值钳制：页码缺省 1 且不超过 `1_000_000`，每页缺省 30 且不超过 100。
    ///
    /// # 参数
    /// * `params` - 关键词、状态与分页条件。
    ///
    /// # 返回
    /// 返回公司视图分页。
    ///
    /// # 错误
    /// 仓储查询失败时返回对应错误。主体缺少 `company_profile` 时，`TryFrom` 返回 `NotFound`。
    pub async fn company_list(
        &self,
        actor: &AuditActor,
        params: &CompanyListParams,
    ) -> Result<PageView<CompanyView>> {
        let service = self.clone();
        let actor = actor.clone();
        let params = params.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.company_list_snapshot(&actor, &params, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn company_list_snapshot(
        &self,
        actor: &AuditActor,
        params: &CompanyListParams,
        executor: &mut dyn Executor,
    ) -> Result<PageView<CompanyView>> {
        self.authorization
            .require(actor, &party::COMPANY_COMPANY_LIST, AccessTarget::Operation, executor)
            .await?;
        let page = self.db.parties().companies(params, executor).await?;
        Ok(PageView {
            items: page.items.into_iter().map(CompanyView::try_from).collect::<Result<_>>()?,
            total: page.total,
            page: params.page.unwrap_or(1).clamp(1, 1_000_000),
            page_size: params.page_size.unwrap_or(30).clamp(1, 100),
        })
    }

    /// 回读公司，包括停用公司供历史资料回显。
    ///
    /// # 参数
    /// * `id` - 主体 ID。
    ///
    /// # 返回
    /// 返回公司视图。
    ///
    /// # 错误
    /// 主体不存在时返回 `NotFound`；主体没有 `company_profile` 时 `TryFrom` 返回 `NotFound`；查询失败时返回仓储错误。
    pub async fn company_detail(&self, actor: &AuditActor, id: &str) -> Result<CompanyView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move { service.company_detail_snapshot(&actor, &id, executor).await })
            })
            .await
    }

    /// 在同一授权快照读取业务数据；执行器由公开入口提供。
    async fn company_detail_snapshot(
        &self,
        actor: &AuditActor,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompanyView> {
        self.authorization
            .require(actor, &party::COMPANY_COMPANY_DETAIL, AccessTarget::Operation, executor)
            .await?;
        self.db
            .parties()
            .find_party(&PartyId::new(id), executor)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".to_string()))?
            .try_into()
    }

    /// 创建公司；稳定编号支持结果未知后的同内容回读。
    ///
    /// 编号已存在、未删除，且公司资料、统一社会信用代码与状态都相同时，直接返回既有公司。
    ///
    /// # 参数
    /// * `req` - 公司资料；`party_no` 创建后不可复用为另一份内容。
    /// * `actor` - 已鉴权操作人。
    ///
    /// # 返回
    /// 返回新建或可安全回读的公司视图。
    ///
    /// # 错误
    /// 名称或别名校验失败时返回 `ValidationError`。编号已占用且内容不一致、已删除或不是同一公司时返回 `ConflictError`。
    /// 创建事务、唯一索引冲突或随后回读失败时返回对应错误。
    pub async fn create_company(&self, req: SaveCompanyRequest, actor: &AuditActor) -> Result<CompanyView> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.create_company_record(req, &actor, executor).await })
            })
            .await
    }

    /// 使用入口事务完成公司编号回放、主体写入与最终回读。
    async fn create_company_record(
        &self,
        req: SaveCompanyRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CompanyView> {
        let mut profile = req.profile()?;
        if let Some(existing) = self
            .company_lookup(
                actor,
                &party::COMPANY_COMPANY_CREATE,
                &CompanyLookup::Number(req.party_no.clone()),
                executor,
            )
            .await?
        {
            profile.bank_name = existing.company_profile.as_ref().and_then(|p| p.bank_name.clone());
            if existing.company_profile.as_ref() == Some(&profile)
                && existing.unified_credit_code == req.unified_credit_code
                && existing.stable.status == req.status
                && existing.base.deleted_at == 0
            {
                return existing.try_into();
            }
            return Err(Error::ConflictError("公司编号已被使用，请刷新后重试".into()));
        }
        let command = CreatePartyRequest {
            party_no: req.party_no,
            party_kind: Some(PartyKind::Enterprise),
            unified_credit_code: req.unified_credit_code,
            legal_name: profile.legal_name.clone(),
            short_name: profile.short_name.clone(),
            change_reason: "新建公司主体".into(),
            status: Some(req.status),
        };
        let result = self
            .create_party_record(command, Some(profile), actor, &party::COMPANY_COMPANY_CREATE, executor)
            .await?;
        self.company_result(actor, &party::COMPANY_COMPANY_CREATE, &result.id, executor).await
    }

    /// 更新公司资料或启停状态，旧引用继续保留同一主体身份。
    ///
    /// 期望版本低于当前版本，且名称、别名、统一社会信用代码与状态都未变化时，直接返回当前公司，不再写入。
    ///
    /// # 参数
    /// * `id` - 公司主体 ID。
    /// * `req` - 整份公司资料；更新必须携带 `version`。
    /// * `actor` - 已鉴权操作人。
    ///
    /// # 返回
    /// 返回更新后或可安全回读的公司视图。
    ///
    /// # 错误
    /// 主体不存在或不是公司时返回 `NotFound`。编号被修改、资料校验失败或缺少版本时返回 `ValidationError`。
    /// 更新事务或随后回读失败时返回对应错误。
    pub async fn update_company(
        &self,
        id: &str,
        req: SaveCompanyRequest,
        actor: &AuditActor,
    ) -> Result<CompanyView> {
        let service = self.clone();
        let actor = actor.clone();
        let id = id.to_owned();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.update_company_record(&id, req, &actor, executor).await })
            })
            .await
    }

    /// 使用入口事务完成公司版本回放、名称修订与最终回读。
    async fn update_company_record(
        &self,
        id: &str,
        req: SaveCompanyRequest,
        actor: &AuditActor,
        executor: &mut dyn Executor,
    ) -> Result<CompanyView> {
        let existing = self.company_result(actor, &party::COMPANY_COMPANY_UPDATE, id, executor).await?;
        if req.party_no != existing.party_no {
            return Err(Error::ValidationError("公司编号不可修改".into()));
        }
        let mut profile = req.profile()?;
        profile.bank_name = existing.bank_name.clone();
        if req.version.is_some_and(|version| version < existing.version)
            && profile.legal_name == existing.legal_name
            && profile.short_name == existing.short_name
            && profile.aliases == existing.aliases
            && req.unified_credit_code == existing.unified_credit_code
            && req.status == existing.status
        {
            return Ok(existing);
        }
        let command = UpdatePartyRequest {
            version: req
                .version
                .ok_or_else(|| Error::ValidationError("缺少公司版本，请刷新后重试".into()))?,
            status: Some(req.status),
            unified_credit_code: Some(req.unified_credit_code.unwrap_or_default()),
            legal_name: profile.legal_name.clone(),
            short_name: profile.short_name.clone(),
            change_reason: "维护公司主体".into(),
        };
        self.update_stage(id, command, Some(&profile), actor, &party::COMPANY_COMPANY_UPDATE, executor)
            .await?;
        self.company_result(actor, &party::COMPANY_COMPANY_UPDATE, id, executor).await
    }
}

impl PartyService {
    /// 以原写入口的资格读取完整公司回包，不额外要求详情权限。
    async fn company_result(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        id: &str,
        executor: &mut dyn Executor,
    ) -> Result<CompanyView> {
        self.company_lookup(actor, permission, &CompanyLookup::Id(id.into()), executor)
            .await?
            .ok_or_else(|| Error::NotFound("主体不存在".into()))?
            .try_into()
    }

    /// 在公开入口提供的原快照内检查资格，再读取完整公司。
    async fn company_lookup(
        &self,
        actor: &AuditActor,
        permission: &Permission,
        lookup: &CompanyLookup,
        executor: &mut dyn Executor,
    ) -> Result<Option<Party>> {
        if !executor.boundary().is_some_and(|boundary| boundary.is_snapshot()) {
            return Err(Error::ValidationError("公司回读必须使用快照事务".into()));
        }
        self.authorization.require(actor, permission, AccessTarget::Operation, executor).await?;
        Ok(match lookup {
            CompanyLookup::Id(id) => self.db.parties().find_party(&PartyId::new(id), executor).await?,
            CompanyLookup::Number(number) => {
                self.db.parties().find_by_party_no_including_deleted(number, executor).await?
            },
        })
    }
}

/// 公司身份和稳定编号回读均读取完整主体，编号回读保留已删除占用判断。
enum CompanyLookup {
    Id(String),
    Number(String),
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use erp_identity::testing::unavailable_authorization;
    use persistence_core::ReadExecutor;
    use persistence_core::testing::SnapshotExecutor;

    use super::*;
    use crate::ports::{FailClosedAuditPort, FailClosedSupplierRolePort};

    /// 只构造惰性数据库句柄，授权失败不得尝试公司读取。
    async fn service() -> PartyService {
        let db = mongodb::Client::with_uri_str("mongodb://127.0.0.1:1/?serverSelectionTimeoutMS=1")
            .await
            .unwrap()
            .database("company_authorization");
        PartyService::new(
            db,
            Arc::new(FailClosedAuditPort),
            Arc::new(FailClosedSupplierRolePort),
            unavailable_authorization(),
        )
    }

    fn request() -> SaveCompanyRequest {
        SaveCompanyRequest {
            party_no: "company-number".into(),
            version: Some(1),
            legal_name: "公司全称".into(),
            short_name: None,
            aliases: Vec::new(),
            unified_credit_code: None,
            status: PartyStatus::Active,
        }
    }

    #[tokio::test]
    async fn company_commands_use_the_callers_snapshot_before_database_access() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let create = service
            .create_company_record(request(), &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("创建应在调用方快照中执行授权");
        let update = service
            .update_company_record("company", request(), &actor, &mut SnapshotExecutor::default())
            .await
            .expect_err("更新应在调用方快照中执行授权");
        for error in [create, update] {
            assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
        }
    }

    #[tokio::test]
    async fn company_commands_reject_non_snapshot_executors_without_starting_transactions() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let create = service
            .create_company_record(request(), &actor, &mut ReadExecutor::default())
            .await
            .expect_err("创建不得在下层补开事务");
        let update = service
            .update_company_record("company", request(), &actor, &mut ReadExecutor::default())
            .await
            .expect_err("更新不得在下层补开事务");
        for error in [create, update] {
            assert!(
                matches!(error, Error::ValidationError(message) if message == "公司回读必须使用快照事务")
            );
        }
    }

    #[tokio::test]
    async fn replay_and_final_result_reads_require_the_original_write_qualification() {
        let service = service().await;
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        for permission in [party::COMPANY_COMPANY_CREATE, party::COMPANY_COMPANY_UPDATE] {
            for lookup in [CompanyLookup::Number("stable-number".into()), CompanyLookup::Id("company".into())]
            {
                let error = service
                    .company_lookup(&actor, &permission, &lookup, &mut SnapshotExecutor::default())
                    .await
                    .expect_err("授权失败不能读取公司");
                assert!(matches!(error, Error::Internal(message) if message == "测试授权设施不可用"));
            }
        }
    }

    #[tokio::test]
    async fn replay_read_rejects_non_snapshot_executors() {
        let actor = AuditActor::new("actor".into(), "actor".into(), AccountKind::Admin);
        let error = service()
            .await
            .company_lookup(
                &actor,
                &party::COMPANY_COMPANY_CREATE,
                &CompanyLookup::Number("stable-number".into()),
                &mut ReadExecutor::default(),
            )
            .await
            .expect_err("事务外读取必须拒绝");
        assert!(matches!(error, Error::ValidationError(message) if message == "公司回读必须使用快照事务"));
    }
}
