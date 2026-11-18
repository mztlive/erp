//! 流水起点和只向前的版本校准。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_identity::AccessTarget;
use erp_permissions::contract;
use persistence_core::Transactional;

use super::ContractTemplateService;
use crate::dto::template::{ConfigureCounterRequest, CounterView};
use crate::entity::template::{ContractCounter, NumberGroup};
use crate::error::{Error, Result};
use crate::repository::templates::ContractTemplateExt;

impl ContractTemplateService {
    /// 返回当前业务年度四组流水；未落库时返回用户确认的历史起点。
    /// 四组当前已用流水和可选版本。
    /// 传播数据库读取错误。
    pub async fn counters(&self, actor: &AuditActor) -> Result<Vec<CounterView>> {
        let service = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .authorization
                        .require(&actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
                        .await?;

                    let year = BusinessDate::today().ymd().0;
                    let mut views = Vec::new();
                    for group in [NumberGroup::FSY, NumberGroup::ZHYF, NumberGroup::GYL, NumberGroup::BDKJ] {
                        let initial = ContractCounter::initial(group, year)?;
                        let existing =
                            service.db.contract_counters().find_by_id(&initial.base.id, executor).await?;
                        views.push(match existing {
                            Some(counter) => counter.into(),
                            None => CounterView {
                                group,
                                year,
                                last_sequence: initial.last_sequence,
                                version: None,
                            },
                        });
                    }
                    Ok(views)
                })
            })
            .await
    }

    /// 原子校准已用流水并记审计；不得覆盖旧版本或降低初始号段。
    /// 校准后流水。
    /// `request` 为编号组、年度、已用流水与期望版本；`actor` 为管理员。
    /// 版本冲突、回退、溢出或事务失败。
    pub async fn configure_counter(
        &self,
        request: ConfigureCounterRequest,
        actor: &AuditActor,
    ) -> Result<CounterView> {
        let actor = actor.clone();
        let initial = ContractCounter::initial(request.group, request.year)?;
        let service = self.clone();
        let audit = self.audit.resource_log(
            actor.clone(),
            "contract_number_counter.configure",
            "contract_number_counter",
            initial.base.id.clone(),
        )?;

        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    service
                        .authorization
                        .require(&actor, &contract::TEMPLATES_CREATE, AccessTarget::Operation, executor)
                        .await?;

                    let existing =
                        service.db.contract_counters().find_by_id(&initial.base.id, executor).await?;
                    if existing.as_ref().map(|c| c.base.version) != request.version {
                        return Err(Error::ConflictError("流水已变化，请刷新后重试".into()));
                    }

                    let is_new = existing.is_none();
                    let mut counter = existing.unwrap_or(initial);
                    counter.advance_to(request.last_sequence)?;
                    if is_new {
                        service.db.contract_counters().create(&counter, executor).await?;
                    } else {
                        service.db.contract_counters().update(&mut counter, executor).await?;
                    }
                    service.audit.persist(&audit, executor).await?;
                    Ok(counter.into())
                })
            })
            .await
    }
}
