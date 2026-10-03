//! 模板服务组合装配；公司事实读取沿用调用方事务执行器。

use std::sync::Arc;

use async_trait::async_trait;
use erp_contract::ports::{SigningCompanyFact, SigningCompanyPort};
use erp_contract::{ContractTemplateService, Result};
use erp_party::{PartyExt, PartyStatus};
use mongodb::Database;
use persistence_core::Executor;

use super::contract::MongoContractAudit;

struct MongoSigningCompanies(Database);

#[async_trait]
impl SigningCompanyPort for MongoSigningCompanies {
    async fn company(&self, id: &str, executor: &mut dyn Executor) -> Result<Option<SigningCompanyFact>> {
        let Some(party) = self.0.parties().find_by_id(id, executor).await? else {
            return Ok(None);
        };
        Ok(party.company_profile.map(|profile| SigningCompanyFact {
            name: profile.legal_name,
            active: party.stable.status == PartyStatus::Active,
        }))
    }
}

/// 装配主体事实和现有合同审计 Port。
/// # 参数
/// * `db` - 业务数据库。
/// # 返回
/// 完整接线的模板服务。
/// # 错误
/// 无。
pub fn contract_template_service(db: Database) -> ContractTemplateService {
    ContractTemplateService::new(
        db.clone(),
        MongoContractAudit::shared(db.clone()),
        Arc::new(MongoSigningCompanies(db)),
    )
}
