/**
 * W10 库存台账 · 后端 DTO 形状（snake_case，来自 services inventory/dto.rs）。
 * 仅内部映射层使用；对外契约类型见 ../types。
 */

import type { DocumentApprovalViewDto } from "@/features/approval-workflow/types"
import type { Page } from "@/lib/api"

export type BackendPage<T> = Page<T>

export type BackendStockBalance = {
    id: string
    warehouse_id: string
    warehouse_code: string
    warehouse_name: string
    sku_id: string
    sku_code: string
    sku_name: string
    spec_summary?: string | null
    on_hand_quantity: string
    reserved_quantity: string
    available_quantity: string
    version: number
    last_movement_id?: string | null
    last_movement_at?: number | null
    last_movement_type?: string | null
    has_active_reservation: boolean
}

export type BackendStockMovement = {
    id: string
    warehouse_id: string
    sku_id: string
    movement_type: string
    direction: string
    quantity: string
    source_document_id: string
    source_document_no?: string | null
    source_line_id?: string | null
    occurred_at: number
    recorded_at: number
    recorded_by?: string | null
}

export type BackendStockReservation = {
    id: string
    warehouse_id: string
    sku_id: string
    sales_order_line_id: string
    reserved_quantity: string
    consumed_quantity: string
    released_quantity: string
    status: string
    version: number
}

export type BackendStockAdjustment = {
    id: string
    adjustment_no: string
    warehouse_id: string
    reason_type: string
    status: string
    prepared_by: string
    reviewed_by?: string | null
    finance_reviewed_by?: string | null
    note?: string | null
    occurred_at?: number | null
    version: number
    created_at: number
}

export type BackendStockAdjustmentLine = {
    id: string
    sku_id: string
    quantity: string
    direction: string
}

export type BackendStockAdjustmentDetail = {
    adjustment: BackendStockAdjustment
    lines: BackendStockAdjustmentLine[]
    posted_movements: BackendStockMovement[]
    approval?: DocumentApprovalViewDto | null
}

export type BackendStockBalanceDetail = {
    balance: BackendStockBalance
    recent_movements: BackendStockMovement[]
    active_reservations: BackendStockReservation[]
    pending_adjustments: BackendStockAdjustment[]
}

export type BackendWarehouse = {
    id: string
    warehouse_code: string
    status?: string
    created_at?: number
    version?: number
}

export type BackendBackgroundJob = {
    id: string
    job_no: string
    job_type: string
    status: string
    total_count: number
    processed_count: number
    success_count: number
    result_file_asset_id?: string | null
    created_at: number
}

/** 导出任务视图（保留签名供页面消费）。 */
export type InventoryExportJob = {
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    total: number
    completed: number
    filterSummary: string
    createdAt: string
    downloadLabel?: string
}
