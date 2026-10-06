//! 一次命令一次事务；回执恢复只返回原结果，禁止重复业务写入。

use std::future::Future;
use std::pin::Pin;
use std::str::FromStr;

use application_core::{AuditActor, CommandReceipt};
use erp_audit::{
    AuditActorLogs, AuditFact, AuditFieldChange, AuditLog, AuditValue, BusinessEventContent,
    BusinessEventContext, BusinessEventResult, registered_action,
};
use erp_core::money::Quantity;
use erp_identity::PortalActor;
use erp_supply::entity::supplier_offering::AvailabilityStatus;
use erp_supply::portal::{PortalAvailabilityUpdateResult, PortalOfferingService};
use persistence_core::{Executor, Transactional};
use serde::Serialize;
use serde::de::DeserializeOwned;

use super::SupplierPortalProcess;
use super::command_recovery::rejection;
use crate::audit::persist_log;
use crate::{Error, Result};

type WriteFuture<'a, T> = Pin<Box<dyn Future<Output = Result<T>> + Send + 'a>>;

impl SupplierPortalProcess {
    pub(super) async fn portal_command<T, P, F>(
        &self,
        actor: &PortalActor,
        action: &'static str,
        key: &str,
        payload: &P,
        write: F,
    ) -> Result<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync + 'static,
        P: Serialize,
        F: for<'a> FnOnce(SupplierPortalProcess, PortalActor, &'a mut dyn Executor) -> WriteFuture<'a, T>
            + Send
            + 'static,
    {
        Ok(self.portal_command_outcome(actor, action, key, payload, write).await?.0)
    }

    pub(super) async fn portal_command_outcome<T, P, F>(
        &self,
        actor: &PortalActor,
        action: &'static str,
        key: &str,
        payload: &P,
        write: F,
    ) -> Result<(T, bool)>
    where
        T: Serialize + DeserializeOwned + Send + Sync + 'static,
        P: Serialize,
        F: for<'a> FnOnce(SupplierPortalProcess, PortalActor, &'a mut dyn Executor) -> WriteFuture<'a, T>
            + Send
            + 'static,
    {
        actor.require_write()?;
        let command = scoped_command(&actor.account_id, &actor.supplier_id, action, key, payload)?;
        let this = self.clone();
        let actor = actor.clone();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    let current = this.session_validate(&actor, executor).await?;
                    current.require_write()?;
                    let service = PortalOfferingService::new(this.db.clone());
                    if let Some(result) = service.command_result(&command, executor).await? {
                        return Ok((decode(result)?, false));
                    }
                    let result = write(this.clone(), current.clone(), executor).await?;
                    this.command_commit(
                        &command,
                        &current.audit_actor(),
                        &current.supplier_id,
                        &result,
                        executor,
                    )
                    .await?;
                    Ok((result, true))
                })
            })
            .await
    }

    pub(super) async fn internal_command<T, P, F>(
        &self,
        actor: &AuditActor,
        supplier_id: &str,
        action: &'static str,
        key: &str,
        payload: &P,
        write: F,
    ) -> Result<T>
    where
        T: Serialize + DeserializeOwned + Send + Sync + 'static,
        P: Serialize,
        F: for<'a> FnOnce(SupplierPortalProcess, AuditActor, &'a mut dyn Executor) -> WriteFuture<'a, T>
            + Send
            + 'static,
    {
        let command = scoped_command(actor.id(), supplier_id, action, key, payload)?;
        let this = self.clone();
        let actor = actor.clone();
        let supplier_id = supplier_id.to_string();
        self.db
            .client()
            .clone()
            .with_transaction(move |executor| {
                Box::pin(async move {
                    this.internal_supplier(&actor, supplier_action(action), &supplier_id, executor).await?;
                    this.internal_permission(&actor, action, executor).await?;
                    let service = PortalOfferingService::new(this.db.clone());
                    if let Some(result) = service.command_result(&command, executor).await? {
                        return decode(result);
                    }
                    let result = write(this.clone(), actor.clone(), executor).await?;
                    this.command_commit(&command, &actor, &supplier_id, &result, executor).await?;
                    Ok(result)
                })
            })
            .await
    }

    async fn command_commit<T: Serialize + Sync>(
        &self,
        command: &CommandReceipt,
        actor: &AuditActor,
        supplier_id: &str,
        result: &T,
        executor: &mut dyn Executor,
    ) -> Result<()> {
        let value = serde_json::to_value(result).map_err(|error| Error::Internal(error.to_string()))?;
        let resource_id = value
            .get("id")
            .and_then(|v| v.as_str())
            .or_else(|| value.get("offering_id").and_then(|v| v.as_str()))
            .unwrap_or(supplier_id);
        let log = if command.action() == "supplier_portal.availability_update" {
            availability_log(command, actor, &value)?
        } else {
            actor
                .clone()
                .resource_log(command.action(), "supplier_portal_request", resource_id.to_string())?
                .with_command_id(Some(command.id().to_string()))?
        };
        persist_log(&self.db, &log, executor).await?;
        PortalOfferingService::new(self.db.clone())
            .command_commit(command, supplier_id, &value, executor)
            .await?;
        Ok(())
    }
}

