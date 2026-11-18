//! 生成或恢复演示仓库，并核对现行收发责任，不覆盖人工配置。

use application_core::AuditActor;
use erp_warehouse::{
    CreateWarehouseRequest, HandlerDuty, IdentityFactPort, Warehouse, WarehouseExt,
    WarehouseFulfillmentOperation,
};
use erp_workflow::ports::WorkflowAuthorizationPort;
use persistence_core::{Executor, ReadExecutor};

use super::ensure_dictionary::EnsureOutcome;
use super::plan::DemoStep;
use super::{DemoMasterDataProcess, lifecycle, record};
use crate::{Error, Result};

impl<A: WorkflowAuthorizationPort> DemoMasterDataProcess<A> {
    // 使用真实仓储岗位创建或恢复已登记仓库，读回并核验收、发责任，供后续仓储演示使用。
    // 缺少岗位时返回提示；撞号、仓库停用、责任失效或写入失败时拒绝。
    pub(super) async fn ensure_warehouse(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
        template: &CreateWarehouseRequest,
        handler_user_id: Option<&str>,
    ) -> Result<EnsureOutcome> {
        let Some(handler_user_id) = handler_user_id else {
            return Ok(EnsureOutcome::Notice("没有可用的仓储账号，未生成仓库".to_string()));
        };

        if let Some(existing) = self
            .db
            .warehouses()
            .find_one_by_field_including_deleted(
                "warehouse_code",
                step.key.clone(),
                &mut ReadExecutor::default(),
            )
            .await?
        {
            record::ensure_owned(&self.db, &step.key, &existing.base.id).await?;
            let live = existing.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
            if !live {
                lifecycle::restore_warehouse(&self.db, self.authorization.clone(), actor, &existing.base.id)
                    .await?;
            }
            self.verify_warehouse_ready(&existing.base.id).await?;
            record::save(&self.db, step, existing.base.id, Vec::new()).await?;
            return Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored });
        }

        let view = match self
            .ports
            .warehouse
            .warehouse_create(warehouse_request(template, handler_user_id), actor)
            .await
        {
            Ok(view) => view,
            Err(erp_warehouse::Error::BusinessLogicError(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成仓库：{message}")));
            },
            Err(error) => return Err(error.into()),
        };

        record::save(&self.db, step, view.id.clone(), Vec::new()).await?;
        self.verify_warehouse_ready(&view.id).await?;
        Ok(EnsureOutcome::Created)
    }

    // 读取正式仓库并核验两项收发责任，确保登记存在不等于仓库已经可用。
    async fn verify_warehouse_ready(&self, id: &str) -> Result<()> {
        let mut executor = ReadExecutor::default();
        let warehouse = self
            .db
            .warehouses()
            .find_by_id(id, &mut executor)
            .await?
            .ok_or_else(|| Error::NotFound("演示仓库未能读回，请核对登记记录".into()))?;
        warehouse_ready(&warehouse, self.ports.warehouse_identity.as_ref(), &mut executor).await
    }
}

// 用真实仓储账号 ID 替换模板的收货及发货占位值，以便正式建仓时核验责任人身份。
fn warehouse_request(template: &CreateWarehouseRequest, handler_user_id: &str) -> CreateWarehouseRequest {
    let mut request = template.clone();
    request.inbound_handler_user_id = handler_user_id.to_string();
    request.outbound_handler_user_id = handler_user_id.to_string();
    request
}

