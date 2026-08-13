/**
 * W18 导入与期初 · 真实 HTTP API（P4 F8）。
 * 后端域：legacy_import（/admin/legacy-import-*）。
 * 导出签名保持与 hooks/queries.ts 一致；Page 形状经 mappers.ts 适配为 feature view。
 */

import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"
import {
    type BackendBatchDetail,
    type BackendBatchListItem,
    type BackendConfirmation,
    type BackendRow,
    buildBatchView,
    environmentFromQuery,
    instantToIso,
    mapIssueCode,
    mapObjectType,
    mapRowStatus,
    toBackendStatusFilter,
    toListItem,
} from "@/features/import-opening/api/mappers"
import type {
    ImportBatchListQuery,
    ImportBatchListView,
    ImportBatchView,
    ImportIssuePage,
    ImportIssueQuery,
} from "@/features/import-opening/types"

// ─── API ─────────────────────────────────────────────────────────────────────

export async function fetchImportBatchList(
    query: ImportBatchListQuery,
): Promise<ImportBatchListView> {
    const env = environmentFromQuery(query.environment)
    const backendStatus = toBackendStatusFilter(query.status)

    const page = await apiGet<Page<BackendBatchListItem>>(
        "/admin/legacy-import-batches",
        {
            page: query.page,
            page_size: query.pageSize,
            batch_no: query.q?.trim() || undefined,
            status: backendStatus,
        },
    )

    let rows = page.items.map((b) => toListItem(b, env))
    if (query.objectType && query.objectType !== "all") {
        const ot = query.objectType
        rows = rows.filter((r) => r.sourceObjectSet.includes(ot))
    }

    // 指标：再拉一页较大集合做计数（后端无 metrics 聚合端点）
    const metricsSource = await apiGet<Page<BackendBatchListItem>>(
        "/admin/legacy-import-batches",
        { page: 1, page_size: 100 },
    )
    const all = metricsSource.items
    const metrics = {
        pendingValidate: all.filter((b) =>
            ["pending_validation", "validating"].includes(b.status),
        ).length,
        pendingConfirm: all.filter((b) => b.status === "pending_confirmation")
            .length,
        applying: all.filter((b) => b.status === "importing").length,
        failedOrPartial: all.filter((b) =>
            ["partial_failed", "failed"].includes(b.status),
        ).length,
    }

    const asOf =
        all[0] != null
            ? instantToIso(all[0].created_at)
            : instantToIso(Math.floor(Date.now() / 1000))

    return {
        metrics,
        rows,
        totalCount: page.total,
        queriedAt: asOf,
    }
}

export async function fetchImportBatchDetail(input: {
    batchId: string
}): Promise<ImportBatchView | null> {
    let batch: BackendBatchDetail
    try {
        batch = await apiGet<BackendBatchDetail>(
            `/admin/legacy-import-batches/${input.batchId}`,
        )
    } catch {
        return null
    }

    const confPage = await apiGet<Page<BackendConfirmation>>(
        "/admin/legacy-import-confirmations",
        {
            page: 1,
            page_size: 50,
            batch_id: input.batchId,
        },
    )

    // environment 后端无字段 — 默认 PRODUCTION 展示（缺口见 evidence）
    return buildBatchView(batch, confPage.items, "PRODUCTION")
}

export async function fetchImportIssues(
    query: ImportIssueQuery,
): Promise<ImportIssuePage> {
    const page = await apiGet<Page<BackendRow>>(
        `/admin/legacy-import-batches/${query.batchId}/rows`,
        {
            page: query.page,
            page_size: query.pageSize,
            source_object_type:
                query.objectType && query.objectType !== "all"
                    ? query.objectType
                    : undefined,
        },
    )

    let rows = page.items
        .map((row) => {
            const rowStatus = mapRowStatus(row)
            if (!rowStatus) return null
            return {
                issueId: row.id,
                batchId: row.batch_id,
                issueCode: mapIssueCode(row.error_code),
                objectType: mapObjectType(row.source_object_type),
                sourceRowNo: Number.parseInt(row.source_row_key, 10) || 0,
                sourceColumnName: row.source_object_type,
                rowStatus,
                errorDetail: row.error_code ?? rowStatus,
                repairable:
                    rowStatus === "FAILED" ||
                    rowStatus === "CONFLICT" ||
                    rowStatus === "PENDING_MAPPING",
            }
        })
        .filter((r): r is NonNullable<typeof r> => r != null)

    if (query.issueCode && query.issueCode !== "all") {
        rows = rows.filter((r) => r.issueCode === query.issueCode)
    }
    if (query.rowStatus && query.rowStatus !== "all") {
        rows = rows.filter((r) => r.rowStatus === query.rowStatus)
    }

    const asOf =
        page.items[0] != null
            ? instantToIso(page.items[0].created_at)
            : instantToIso(Math.floor(Date.now() / 1000))

    return {
        rows,
        totalCount: rows.length,
        issueVersion: `issv-${query.batchId}-${page.page}`,
        queriedAt: asOf,
    }
}