fn supplier_action(action: &str) -> &'static str {
    match action {
        "supplier_portal.commercial_approve" => "update",
        _ => "detail",
    }
}

fn availability_log(
    command: &CommandReceipt,
    actor: &AuditActor,
    value: &serde_json::Value,
) -> Result<AuditLog> {
    let change: PortalAvailabilityUpdateResult =
        serde_json::from_value(value.clone()).map_err(|e| Error::Internal(e.to_string()))?;
    let mut changes = vec![AuditFieldChange {
        field: "availability_status".into(),
        before: availability_value(change.before.availability_status),
        after: availability_value(change.after.availability_status),
    }];
    let mut facts = Vec::new();
    match (change.before.available_quantity.as_deref(), change.after.available_quantity.as_deref()) {
        (Some(before), Some(after)) => changes.push(AuditFieldChange {
            field: "available_quantity".into(),
            before: AuditValue::Quantity { value: Quantity::from_str(before)? },
            after: AuditValue::Quantity { value: Quantity::from_str(after)? },
        }),
        (before, after) => {
            changes.push(AuditFieldChange {
                field: "quantity_reported".into(),
                before: quantity_state(before),
                after: quantity_state(after),
            });
            if let Some(before) = before {
                facts.push(AuditFact {
                    field: "before_available_quantity".into(),
                    value: AuditValue::Quantity { value: Quantity::from_str(before)? },
                });
            }
            if let Some(after) = after {
                facts.push(AuditFact {
                    field: "after_available_quantity".into(),
                    value: AuditValue::Quantity { value: Quantity::from_str(after)? },
                });
            }
        },
    }
    let context = BusinessEventContext::new(
        actor.clone(),
        registered_action(command.action(), "supplier_portal_request")?,
    )?
    .with_command_id(Some(command.id().into()))?;
    Ok(context.log(BusinessEventContent {
        target_id: change.current.offering_id,
        target_number: None,
        result: BusinessEventResult::Succeeded,
        field_changes: changes,
        facts,
    })?)
}
fn quantity_state(value: Option<&str>) -> AuditValue {
    match value {
        Some(_) => AuditValue::Code { code: "PROVIDED".into(), label: "已提供".into() },
        None => AuditValue::Code { code: "NOT_PROVIDED".into(), label: "未提供".into() },
    }
}
fn availability_value(value: AvailabilityStatus) -> AuditValue {
    AuditValue::Code { code: value.as_str().into(), label: value.label().into() }
}

fn decode<T: DeserializeOwned>(value: serde_json::Value) -> Result<T> {
    if let Some(rejection) = rejection(&value)? {
        return Err(rejection.into_error());
    }
    serde_json::from_value(value).map_err(|error| Error::Internal(format!("门户命令回执结果损坏: {error}")))
}

pub(super) fn scoped_command<P: Serialize>(
    actor_id: &str,
    supplier_id: &str,
    action: &str,
    key: &str,
    payload: &P,
) -> Result<CommandReceipt> {
    let payload = CommandReceipt::from_payload(
        "supplier-portal-",
        actor_id,
        action,
        "supplier_portal_request",
        key,
        payload,
    )?;
    Ok(CommandReceipt::from_resource_parts(
        "supplier-portal-",
        actor_id,
        action,
        "supplier_portal_request",
        supplier_id,
        key,
        [payload.fingerprint().as_str().to_string()],
    )?)
}

#[cfg(test)]
mod tests {
    use erp_core::AccountKind;
    use serde_json::json;

    use super::*;

