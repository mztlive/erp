//! 公司维护用例；所有写入复用主体名称修订事务。
use super::*;
use crate::dto::company::{CompanyListParams, CompanyView, SaveCompanyRequest};

impl PartyService {
    /// 分页查询我方公司，直接返回可选择的完整名称。
    ///
    /// # Errors
    /// 查询失败或持久化公司角色损坏时返回错误。
    pub async fn company_list(&self, params: &CompanyListParams) -> Result<PageView<CompanyView>> {
        let page = self.db.parties().companies(params, &mut NoTransaction).await?;
        Ok(PageView {
            items: page
                .items
                .into_iter()
                .map(CompanyView::try_from)
                .collect::<Result<_>>()?,
            total: page.total,
            page: params.page.unwrap_or(1).clamp(1, 1_000_000),
            page_size: params.page_size.unwrap_or(30).clamp(1, 100),
        })
    }

    /// 回读公司，包括停用公司供历史资料回显。
    ///
    /// # Errors
    /// 主体不存在、不是我方公司或查询失败时返回错误。
    pub async fn company_detail(&self, id: &str) -> Result<CompanyView> {
        self.load_party(id).await?.try_into()
    }

    /// 创建公司；稳定编号支持结果未知后的同内容回读。
    ///
    /// # Errors
    /// 编号占用、名称别名冲突、校验或事务失败时返回错误。
    pub async fn create_company(&self, req: SaveCompanyRequest, actor: &AuditActor) -> Result<CompanyView> {
        let profile = req.profile()?;
        if let Some(existing) = self
            .db
            .parties()
            .find_by_party_no_including_deleted(&req.party_no, &mut NoTransaction)
            .await?
        {
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
        let result = self.create_party_record(command, Some(profile), actor).await?;
        self.company_detail(&result.id).await
    }

    /// 更新公司资料或启停状态，旧引用继续保留同一主体身份。
    ///
    /// # Errors
    /// 缺少版本、编号变更、非公司身份或事务失败时返回错误。
    pub async fn update_company(
        &self,
        id: &str,
        req: SaveCompanyRequest,
        actor: &AuditActor,
    ) -> Result<CompanyView> {
        let existing = self.company_detail(id).await?;
        if req.party_no != existing.party_no {
            return Err(Error::ValidationError("公司编号不可修改".into()));
        }
        let profile = req.profile()?;
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
        self.update_party_record(id, command, Some(profile), actor)
            .await?;
        self.company_detail(id).await
    }
}
