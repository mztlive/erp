//! API 启动时在同一事务内同步我方主体、修订及从属资料。
mod facts;
mod identity;

use std::sync::Arc;

use application_core::AuditActor;
use erp_core::AccountKind;
use erp_core::common::time::BusinessDate;
use persistence_core::Transactional;

use super::{PartyService, SensitiveDataCodec};
use crate::Result;
use crate::entity::party::builtin::BuiltinCompany;

const ACTOR: &str = "system:company-bootstrap";

impl PartyService {
    /// 将内建我方公司资料同步到当前数据库；同内容重启不产生新版本。
    ///
    /// # 参数
    /// * `codec` - 当前应用使用的敏感资料编解码器。
    /// # 返回
    /// 返回本次发生资料变更的公司数量。
    /// # 错误
    /// 身份冲突、历史账户内容冲突、资料校验或事务失败时返回错误；整体回滚。
    pub async fn sync_builtin_companies(&self, codec: Arc<SensitiveDataCodec>) -> Result<usize> {
        let seeds = BuiltinCompany::all()?;
        let service = Self {
            db: self.db.clone(),
            audit: self.audit.clone(),
            supplier_roles: self.supplier_roles.clone(),
        };
        let actor = AuditActor::new(ACTOR.into(), ACTOR.into(), AccountKind::Admin);
        let today = BusinessDate::today();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let mut changed = 0;
                    for seed in seeds {
                        let (party, identity_changed) =
                            service.sync_company_identity(&seed, executor).await?;
                        let facts_changed =
                            service.sync_company_facts(&seed, &party, &codec, today, executor).await?;
                        if identity_changed || facts_changed {
                            let audit = service.audit.resource_log(
                                actor.clone(),
                                "company.bootstrap",
                                "party",
                                party.base.id.clone(),
                            )?;
                            service.audit.persist(&audit, executor).await?;
                            changed += 1;
                        }
                    }
                    Ok(changed)
                })
            })
            .await
    }
}
