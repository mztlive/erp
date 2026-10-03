//! 生成或恢复演示仓库，并核对现行收发责任，不覆盖人工配置。

use application_core::AuditActor;
use erp_warehouse::{
    CreateWarehouseRequest, HandlerDuty, IdentityFactPort, Warehouse, WarehouseExt,
    WarehouseFulfillmentOperation,
};
use persistence_core::NoTransaction;

use super::ensure_dictionary::{EnsureOutcome, step_label};
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle};
use crate::adapters::MongoWarehouseIdentity;
use crate::{Error, Result};

impl DemoMasterDataService {
    /// 解析仓储岗位后创建或恢复登记仓库。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`template` - 仓库创建输入；`handler_user_id` - 仓储经办人 ID。
    ///
    /// # 返回
    /// 返回仓库创建、恢复、已存在或缺少岗位的提示。
    ///
    /// # 错误
    /// 编号归属不符、仓库停用、收发责任不可用或持久化失败时返回错误。
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
            .find_one_by_field_including_deleted("warehouse_code", step.key.clone(), &mut NoTransaction)
            .await?
        {
            record::ensure_owned(&self.db, &step.key, &existing.base.id).await?;
            let live = existing.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
            if !live {
                lifecycle::restore_warehouse(&self.db, actor, &existing.base.id).await?;
            }
            self.verify_warehouse_ready(&existing.base.id).await?;
            self.remember_warehouse(step, existing.base.id).await?;
            return Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored });
        }
        let view = match self
            .warehouses()
            .warehouse_create(warehouse_request(template, handler_user_id), actor)
            .await
        {
            Ok(view) => view,
            Err(erp_warehouse::Error::BusinessLogicError(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成仓库：{message}")));
            },
            Err(error) => return Err(error.into()),
        };
        self.remember_warehouse(step, view.id.clone()).await?;
        self.verify_warehouse_ready(&view.id).await?;
        Ok(EnsureOutcome::Created)
    }

    /// 读取现行仓库并通过正式身份事实核对两项收发责任。
    async fn verify_warehouse_ready(&self, id: &str) -> Result<()> {
        let warehouse = self
            .db
            .warehouses()
            .find_by_id(id, &mut NoTransaction)
            .await?
            .ok_or_else(|| Error::NotFound("演示仓库未能读回，请核对登记记录".into()))?;
        let identity = MongoWarehouseIdentity::new(self.db.clone(), self.rbac.clone());
        warehouse_ready(&warehouse, &identity).await
    }

    /// 登记仓库实际主键。
    async fn remember_warehouse(&self, step: &DemoStep, id: String) -> Result<()> {
        record::save(
            &self.db,
            &DemoMasterRecord {
                key: step.key.clone(),
                kind: step.kind.as_str().to_string(),
                entity_id: id,
                related_ids: Vec::new(),
                label: step_label(step),
                removed: false,
            },
        )
        .await
    }
}

/// 将仓储岗位占位引用替换为当前账号 ID。
fn warehouse_request(template: &CreateWarehouseRequest, handler_user_id: &str) -> CreateWarehouseRequest {
    let mut request = template.clone();
    request.inbound_handler_user_id = handler_user_id.to_string();
    request.outbound_handler_user_id = handler_user_id.to_string();
    request
}

/// 使用领域责任解析与身份端口验证现行经办人，不调整仓库状态或负责人。
async fn warehouse_ready(warehouse: &Warehouse, identity: &dyn IdentityFactPort) -> Result<()> {
    for (operation, duty) in [
        (WarehouseFulfillmentOperation::Receipt, HandlerDuty::Inbound),
        (WarehouseFulfillmentOperation::WarehouseShip, HandlerDuty::Outbound),
    ] {
        let id = warehouse.fulfillment_handler(operation).map_err(|error| {
            Error::ValidationError(format!("演示仓库 {} 未就绪：{error}", warehouse.warehouse_code))
        })?;
        let fact = identity.handler_identity(id).await?;
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
        fail: bool,
    }

    #[async_trait]
    impl IdentityFactPort for Identity {
        async fn handler_identity(&self, id: &str) -> erp_warehouse::Result<Option<HandlerIdentityFact>> {
            self.calls.lock().unwrap().push(id.into());
            if self.fail {
                return Err(erp_warehouse::Error::Internal("身份读取失败".into()));
            }
            Ok(self.facts.iter().find(|fact| fact.user_id == id).cloned())
        }

        async fn admin_handler_identities(&self) -> erp_warehouse::Result<Vec<HandlerIdentityFact>> {
            panic!("演示就绪校验不得扫描全部账号")
        }
    }

    fn identity() -> Identity {
        Identity {
            facts: vec![fact("inbound", true, false), fact("outbound", false, true)],
            calls: Mutex::new(vec![]),
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
        warehouse_ready(&warehouse, &identity).await.unwrap();
        assert_eq!(warehouse, before);
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound", "outbound"]);
    }

    #[tokio::test]
    async fn stopped_and_unstaffed_warehouses_fail_before_identity_reads() {
        let mut warehouse = warehouse();
        let identity = identity();
        warehouse.stable.status = EnableStatus::Disabled;
        assert!(matches!(warehouse_ready(&warehouse, &identity).await,
            Err(Error::ValidationError(message)) if message.contains("停用")));
        warehouse.stable.status = EnableStatus::Active;
        warehouse.inbound_handler_user_id = None;
        assert!(matches!(warehouse_ready(&warehouse, &identity).await,
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
            assert!(matches!(warehouse_ready(&warehouse, &identity).await,
                Err(Error::ValidationError(message)) if message.contains("入库经办人不可用")));
            assert_eq!(*identity.calls.lock().unwrap(), ["inbound"]);
        }
        let mut identity = identity();
        identity.facts[1].outbound_eligible = false;
        assert!(matches!(warehouse_ready(&warehouse, &identity).await,
            Err(Error::ValidationError(message)) if message.contains("仓发经办人")));
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound", "outbound"]);
    }

    #[tokio::test]
    async fn identity_provider_failure_preserves_original_error_and_stops() {
        let mut identity = identity();
        identity.fail = true;
        assert!(matches!(warehouse_ready(&warehouse(), &identity).await,
            Err(Error::Internal(message)) if message == "身份读取失败"));
        assert_eq!(*identity.calls.lock().unwrap(), ["inbound"]);
    }
}
