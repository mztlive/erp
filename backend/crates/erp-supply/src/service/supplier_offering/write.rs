//! 原命令写序与响应/命令ID生成时点，由生产Mongo provider执行。
use super::*;
use crate::repository::supplier_offering::write::{self as repository_write, OfferingWritePort};
/// 原同域写入顺序；每步使用同一调用方Executor。
pub(super) async fn created<P: OfferingWritePort>(
    port: &P,
    prepared: &PreparedCreate,
    executor: &mut dyn Executor,
) -> Result<()> {
    repository_write::create_triple(
        port,
        &prepared.offering,
        &prepared.revision,
        &prepared.availability,
        executor,
    )
    .await?;
    port.command(&prepared.command, executor).await?;
    Ok(())
}
/// 原同域写入顺序；每步使用同一调用方Executor。
pub(super) async fn revised<P: OfferingWritePort>(
    port: &P,
    prepared: &mut PreparedRevision,
    executor: &mut dyn Executor,
) -> Result<ReviseSupplierOfferingResult> {
    repository_write::append_revision(port, &mut prepared.offering, &prepared.revision, executor).await?;
    let result = ReviseSupplierOfferingResult {
        offering_id: prepared.offering.base.id.clone(),
        revision_id: prepared.revision.base.id.clone(),
        revision_no: prepared.next_no,
        status: prepared.next_status,
        version: prepared.expected_version,
        safety_pause: None,
    };
    let command = SupplierOfferingCommand::with_result(
        next_id(),
        &prepared.idempotency_key,
        REVISE_OFFERING_COMMAND,
        &prepared.fingerprint,
        &result,
    )?;
    port.command(&command, executor).await?;
    Ok(result)
}
/// 原同域写入顺序；每步使用同一调用方Executor。
pub(super) async fn availability<P: OfferingWritePort>(
    port: &P,
    prepared: &mut PreparedAvailability,
    executor: &mut dyn Executor,
) -> Result<UpdateSupplierOfferingAvailabilityResult> {
    port.update_availability(&mut prepared.availability, executor).await?;
    let result = UpdateSupplierOfferingAvailabilityResult {
        offering_id: prepared.offering_id.to_string(),
        availability_status: prepared.availability.availability_status,
        availability_version: prepared.result_version,
        source_updated_at: prepared.availability.source_updated_at.unix_secs(),
        safety_pause: None,
    };
    let command = SupplierOfferingCommand::with_result(
        next_id(),
        &prepared.idempotency_key,
        UPDATE_OFFERING_AVAILABILITY_COMMAND,
        &prepared.fingerprint,
        &result,
    )?;
    port.command(&command, executor).await?;
    Ok(result)
}

#[cfg(test)]
mod tests {
    use std::sync::Mutex;

    use async_trait::async_trait;

