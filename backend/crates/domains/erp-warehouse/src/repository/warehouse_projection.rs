//! 仓库列表投影字段：列表接口只取必要字段，敏感字段不进入投影。

use mongodb::bson::{Document, doc};

/// 仓库列表投影字段。
pub(super) fn warehouse_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_code": 1,
        "status": 1,
        "inbound_handler_user_id": 1,
        "outbound_handler_user_id": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 仓库修订列表投影字段（敏感字段密文/指纹不进入投影）。
pub(super) fn warehouse_revision_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "revision_no": 1,
        "name": 1,
        "region_code": 1,
        "effective_from": 1,
        "effective_to": 1,
        "change_reason": 1,
        "version": 1,
        "created_at": 1,
    }
}

/// 仓库-SKU 预警策略列表投影字段。
pub(super) fn warehouse_sku_policy_projection() -> Document {
    doc! {
        "id": 1,
        "warehouse_id": 1,
        "sku_id": 1,
        "minimum_available_quantity": 1,
        "status": 1,
        "effective_from": 1,
        "effective_to": 1,
        "version": 1,
        "created_at": 1,
    }
}
