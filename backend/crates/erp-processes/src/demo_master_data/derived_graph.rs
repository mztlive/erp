//! 演示衍生数据的外键图。
//!
//! 只删除图上点名的集合和字段。主数据及专属子表由 master_graph 按实际 ID 硬删。
//! 库存按演示 SKU 或仓库定位；发现非演示 SKU 时整轮拒绝删除。

/// 演示身份里会被业务单据引用的一类。
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(super) enum SeedKind {
    /// 客户角色。
    Customer,
    /// 主体。
    Party,
    /// 供应商角色。
    Supplier,
    /// SKU。
    Sku,
    /// 仓库 ID。
    Warehouse,
}

/// 一条外键。`seed` 为空时，字段匹配已经定位到的单据 ID。
pub(super) struct Edge {
    /// 集合名。
    pub collection: &'static str,
    /// 外键字段。可以是 Mongo 点路径。
    pub field: &'static str,
    /// 匹配演示身份；空表示匹配已定位的单据 ID。
    pub seed: Option<SeedKind>,
    /// 命中文档上要继续向上收集的父 ID 字段。
    pub lift: Option<&'static str>,
}

/// 按主键收回已经被子表指上来的单据，并可再向上收集一层。
pub(super) struct IdPull {
    /// 集合名。
    pub collection: &'static str,
    /// 命中后继续向上收集的字段。
    pub lift: Option<&'static str>,
}