    use super::*;
    use crate::dto::supplier_offering::SupplierOfferingTermsWrite;
    use crate::entity::supplier_offering::{AvailabilityStatus, OfferingSourceType};
    struct Marker(u64);
    impl Executor for Marker {
        fn session(&mut self) -> Option<&mut mongodb::ClientSession> {
            None
        }
    }
    struct Recorder {
        pointer: usize,
        calls: Mutex<Vec<&'static str>>,
        fail: Option<usize>,
    }
    impl Recorder {
        fn record(&self, name: &'static str, executor: &mut dyn Executor) -> persistence_core::Result<()> {
            assert_eq!(executor as *mut dyn Executor as *mut () as usize, self.pointer);
            let mut calls = self.calls.lock().unwrap();
            let i = calls.len();
            calls.push(name);
            if self.fail == Some(i) {
                return Err(persistence_core::Error::OptimisticLockingError);
            }
            Ok(())
        }
    }
    #[async_trait]
    impl OfferingWritePort for Recorder {
        async fn offering(
            &self,
            value: &SupplierOffering,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            assert_eq!(value.base.id, "offering");
            self.record("offering", executor)
        }
        async fn revision(
            &self,
            value: &SupplierOfferingRevision,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            assert_eq!(value.supplier_offering_id.as_ref(), "offering");
            self.record("revision", executor)
        }
        async fn availability(
            &self,
            value: &SupplierOfferingAvailability,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            assert_eq!(value.supplier_offering_id.as_ref(), "offering");
            self.record("availability", executor)
        }
        async fn update_offering(
            &self,
            value: &mut SupplierOffering,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            self.record("offering_cas", executor)?;
            value.base.version += 1;
            Ok(())
        }
        async fn update_availability(
            &self,
            value: &mut SupplierOfferingAvailability,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            self.record("availability_cas", executor)?;
            value.base.version += 1;
            Ok(())
        }
        async fn command(
            &self,
            value: &SupplierOfferingCommand,
            executor: &mut dyn Executor,
        ) -> persistence_core::Result<()> {
            assert_eq!(value.idempotency_key, "key");
            assert!(!value.result_json.is_empty());
            self.record("command", executor)
        }
    }
    fn fixture() -> PreparedCreate {
        let req = CreateSupplierOfferingRequest {
            sku_id: "sku".into(),
            supplier_id: "supplier".into(),
            supplier_product_code: None,
            supplier_sku_code: "SUP-1".into(),
            source_type: OfferingSourceType::Manual,
            source_connection_id: None,
            terms: SupplierOfferingTermsWrite {
                dropship_supply_price_gross: "10".into(),
                bulk_supply_price_gross: "9".into(),
                input_tax_rate: "0.13".into(),
                bulk_minimum_order_quantity: "1".into(),
                supply_region: vec!["CN".into()],
                product_capabilities: vec![],
                valid_from: "2026-01-01".into(),
                valid_to: None,
                dropship_express: None,
                freight_amount: None,
                service_fee_amount: None,
            },
            availability_status: AvailabilityStatus::Available,
            available_quantity: Some("10".into()),
            source_updated_at: Some(100),
            source_revision_token: None,
            change_reason: "reason".into(),
            idempotency_key: "key".into(),
        };
        let id = SupplierOfferingId::new("offering");
        let mut offering =
            SupplierOffering::new(id.clone(), req.try_into_offering_data().unwrap(), "actor").unwrap();
        let revision = SupplierOfferingRevision::new(
            SupplierOfferingRevisionId::new("revision"),
            req.terms.try_into_revision_data(id.clone(), 1).unwrap(),
        )
        .unwrap();
        let availability = SupplierOfferingAvailability::new(
            SupplierOfferingAvailabilityId::new("availability"),
            req.try_into_availability_data(
                id,
                Instant::from_unix_secs(100),
                Instant::from_unix_secs(101),
                "actor".into(),
            )
            .unwrap(),
        )
        .unwrap();
        offering.stable.current_revision_id = Some(revision.base.id.clone());
        let result = CreateSupplierOfferingResult {
            offering_id: "offering".into(),
            revision_id: "revision".into(),
            availability_id: "availability".into(),
            revision_no: 1,
            status: OfferingStatus::Active,
        };
        let fingerprint = req.command_fingerprint().unwrap();
        let command = SupplierOfferingCommand::with_result(
            "command",
            "key",
            CREATE_OFFERING_COMMAND,
            &fingerprint,
            &result,
        )
        .unwrap();
        PreparedCreate { offering, revision, availability, command, result, fingerprint }
    }
    async fn invoke(kind: u8, fail: Option<usize>) -> (Result<()>, Vec<&'static str>) {
        let mut executor = Marker(163);
        let port =
            Recorder { pointer: &mut executor as *mut Marker as usize, calls: Mutex::new(vec![]), fail };
        let f = fixture();
        let result = match kind {
            0 => created(&port, &f, &mut executor).await,
            1 => {
                let mut revision = f.revision;
                revision.revision.revision_no = 2;
                let mut p = PreparedRevision {
                    expected_version: f.offering.base.version + 1,
                    offering: f.offering,
                    revision,
                    next_no: 2,
                    next_status: OfferingStatus::Active,
                    idempotency_key: "key".into(),
                    fingerprint: f.fingerprint,
                };
                revised(&port, &mut p, &mut executor).await.map(|result| {
                    assert_eq!(result.version, p.offering.base.version);
                    assert_eq!(result.revision_no, p.revision.revision.revision_no);
                    assert_eq!(result.safety_pause, None);
                })
            },
            _ => {
                let mut p = PreparedAvailability {
                    result_version: f.availability.base.version + 1,
                    availability: f.availability,
                    offering_id: SupplierOfferingId::new("offering"),
                    idempotency_key: "key".into(),
                    fingerprint: f.fingerprint,
                };
                availability(&port, &mut p, &mut executor).await.map(|result| {
                    assert_eq!(result.availability_version, p.availability.base.version);
                    assert_eq!(result.source_updated_at, 100);
                    assert_eq!(result.safety_pause, None);
                })
            },
        };
        assert_eq!(executor.0, 163);
        (result, port.calls.into_inner().unwrap())
    }
    #[tokio::test]
    async fn offering_writes_preserve_one_executor_and_original_three_command_orders() {
        for (kind, order) in [
            (0, vec!["offering", "revision", "availability", "command"]),
            (1, vec!["revision", "offering_cas", "command"]),
            (2, vec!["availability_cas", "command"]),
        ] {
            let (result, calls) = invoke(kind, None).await;
            result.unwrap();
            assert_eq!(calls, order);
        }
    }
    #[tokio::test]
    async fn offering_writes_stop_at_every_storage_failure() {
        for (kind, order) in [
            (0, vec!["offering", "revision", "availability", "command"]),
            (1, vec!["revision", "offering_cas", "command"]),
            (2, vec!["availability_cas", "command"]),
        ] {
            for i in 0..order.len() {
                let (result, calls) = invoke(kind, Some(i)).await;
                assert!(
                    matches!(result,Err(Error::ConflictError(ref e)) if e=="数据已被其他请求修改，请刷新后重试")
                );
                assert_eq!(calls, order[..=i]);
            }
        }
    }
}
