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
    pub async fn company_list(&self, params: &CompanyListParams) -> Result<PageView<CompanyView>> {
        let page = self.db.parties().companies(params, &mut NoTransaction).await?;
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
    pub async fn company_detail(&self, id: &str) -> Result<CompanyView> {
        self.load_party(id).await?.try_into()
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
        let profile = req.profile()?;
        if let Some(existing) =
            self.db.parties().find_by_party_no_including_deleted(&req.party_no, &mut NoTransaction).await?
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
        self.update_party_record(id, command, Some(profile), actor).await?;
        self.company_detail(id).await
    }
}