    #[test]
    fn supplier_profile_write_is_required_only_for_commercial_activation() {
        assert_eq!(supplier_action("supplier_portal.commercial_approve"), "update");
        for action in [
            "supplier_portal.application_approve",
            "supplier_portal.application_return",
            "supplier_portal.new_product_approve",
            "supplier_portal.new_product_return",
            "supplier_portal.commercial_return",
            "supplier_portal.account_create",
            "supplier_portal.account_update",
            "supplier_portal.quote_access_update",
        ] {
            assert_eq!(supplier_action(action), "detail");
        }
    }

    #[test]
    fn terminal_rejections_decode_to_the_original_error_before_normal_result_deserialization() {
        let receipt = |kind: &str| {
            json!({
                "portal_command_outcome":"rejected",
                "rejection":{"kind":kind,"message":"原命令已封存"}
            })
        };
        assert!(matches!(decode::<PortalAvailabilityUpdateResult>(receipt("validation")),
            Err(Error::ValidationError(message)) if message == "原命令已封存"));
        assert!(matches!(decode::<PortalAvailabilityUpdateResult>(receipt("business")),
            Err(Error::BusinessLogicError(message)) if message == "原命令已封存"));
        assert!(matches!(decode::<PortalAvailabilityUpdateResult>(receipt("conflict")),
            Err(Error::ConflictError(message)) if message == "原命令已封存"));
        assert!(matches!(
            decode::<serde_json::Value>(json!({"portal_command_outcome":"rejected"})),
            Err(Error::Internal(_))
        ));
        let positive = json!({"offering_id":"original-offering","availability_version":2});
        assert_eq!(decode::<serde_json::Value>(positive.clone()).unwrap(), positive);
    }

    fn record(before: Option<&str>, after: Option<&str>) -> serde_json::Value {
        json!({"offering_id":"offering-1","availability_status":"UNAVAILABLE","availability_version":2,"source_updated_at":10,"safety_pause":null,
            "before":{"availability_status":"AVAILABLE","available_quantity":before,"version":1,"source_updated_at":9},
            "after":{"availability_status":"UNAVAILABLE","available_quantity":after,"version":2,"source_updated_at":10}})
    }
    #[test]
    fn production_audit_records_unknown_quantity_without_turning_it_into_zero() {
        let actor = AuditActor::new("external".into(), "supplier-user".into(), AccountKind::Supplier);
        let command = scoped_command(
            actor.id(),
            "supplier-1",
            "supplier_portal.availability_update",
            "original-key",
            &json!({"quantity":null}),
        )
        .unwrap();
        let log = availability_log(&command, &actor, &record(None, Some("0"))).unwrap();
        let event = log.structured_event.unwrap();
        assert_eq!(event.command_id.as_deref(), Some(command.id()));
        assert_eq!(event.actor_type, AccountKind::Supplier);
        let reported = event.field_changes.iter().find(|c| c.field == "quantity_reported").unwrap();
        assert_eq!(reported.before, AuditValue::Code { code: "NOT_PROVIDED".into(), label: "未提供".into() });
        assert!(event.facts.iter().all(|f| f.field != "before_available_quantity"));
        assert_eq!(
            event.facts.iter().find(|f| f.field == "after_available_quantity").unwrap().value,
            AuditValue::Quantity { value: Quantity::from_str("0").unwrap() }
        );
    }
    #[test]
    fn production_audit_records_concrete_quantities_and_valid_domain_status_labels() {
        let actor = AuditActor::new("external".into(), "supplier-user".into(), AccountKind::Supplier);
        let command = scoped_command(
            actor.id(),
            "supplier-1",
            "supplier_portal.availability_update",
            "original-key",
            &json!({"quantity":"2"}),
        )
        .unwrap();
        let log = availability_log(&command, &actor, &record(Some("1.5"), Some("2"))).unwrap();
        let event = log.structured_event.unwrap();
        let quantity = event.field_changes.iter().find(|c| c.field == "available_quantity").unwrap();
        assert_eq!(quantity.before, AuditValue::Quantity { value: Quantity::from_str("1.5").unwrap() });
        assert_eq!(quantity.after, AuditValue::Quantity { value: Quantity::from_str("2").unwrap() });
        assert_eq!(
            event.field_changes[0].after,
            AuditValue::Code { code: "UNAVAILABLE".into(), label: "不可供".into() }
        );
    }
}
