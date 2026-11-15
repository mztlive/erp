#[cfg(test)]
mod tests {
    use erp_core::common::time::Instant;
    use erp_core::ids::WorkItemId;

    use super::super::{WorkItem, WorkItemData, WorkItemType, direct_data};

    #[test]
    fn fulfillment_task_cannot_bypass_frozen_responsibility_key() {
        let data = WorkItemData {
            work_item_type: WorkItemType::FulfillmentOperation,
            business_object_type: "delivery".to_string(),
            owner_role: "warehouse_outbound_handler".to_string(),
            reason_code: Some("WAREHOUSE_DELIVERY_READY".to_string()),
            ..direct_data()
        };
        assert!(
            WorkItem::new_at(
                WorkItemId::new("wi-fulfillment-missing-key"),
                data.clone(),
                Instant::from_unix_secs(100),
            )
            .is_err()
        );
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-fulfillment"),
            data,
            "warehouse:wh-1:warehouse_ship",
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("warehouse:wh-1:warehouse_ship"));
    }

    #[test]
    fn customer_acceptance_task_requires_frozen_responsibility_key() {
        let data = WorkItemData {
            work_item_type: WorkItemType::CustomerAcceptanceRegistration,
            business_object_type: "sales_order".to_string(),
            owner_role: "sales_order_owner".to_string(),
            reason_code: Some("CUSTOMER_ACCEPTANCE_REQUIRED".to_string()),
            ..direct_data()
        };
        assert!(
            WorkItem::new_at(
                WorkItemId::new("wi-acceptance-missing-key"),
                data.clone(),
                Instant::from_unix_secs(100),
            )
            .is_err()
        );
        let item = WorkItem::new_with_responsibility_key(
            WorkItemId::new("wi-acceptance"),
            data,
            "sales_order:so-1:customer_acceptance",
        )
        .unwrap();
        assert_eq!(item.responsibility_key(), Some("sales_order:so-1:customer_acceptance"));
    }
}
