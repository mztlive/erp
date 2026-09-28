//! 生成或恢复演示仓库。没有仓储账号时跳过。

use application_core::AuditActor;
use erp_warehouse::{CreateWarehouseRequest, WarehouseExt};
use persistence_core::NoTransaction;

use super::ensure_dictionary::{EnsureOutcome, step_label};
use super::plan::DemoStep;
use super::record::{self, DemoMasterRecord};
use super::{DemoMasterDataService, lifecycle};
use crate::Result;

impl DemoMasterDataService {
    pub(super) async fn ensure_warehouse(
        &self,
        actor: &AuditActor,
        step: &DemoStep,
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
            let live = existing.base.deleted_at == entity_core::NOT_DELETED_TIMESTAMP;
            if !live {
                lifecycle::restore_warehouse(&self.db, actor, &existing.base.id).await?;
            }
            self.remember_warehouse(step, existing.base.id).await?;
            return Ok(if live { EnsureOutcome::Skipped } else { EnsureOutcome::Restored });
        }
        let view = match self
            .warehouses()
            .warehouse_create(warehouse_request(step, handler_user_id)?, actor)
            .await
        {
            Ok(view) => view,
            Err(erp_warehouse::Error::BusinessLogicError(message)) => {
                return Ok(EnsureOutcome::Notice(format!("未生成仓库：{message}")));
            },
            Err(error) => return Err(error.into()),
        };
        self.remember_warehouse(step, view.id).await?;
        Ok(EnsureOutcome::Created)
    }

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

fn warehouse_request(step: &DemoStep, handler_user_id: &str) -> Result<CreateWarehouseRequest> {
    Ok(CreateWarehouseRequest {
        warehouse_code: step.key.clone(),
        name: step_label(step),
        address: format!("演示物流园{:02}号", step.ordinal),
        contact: format!("演示仓管{:02}", step.ordinal),
        effective_from: super::demo_date()?,
        effective_to: None,
        change_reason: "演示主数据".to_string(),
        status: None,
        inbound_handler_user_id: handler_user_id.to_string(),
        outbound_handler_user_id: handler_user_id.to_string(),
    })
}