const EDGES: &[Edge] = &[
    Edge { collection: "contracts", field: "customer_id", seed: Some(SeedKind::Customer), lift: None },
    Edge { collection: "contract_revisions", field: "contract_id", seed: None, lift: None },
    Edge { collection: "contracts", field: "settlement_party_id", seed: Some(SeedKind::Party), lift: None },
    Edge { collection: "stock_balances", field: "warehouse_id", seed: Some(SeedKind::Warehouse), lift: None },
    Edge {
        collection: "stock_movements",
        field: "warehouse_id",
        seed: Some(SeedKind::Warehouse),
        lift: None,
    },
    Edge {
        collection: "stock_reservations",
        field: "warehouse_id",
        seed: Some(SeedKind::Warehouse),
        lift: None,
    },
    Edge {
        collection: "stock_adjustments",
        field: "warehouse_id",
        seed: Some(SeedKind::Warehouse),
        lift: None,
    },
    Edge {
        collection: "deliveries",
        field: "warehouse_id",
        seed: Some(SeedKind::Warehouse),
        lift: Some("sales_order_id"),
    },
    Edge {
        collection: "purchase_receipts",
        field: "warehouse_id",
        seed: Some(SeedKind::Warehouse),
        lift: Some("purchase_order_id"),
    },
    Edge {
        collection: "supplier_offerings",
        field: "supplier_id",
        seed: Some(SeedKind::Supplier),
        lift: None,
    },
    Edge { collection: "supplier_offerings", field: "sku_id", seed: Some(SeedKind::Sku), lift: None },
    Edge { collection: "supplier_offering_revisions", field: "supplier_offering_id", seed: None, lift: None },
    Edge {
        collection: "supplier_offering_availabilities",
        field: "supplier_offering_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_order_actions",
        field: "supplier_fulfillment_order_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_order_action_lines",
        field: "supplier_order_action_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_refund_facts",
        field: "supplier_fulfillment_order_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_refund_allocations",
        field: "supplier_refund_fact_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_settlement_statements",
        field: "supplier_id",
        seed: Some(SeedKind::Supplier),
        lift: None,
    },
    Edge { collection: "supplier_settlement_items", field: "statement_id", seed: None, lift: None },
    Edge {
        collection: "supplier_settlement_differences",
        field: "statement_item_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_settlement_difference_evidence",
        field: "statement_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "sales_selection_prepare_tasks", field: "booklet_id", seed: None, lift: None },
    Edge { collection: "sales_selection_sessions", field: "booklet_id", seed: None, lift: None },
    Edge { collection: "sales_selection_idempotency", field: "booklet_id", seed: None, lift: None },
    Edge { collection: "document_attachments", field: "document_id", seed: None, lift: None },
    Edge {
        collection: "sales_order_working_copy_lines",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("working_copy_id"),
    },
    Edge {
        collection: "purchase_order_submission_lines",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("purchase_order_submission_id"),
    },
    Edge { collection: "stock_adjustment_lines", field: "stock_adjustment_id", seed: None, lift: None },
    Edge { collection: "sales_return_cases", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "sales_return_lines", field: "sales_return_case_id", seed: None, lift: None },
    Edge { collection: "purchase_return_orders", field: "purchase_order_id", seed: None, lift: None },
    Edge { collection: "purchase_return_lines", field: "purchase_return_order_id", seed: None, lift: None },
    Edge { collection: "customer_refunds", field: "sales_return_case_id", seed: None, lift: None },
    Edge { collection: "supplier_refunds", field: "purchase_return_order_id", seed: None, lift: None },
    Edge { collection: "electronic_deliveries", field: "sales_order_line_id", seed: None, lift: None },
    Edge { collection: "service_fulfillments", field: "sales_order_line_id", seed: None, lift: None },
    Edge { collection: "receipt_allocations", field: "receivable_entry_id", seed: None, lift: None },
    Edge { collection: "payment_allocations", field: "payable_entry_id", seed: None, lift: None },
    Edge { collection: "invoices", field: "party_id", seed: Some(SeedKind::Party), lift: None },
    Edge { collection: "invoices", field: "sales_invoice_request_id", seed: None, lift: None },
    Edge { collection: "sales_invoice_allocations", field: "invoice_id", seed: None, lift: None },
    Edge { collection: "purchase_invoice_allocations", field: "invoice_id", seed: None, lift: None },
    Edge { collection: "receivable_entries", field: "source_document_id", seed: None, lift: None },
    Edge { collection: "payable_entries", field: "source_document_id", seed: None, lift: None },
    Edge { collection: "receivable_entry_offsets", field: "decrease_entry_id", seed: None, lift: None },
    Edge { collection: "receivable_entry_offsets", field: "increase_entry_id", seed: None, lift: None },
    Edge { collection: "payable_entry_offsets", field: "decrease_entry_id", seed: None, lift: None },
    Edge { collection: "payable_entry_offsets", field: "increase_entry_id", seed: None, lift: None },
    Edge { collection: "cost_allocations", field: "sales_order_id", seed: None, lift: None },
    Edge {
        collection: "purchase_line_sales_allocations",
        field: "sales_order_revision_line_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "purchase_line_sales_allocations",
        field: "purchase_order_revision_line_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "sales_selection_display_items", field: "booklet_id", seed: None, lift: None },
    Edge { collection: "sales_selection_pool_members", field: "booklet_id", seed: None, lift: None },
    Edge {
        collection: "sales_selection_proposal_display_lines",
        field: "proposal_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "sales_selection_proposal_sku_lines", field: "proposal_id", seed: None, lift: None },
    Edge { collection: "sales_orders", field: "customer_id", seed: Some(SeedKind::Customer), lift: None },
    Edge {
        collection: "sales_orders",
        field: "settlement_party_id",
        seed: Some(SeedKind::Party),
        lift: None,
    },
    Edge {
        collection: "sales_order_working_copies",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: Some("sales_order_id"),
    },
    Edge {
        collection: "sales_order_submissions",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: Some("sales_order_id"),
    },
    Edge {
        collection: "sales_change_submissions",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge {
        collection: "sales_selection_booklets",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge {
        collection: "sales_selection_proposals",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge { collection: "purchase_orders", field: "supplier_id", seed: Some(SeedKind::Supplier), lift: None },
    Edge {
        collection: "purchase_order_submissions",
        field: "supplier_id",
        seed: Some(SeedKind::Supplier),
        lift: Some("purchase_order_id"),
    },
    Edge {
        collection: "receivable_accounts",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge {
        collection: "sales_invoice_requests",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge {
        collection: "customer_receipts",
        field: "customer_id",
        seed: Some(SeedKind::Customer),
        lift: None,
    },
    Edge { collection: "customer_refunds", field: "customer_id", seed: Some(SeedKind::Customer), lift: None },
    Edge { collection: "payable_accounts", field: "supplier_id", seed: Some(SeedKind::Supplier), lift: None },
    Edge {
        collection: "supplier_payments",
        field: "supplier_id",
        seed: Some(SeedKind::Supplier),
        lift: None,
    },
    Edge { collection: "supplier_refunds", field: "supplier_id", seed: Some(SeedKind::Supplier), lift: None },
    Edge {
        collection: "supplier_fulfillment_orders",
        field: "supplier_id",
        seed: Some(SeedKind::Supplier),
        lift: None,
    },
    Edge { collection: "stock_balances", field: "sku_id", seed: Some(SeedKind::Sku), lift: None },
    Edge { collection: "stock_movements", field: "sku_id", seed: Some(SeedKind::Sku), lift: None },
    Edge { collection: "stock_reservations", field: "sku_id", seed: Some(SeedKind::Sku), lift: None },
    Edge {
        collection: "stock_adjustment_lines",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("stock_adjustment_id"),
    },
    Edge {
        collection: "sales_order_goods_service_line_revisions",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("revision_line_id"),
    },
    Edge {
        collection: "sales_order_submission_lines",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("submission_id"),
    },
    Edge {
        collection: "purchase_order_revision_lines",
        field: "sku_id",
        seed: Some(SeedKind::Sku),
        lift: Some("purchase_order_revision_id"),
    },
    Edge { collection: "sales_order_lines", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "sales_order_working_copies", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "sales_order_working_copy_lines", field: "working_copy_id", seed: None, lift: None },
    Edge { collection: "sales_order_submissions", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "sales_order_submission_lines", field: "submission_id", seed: None, lift: None },
    Edge { collection: "sales_order_revisions", field: "sales_order_id", seed: None, lift: None },
    Edge {
        collection: "sales_order_revision_lines",
        field: "sales_order_revision_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "sales_order_goods_service_line_revisions",
        field: "revision_line_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "sales_order_voucher_line_revisions",
        field: "revision_line_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "sales_change_orders", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "sales_change_submissions", field: "sales_change_order_id", seed: None, lift: None },
    Edge {
        collection: "sales_change_submission_lines",
        field: "sales_change_submission_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "purchase_order_submissions", field: "purchase_order_id", seed: None, lift: None },
    Edge {
        collection: "purchase_order_submission_lines",
        field: "purchase_order_submission_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "purchase_order_revisions", field: "purchase_order_id", seed: None, lift: None },
    Edge {
        collection: "purchase_order_revision_lines",
        field: "purchase_order_revision_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "purchase_change_orders", field: "purchase_order_id", seed: None, lift: None },
    Edge {
        collection: "purchase_change_submissions",
        field: "purchase_change_order_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "purchase_change_submission_lines",
        field: "purchase_change_submission_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "deliveries", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "delivery_lines", field: "delivery_id", seed: None, lift: None },
    Edge { collection: "purchase_receipts", field: "purchase_order_id", seed: None, lift: None },
    Edge { collection: "purchase_receipt_lines", field: "purchase_receipt_id", seed: None, lift: None },
    Edge { collection: "customer_acceptances", field: "sales_order_id", seed: None, lift: None },
    Edge { collection: "customer_acceptance_lines", field: "customer_acceptance_id", seed: None, lift: None },
    Edge {
        collection: "supplier_fulfillment_items",
        field: "supplier_fulfillment_order_id",
        seed: None,
        lift: None,
    },
    Edge {
        collection: "supplier_order_status_histories",
        field: "supplier_fulfillment_order_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "work_items", field: "business_object_id", seed: None, lift: None },
    Edge { collection: "approval_process_instances", field: "subject.subject_id", seed: None, lift: None },
    Edge { collection: "approval_node_executions", field: "process_instance_id", seed: None, lift: None },
    Edge { collection: "approval_instance_assignees", field: "process_instance_id", seed: None, lift: None },
    Edge { collection: "approval_subject_snapshots", field: "business_object_id", seed: None, lift: None },
    Edge {
        collection: "approval_subject_snapshots",
        field: "approval_process_instance_id",
        seed: None,
        lift: None,
    },
    Edge { collection: "document_participants", field: "document_id", seed: None, lift: None },
    Edge { collection: "document_relations", field: "from_document_id", seed: None, lift: None },
    Edge { collection: "document_relations", field: "to_document_id", seed: None, lift: None },
    Edge { collection: "workflow_actions", field: "document_id", seed: None, lift: None },
    Edge { collection: "receivable_entries", field: "receivable_account_id", seed: None, lift: None },
    Edge { collection: "receipt_allocations", field: "customer_receipt_id", seed: None, lift: None },
    Edge { collection: "sales_invoice_allocations", field: "receivable_account_id", seed: None, lift: None },
    Edge { collection: "payable_entries", field: "payable_account_id", seed: None, lift: None },
    Edge { collection: "payment_allocations", field: "supplier_payment_id", seed: None, lift: None },
    Edge { collection: "purchase_invoice_allocations", field: "payable_account_id", seed: None, lift: None },
    Edge { collection: "stock_reservation_entries", field: "reservation_id", seed: None, lift: None },
];

const ID_PULLS: &[IdPull] = &[
    IdPull { collection: "stock_adjustments", lift: None },
    IdPull { collection: "sales_order_working_copies", lift: Some("sales_order_id") },
    IdPull { collection: "purchase_order_submissions", lift: Some("purchase_order_id") },
    IdPull { collection: "supplier_fulfillment_orders", lift: None },
    IdPull { collection: "sales_order_revision_lines", lift: Some("sales_order_revision_id") },
    IdPull { collection: "sales_order_revisions", lift: Some("sales_order_id") },
    IdPull { collection: "sales_order_submissions", lift: Some("sales_order_id") },
    IdPull { collection: "sales_orders", lift: None },
    IdPull { collection: "purchase_order_revisions", lift: Some("purchase_order_id") },
    IdPull { collection: "purchase_orders", lift: None },
    IdPull { collection: "business_documents", lift: None },
];

const KEPT: &[&str] = &[
    "accounts",
    "roles",
    "casbin_rules",
    "permissions",
    "user_roles",
    "data_scopes",
    "audit_events",
    "audit_logs",
    "approval_process_definitions",
    "approval_node_definitions",
    "approval_transition_definitions",
    "demo_master_records",
    "parties",
    "party_revisions",
    "party_contacts",
    "party_addresses",
    "party_bank_accounts",
    "party_tax_profiles",
    "customer_accounts",
    "customer_assignments",
    "customer_profile_commands",
    "supplier_accounts",
    "supplier_commercial_profile_revisions",
    "supplier_capabilities",
    "supplier_capability_revisions",
    "supplier_qualifications",
    "supplier_qualification_revisions",
    "supplier_qualification_capabilities",
    "supplier_rating_revisions",
    "supplier_profile_commands",
    "products",
    "product_revisions",
    "product_revision_medias",
    "skus",
    "sku_revisions",
    "warehouses",
    "warehouse_revisions",
    "product_brands",
    "product_categories",
    "unit_of_measures",
    "org_units",
    "org_memberships",
    "org_management_assignments",
    "org_revisions",
    "org_changes",
    "document_number_counters",
];

/// 返回外键图。
pub(super) fn edges() -> &'static [Edge] {
    EDGES
}

/// 返回按主键收回的集合。
pub(super) fn id_pulls() -> &'static [IdPull] {
    ID_PULLS
}

/// 主数据、账号、审批定义、组织和计数器不允许被衍生删除硬删。
pub(super) fn deletion_allowed(collection: &str) -> bool {
    !collection.starts_with("system.") && !KEPT.contains(&collection)
}

#[cfg(test)]
mod tests {
    use super::{deletion_allowed, edges, id_pulls};

    #[test]
    fn graph_keeps_master_aggregates_and_definition_rows() {
        for name in [
            "supplier_qualifications",
            "supplier_capabilities",
            "supplier_profile_commands",
            "supplier_accounts",
            "customer_accounts",
            "products",
            "skus",
            "warehouses",
            "parties",
            "approval_process_definitions",
            "accounts",
            "org_memberships",
            "document_number_counters",
        ] {
            assert!(!deletion_allowed(name), "{name}");
            assert!(edges().iter().all(|edge| edge.collection != name));
            assert!(id_pulls().iter().all(|pull| pull.collection != name));
        }
    }

    #[test]
    fn order_lines_follow_the_parent_document_instead_of_master_ids() {
        let lines = edges().iter().find(|edge| edge.collection == "sales_order_lines").unwrap();
        assert_eq!(lines.field, "sales_order_id");
        assert!(lines.seed.is_none());
        assert!(edges().iter().any(|edge| {
            edge.collection == "sales_order_goods_service_line_revisions"
                && edge.field == "sku_id"
                && edge.lift == Some("revision_line_id")
        }));
    }

    #[test]
    fn every_named_collection_can_be_deleted() {
        for edge in edges() {
            assert!(deletion_allowed(edge.collection), "{}", edge.collection);
        }
        for pull in id_pulls() {
            assert!(deletion_allowed(pull.collection), "{}", pull.collection);
        }
    }
}