// 通过仓库领域责任解析和身份端口核对当前收发经办人；只判断可用性，不更改仓库状态或接管原负责人。
async fn warehouse_ready(
    warehouse: &Warehouse,
    identity: &dyn IdentityFactPort,
    executor: &mut dyn Executor,
) -> Result<()> {
    for (operation, duty) in [
        (WarehouseFulfillmentOperation::Receipt, HandlerDuty::Inbound),
        (WarehouseFulfillmentOperation::WarehouseShip, HandlerDuty::Outbound),
    ] {
        let id = warehouse.fulfillment_handler(operation).map_err(|error| {
            Error::ValidationError(format!("演示仓库 {} 未就绪：{error}", warehouse.warehouse_code))
        })?;
        let fact = identity.handler_identity(id, executor).await?;
        if !fact.as_ref().is_some_and(|fact| fact.can_login && duty.is_eligible(fact)) {
            return Err(Error::ValidationError(format!(
                "演示仓库 {} 的{}经办人不可用或缺少完整执行权限，请先调整仓库责任或账号权限后重试",
                warehouse.warehouse_code,
                duty.label(),
            )));
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;
    use erp_core::ids::WarehouseId;
    use erp_warehouse::{EnableStatus, HandlerIdentityFact, WarehouseData};

    use super::*;

    struct Identity {
        facts: Vec<HandlerIdentityFact>,
        calls: Mutex<Vec<String>>,
        executor_ids: Mutex<Vec<usize>>,
        fail: bool,
    }

    #[async_trait]
    impl IdentityFactPort for Identity {
        async fn handler_identity(
            &self,
            id: &str,
            executor: &mut dyn Executor,
        ) -> erp_warehouse::Result<Option<HandlerIdentityFact>> {
            self.calls.lock().unwrap().push(id.into());
            self.executor_ids.lock().unwrap().push(executor as *mut dyn Executor as *mut () as usize);
            if self.fail {
                return Err(erp_warehouse::Error::Internal("身份读取失败".into()));
            }
            Ok(self.facts.iter().find(|fact| fact.user_id == id).cloned())
        }

        async fn admin_handler_identities(
            &self,
            _: &mut dyn Executor,
        ) -> erp_warehouse::Result<Vec<HandlerIdentityFact>> {
            panic!("演示就绪校验不得扫描全部账号")
        }
    }

    fn identity() -> Identity {
        Identity {
            facts: vec![fact("inbound", true, false), fact("outbound", false, true)],
            calls: Mutex::new(vec![]),
            executor_ids: Mutex::new(vec![]),
            fail: false,
        }
    }

    fn fact(id: &str, inbound: bool, outbound: bool) -> HandlerIdentityFact {
        HandlerIdentityFact {
            user_id: id.into(),
            display_name: id.into(),
            account: id.into(),
            can_login: true,
            inbound_eligible: inbound,
            outbound_eligible: outbound,
        }
    }

    fn warehouse() -> Warehouse {
        Warehouse::new(
            WarehouseId::new("warehouse-id"),
            WarehouseData::new("DEMO-MD-W-01")
                .with_inbound_handler_user_id("inbound")
                .with_outbound_handler_user_id("outbound"),
            "admin",
        )
        .unwrap()
    }

    #[tokio::test]
    async fn existing_warehouse_checks_actual_handlers_without_changing_configuration() {
        let warehouse = warehouse();
        let before = warehouse.clone();
        let identity = identity();
        let mut executor = ReadExecutor::default();
        let expected = &mut executor as *mut ReadExecutor as usize;
        warehouse_ready(&warehouse, &identity, &mut executor).await.unwrap();
        assert_eq!(warehouse, before);
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound", "outbound"]);
        assert_eq!(*identity.executor_ids.lock().unwrap(), [expected, expected]);
    }

    #[tokio::test]
    async fn stopped_and_unstaffed_warehouses_fail_before_identity_reads() {
        let mut warehouse = warehouse();
        let identity = identity();
        warehouse.stable.status = EnableStatus::Disabled;
        assert!(matches!(warehouse_ready(&warehouse, &identity, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message.contains("停用")));

        warehouse.stable.status = EnableStatus::Active;
        warehouse.inbound_handler_user_id = None;
        assert!(matches!(warehouse_ready(&warehouse, &identity, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message.contains("未配置入库经办人")));
        assert!(identity.calls.lock().unwrap().is_empty());
    }

    #[tokio::test]
    async fn unavailable_or_unauthorized_handler_stops_at_first_duty() {
        let warehouse = warehouse();
        for missing in [false, true] {
            let mut identity = identity();
            if missing {
                identity.facts.clear();
            } else {
                identity.facts[0].can_login = false;
            }
            assert!(matches!(warehouse_ready(&warehouse, &identity, &mut ReadExecutor::default()).await,
                Err(Error::ValidationError(message)) if message.contains("入库经办人不可用")));
            assert_eq!(*identity.calls.lock().unwrap(), ["inbound"]);
        }

        let mut identity = identity();
        identity.facts[1].outbound_eligible = false;
        assert!(matches!(warehouse_ready(&warehouse, &identity, &mut ReadExecutor::default()).await,
            Err(Error::ValidationError(message)) if message.contains("仓发经办人")));
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound", "outbound"]);
    }

    #[tokio::test]
    async fn identity_provider_failure_preserves_original_error_and_stops() {
        let mut identity = identity();
        identity.fail = true;
        assert!(matches!(warehouse_ready(&warehouse(), &identity, &mut ReadExecutor::default()).await,
            Err(Error::Internal(message)) if message == "身份读取失败"));
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound"]);
    }
}
