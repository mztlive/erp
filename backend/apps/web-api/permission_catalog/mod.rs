//! 显式权限目录：构建生成器与运行时共同编译的普通 Rust 数据。
mod types;
pub use types::{PermissionBinding, PermissionDef, PermissionGroupDef, PermissionModule};

pub mod access_control;
pub mod admin;
pub mod approval_instance;
pub mod approval_process;
pub mod bulk_job;
pub mod catalog;
pub mod contract;
pub mod cost;
pub mod customer;
pub mod customer_quality;
pub mod demo_master_data;
pub mod document_registry;
pub mod file_asset;
pub mod financial_files;
pub mod fulfillment;
pub mod groups;
pub mod integration_ops;
pub mod inventory;
pub mod legacy_import;
pub mod organization;
pub mod party;
pub mod payable;
pub mod purchase_order;
pub mod purchase_payment_request;
pub mod receivable;
pub mod returns;
pub mod sales_inquiry;
pub mod sales_order;
pub mod sales_review;
pub mod sales_selection;
pub mod source_registry;
pub mod supplier;
pub mod supplier_api;
pub mod supplier_deposit;
pub mod supplier_fulfillment;
pub mod supplier_offering;
pub mod supplier_portal;
pub mod supplier_settlement;
pub mod warehouse;
pub mod work_item;

/// 目录枚举入口；新增模块必须在此登记。
pub const MODULES: &[PermissionModule] = &[
    PermissionModule { name: "access_control", entries: access_control::ENTRIES },
    PermissionModule { name: "admin", entries: admin::ENTRIES },
    PermissionModule { name: "approval_instance", entries: approval_instance::ENTRIES },
    PermissionModule { name: "approval_process", entries: approval_process::ENTRIES },
    PermissionModule { name: "bulk_job", entries: bulk_job::ENTRIES },
    PermissionModule { name: "catalog", entries: catalog::ENTRIES },
    PermissionModule { name: "contract", entries: contract::ENTRIES },
    PermissionModule { name: "cost", entries: cost::ENTRIES },
    PermissionModule { name: "customer", entries: customer::ENTRIES },
    PermissionModule { name: "customer_quality", entries: customer_quality::ENTRIES },
    PermissionModule { name: "demo_master_data", entries: demo_master_data::ENTRIES },
    PermissionModule { name: "document_registry", entries: document_registry::ENTRIES },
    PermissionModule { name: "file_asset", entries: file_asset::ENTRIES },
    PermissionModule { name: "financial_files", entries: financial_files::ENTRIES },
    PermissionModule { name: "fulfillment", entries: fulfillment::ENTRIES },
    PermissionModule { name: "integration_ops", entries: integration_ops::ENTRIES },
    PermissionModule { name: "inventory", entries: inventory::ENTRIES },
    PermissionModule { name: "legacy_import", entries: legacy_import::ENTRIES },
    PermissionModule { name: "organization", entries: organization::ENTRIES },
    PermissionModule { name: "party", entries: party::ENTRIES },
    PermissionModule { name: "payable", entries: payable::ENTRIES },
    PermissionModule { name: "purchase_order", entries: purchase_order::ENTRIES },
    PermissionModule { name: "purchase_payment_request", entries: purchase_payment_request::ENTRIES },
    PermissionModule { name: "receivable", entries: receivable::ENTRIES },
    PermissionModule { name: "returns", entries: returns::ENTRIES },
    PermissionModule { name: "sales_inquiry", entries: sales_inquiry::ENTRIES },
    PermissionModule { name: "sales_order", entries: sales_order::ENTRIES },
    PermissionModule { name: "sales_review", entries: sales_review::ENTRIES },
    PermissionModule { name: "sales_selection", entries: sales_selection::ENTRIES },
    PermissionModule { name: "source_registry", entries: source_registry::ENTRIES },
    PermissionModule { name: "supplier", entries: supplier::ENTRIES },
    PermissionModule { name: "supplier_api", entries: supplier_api::ENTRIES },
    PermissionModule { name: "supplier_deposit", entries: supplier_deposit::ENTRIES },
    PermissionModule { name: "supplier_fulfillment", entries: supplier_fulfillment::ENTRIES },
    PermissionModule { name: "supplier_offering", entries: supplier_offering::ENTRIES },
    PermissionModule { name: "supplier_portal", entries: supplier_portal::ENTRIES },
    PermissionModule { name: "supplier_settlement", entries: supplier_settlement::ENTRIES },
    PermissionModule { name: "warehouse", entries: warehouse::ENTRIES },
    PermissionModule { name: "work_item", entries: work_item::ENTRIES },
];
