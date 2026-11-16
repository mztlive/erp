//! 流水起点和只向前的版本校准。

use application_core::AuditActor;
use erp_core::common::time::BusinessDate;
use erp_identity::AccessTarget;
use erp_permissions::contract;
use persistence_core::Transactional;

use super::ContractTemplateService;
use crate::dto::template::{ConfigureCounterRequest, CounterView};
use crate::entity::template::{ContractCounter, NumberGroup};
use crate::error::Result;
use crate::repository::templates::ContractTemplateExt;

impl ContractTemplateService {
    /// 返回当前业务年度四组流水；未落库时返回用户确认的历史起点。
    /// # 参数
    /// 无。
    /// # 返回
    /// 四组当前已用流水和可选版本。
    /// # 错误
    /// 数据库读取失败。
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
    /// # 参数
    /// * `request` - 编号组、年度、已用流水与期望版本。
    /// * `actor` - 管理员。
    /// # 返回
    /// 校准后流水。
    /// # 错误
    /// 版本冲突、回退、溢出或事务失败。
    pub async fn configure_counter(
        &self,
        request: ConfigureCounterRequest,
        actor: &AuditActor,
    ) -> Result<CounterView> {
        let actor = actor.clone();
        let service = self.clone();
        self.db
            .client()
            .with_transaction(move |executor| {
                Box::pin(async move { service.writer().configure_counter(request, &actor, executor).await })
            })
            .await
    }
}
