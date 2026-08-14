/**
 * W25 商城消费订单 · 真实 HTTP API（queryFn / mutationFn）
 * 路径：/admin/mall-orders、/admin/mall-order-facts、/admin/background-jobs
 * 后端 snake_case + 秒级时间戳 → 前端 camelCase + ISO，适配在 mapping.ts / detail-mapping.ts。
 */

import { apiGet, apiPost, type Page } from "@/lib/api"
import type {
    EmptyReason,
    ExportCommand,
    ExportJobResult,
    MallConsumptionOrderListQuery,
    MallConsumptionOrderListResult,
    MallConsumptionOrderView,
    SalesOrderConsumptionSummary,
} from "@/features/mall-consumption-orders/types"
import { BOUNDARY_NOTICE } from "./constants"
import { mapDetail } from "./detail-mapping"
import {
    attributionToBackend,
    dateToUnixEnd,
    dateToUnixStart,
    emptyMetrics,
    filterSummary,
    mapListRow,
    tsToIso,
} from "./mapping"
import type { BackendBackgroundJob, BackendDetail, BackendListRow } from "./wire-types"

/**
 * 销售单协同摘要。后端 P3 未提供按 sales_order_id 聚合接口 → 返回空摘要。
 * 缺口：GET /admin/mall-orders?origin_sales_order_id= 或专用 summary 端点。
 */
export async function fetchSalesOrderConsumptionSummary(
    salesOrderId: string,
): Promise<SalesOrderConsumptionSummary> {
    return {
        salesOrderId,
        orderCount: 0,
        paidAmount: "0.00",
        refundedAmount: "0.00",
        restoredBalanceAmount: "0.00",
    }
}

export async function fetchConsumptionOrderList(
    query: MallConsumptionOrderListQuery,
): Promise<MallConsumptionOrderListResult> {
    const queriedAt = new Date().toISOString()
    const pageSize = Math.max(1, query.pageSize ?? 8)
    const page = Math.max(1, query.page ?? 1)

    // 期间门禁：未选完整起止时不请求全量（与 W25 一致）
    if (!query.occurredFrom || !query.occurredTo) {
        return {
            rows: [],
            pageInfo: { page: 1, pageSize, total: 0 },
            metrics: [],
            malls: [],
            filterSummary: "请先选择记录发生起止时间后查询",
            emptyReason: "FILTER_EMPTY",
            hasModulePermission: true,
            hasDataScope: true,
            permissionVersion: "server",
            dataScopeVersion: "server",
            factWatermark: queriedAt,
            queriedAt,
            boundaryNotice: BOUNDARY_NOTICE,
        }
    }

    const sortParts = (query.sort ?? "").split(".")
    const sortBy =
        sortParts[0] === "occurredAt"
            ? "paid_at"
            : sortParts[0] === "paidAt"
              ? "paid_at"
              : "paid_at"
    const sortDir = sortParts[1] === "asc" ? "asc" : "desc"

    const pageRes = await apiGet<Page<BackendListRow>>("/admin/mall-orders", {
        page,
        page_size: pageSize,
        q: query.q?.trim() || undefined,
        mall_id: query.mallIds?.[0],
        fulfillment_chain: query.fulfillmentChains?.[0],
        attribution_status: query.attributionStatuses?.[0]
            ? attributionToBackend(query.attributionStatuses[0])
            : undefined,
        paid_at_from: dateToUnixStart(query.occurredFrom),
        paid_at_to: dateToUnixEnd(query.occurredTo),
        sort_by: sortBy,
        sort_dir: sortDir,
    })

    const rows = (pageRes.items ?? []).map(mapListRow)
    const total = pageRes.total ?? 0
    const mallsMap = new Map<string, string>()
    for (const r of rows) mallsMap.set(r.mallId, r.mallName)

    let emptyReason: EmptyReason | undefined
    if (total === 0) {
        emptyReason =
            Boolean(query.q?.trim()) ||
            Boolean(query.mallIds?.length) ||
            Boolean(query.fulfillmentChains?.length) ||
            Boolean(query.attributionStatuses?.length)
                ? "FILTER_EMPTY"
                : "NO_DATA"
    }

    return {
        rows,
        pageInfo: {
            page: pageRes.page ?? page,
            pageSize: pageRes.page_size ?? pageSize,
            total,
        },
        // 指标聚合端点未交付 → 零值（backend_gap）
        metrics: emptyMetrics(),
        malls: Array.from(mallsMap.entries()).map(([id, name]) => ({
            id,
            name,
        })),
        filterSummary: filterSummary(query, total),
        emptyReason,
        hasModulePermission: true,
        hasDataScope: true,
        permissionVersion: "server",
        dataScopeVersion: "server",
        factWatermark: queriedAt,
        queriedAt,
        boundaryNotice: BOUNDARY_NOTICE,
    }
}

export async function fetchConsumptionOrderDetail(
    mallOrderId: string,
): Promise<MallConsumptionOrderView | null> {
    try {
        const detail = await apiGet<BackendDetail>(
            `/admin/mall-orders/${encodeURIComponent(mallOrderId)}`,
        )
        return mapDetail(detail)
    } catch (err) {
        const status =
            err && typeof err === "object" && "status" in err
                ? (err as { status?: number }).status
                : undefined
        if (status === 404) return null
        throw err
    }
}

export async function createConsumptionOrderExportJob(
    command: ExportCommand,
): Promise<ExportJobResult> {
    const job = await apiPost<BackendBackgroundJob>("/admin/background-jobs", {
        job_no: `EXP-W25-${command.requestId.slice(-12)}`,
        job_type: "export",
        domain_job_type: "mall_consumption_order_export",
        selection_snapshot_id: command.selectionSnapshotId || null,
        request_id: command.requestId,
        total_count: Math.max(1, command.rowCount || 1),
        items: [
            {
                object_type: "mall_order",
                object_id: command.selectionSnapshotId || command.requestId,
            },
        ],
    })

    return {
        jobId: job.id,
        requestId: command.requestId,
        rowCount: command.rowCount,
        permissionVersion: "server",
        fieldSetId: command.fieldSetId,
        maskDisclaimer:
            "导出使用系统筛选结果与字段权限打码：地址、手机号、完整支付引用、卡号/卡密、未授权成本金额不会以明文写入文件。下载时重新鉴权。",
        expiresAt: job.result_expires_at
            ? tsToIso(job.result_expires_at)
            : new Date(Date.now() + 24 * 60 * 60 * 1000).toISOString(),
        downloadLabel: `商城消费订单_${job.job_no ?? job.id}.csv`,
        status: job.status === "completed" ? "succeeded" : "queued",
    }
}
