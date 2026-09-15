//! 公司主体维护和选择接口合同。
use serde::{Deserialize, Serialize};

use crate::entity::party::company::CompanyProfile;
use crate::{Party, PartyStatus, Result};

/// 公司创建或整份资料更新；更新必须携带版本。
#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct SaveCompanyRequest {
    pub party_no: String,
    pub version: Option<u64>,
    pub legal_name: String,
    pub short_name: Option<String>,
    #[serde(default)]
    pub aliases: Vec<String>,
    pub unified_credit_code: Option<String>,
    pub status: PartyStatus,
}

impl SaveCompanyRequest {
    /// 生成经过校验的公司角色内容。
    ///
    /// # Errors
    /// 非法名称或别名返回校验失败。
    pub fn profile(&self) -> Result<CompanyProfile> {
        CompanyProfile::new(self.legal_name.clone(), self.short_name.clone(), self.aliases.clone())
            .map_err(|e| crate::Error::ValidationError(e.to_string()))
    }
}

/// 分页查询公司，可包含停用公司用于维护。
#[derive(Debug, Clone, Default, Deserialize)]
pub struct CompanyListParams {
    pub keyword: Option<String>,
    pub status: Option<PartyStatus>,
    pub page: Option<u64>,
    pub page_size: Option<u32>,
}

/// 公司选择及维护视图，不包含敏感账户信息。
#[derive(Debug, Clone, Serialize)]
pub struct CompanyView {
    pub id: String,
    pub party_no: String,
    pub version: u64,
    pub status: PartyStatus,
    pub unified_credit_code: Option<String>,
    pub legal_name: String,
    pub short_name: Option<String>,
    pub aliases: Vec<String>,
}

impl TryFrom<Party> for CompanyView {
    type Error = crate::Error;
    fn try_from(party: Party) -> Result<Self> {
        let company = party.company_profile.ok_or_else(|| crate::Error::NotFound("公司主体不存在".into()))?;
        Ok(Self {
            id: party.base.id,
            party_no: party.party_no,
            version: party.base.version,
            status: party.stable.status,
            unified_credit_code: party.unified_credit_code,
            legal_name: company.legal_name,
            short_name: company.short_name,
            aliases: company.aliases,
        })
    }
}
