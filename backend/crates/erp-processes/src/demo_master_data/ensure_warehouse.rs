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
    /// 解析仓储岗位后创建或恢复登记仓库。
    ///
    /// # 参数
    /// `actor` - 操作人；`step` - 种子身份；`template` - 仓库创建输入；`handler_user_id` - 仓储经办人 ID。
    ///
    /// # 返回
    /// 返回仓库创建、恢复、已存在或缺少岗位的提示。
    ///
    /// # 错误
    /// 编号归属不符或持久化失败时返回错误。
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
        self.remember_warehouse(step, view.id).await?;
        Ok(EnsureOutcome::Created)
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
