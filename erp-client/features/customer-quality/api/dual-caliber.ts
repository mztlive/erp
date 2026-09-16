/**
 * S3-05 M10 双口径真实 HTTP。
 * 当前口径：`GET /admin/customer-quality/current`；
 * 历史口径：`GET /admin/customer-quality/history`（冻结归属，不用现任回填）。
 * 导出各走本口径 exports，请求体与列表同筛选并携带范围版本。
 */

import { apiGet, apiPost } from "@/lib/api"

import type {
    CurrentQualityQuery,
    CurrentQualityView,
    HistoryQualityQuery,
    HistoryQualityView,
    QualityExport,
} from "@/features/customer-quality/dual-types"

function csvParam(values?: readonly string[]): string | undefined {
    if (!values || values.length === 0) return undefined
    return values.join(",")
}

function currentToParams(query: CurrentQualityQuery): Record<string, unknown> {
    return {
        from: query.from,
        to: query.to,
        owner_user_ids: csvParam(query.ownerUserIds),
        org_unit_ids: csvParam(query.orgUnitIds),
        include_descendants: query.includeDescendants ?? undefined,
        customer_id: query.customerId,
        owner_group: query.ownerGroup,
        q: query.q,
        dimension: query.dimension,
        sort: query.sort,
        scope_version: query.scopeVersion,
        page: query.page,
        page_size: query.pageSize,
    }
}

function historyToParams(query: HistoryQualityQuery): Record<string, unknown> {
    return {
        from: query.from,
        to: query.to,
        attribution_user_ids: csvParam(query.attributionUserIds),
        attribution_org_unit_ids: csvParam(query.attributionOrgUnitIds),
        attribution_group: query.attributionGroup,
        customer_id: query.customerId,
        q: query.q,
        dimension: query.dimension,
        sort: query.sort,
        scope_version: query.scopeVersion,
        page: query.page,
        page_size: query.pageSize,
    }
}

export async function fetchCurrentQuality(
    query: CurrentQualityQuery,
): Promise<CurrentQualityView> {
    return apiGet<CurrentQualityView>(
        "/admin/customer-quality/current",
        currentToParams(query),
    )
}

export async function fetchHistoryQuality(
    query: HistoryQualityQuery,
): Promise<HistoryQualityView> {
    return apiGet<HistoryQualityView>(
        "/admin/customer-quality/history",
        historyToParams(query),
    )
}

/** 导出重读全部匹配行；客户端只提交筛选与版本，不提供金额。 */
export async function exportCurrentQuality(
    query: CurrentQualityQuery,
): Promise<QualityExport> {
    return apiPost<QualityExport>(
        "/admin/customer-quality/current/exports",
        currentToParams(query),
    )
}

export async function exportHistoryQuality(
    query: HistoryQualityQuery,
): Promise<QualityExport> {
    return apiPost<QualityExport>(
        "/admin/customer-quality/history/exports",
        historyToParams(query),
    )
}

/** 服务端 CSV 下载：文件名与内容均来自服务端，客户端不改写。 */
export function downloadQualityCsv(csvContent: string, fileName: string) {
    const blob = new Blob(["\uFEFF" + csvContent], {
        type: "text/csv;charset=utf-8",
    })
    const url = URL.createObjectURL(blob)
    const anchor = document.createElement("a")
    anchor.href = url
    anchor.download = fileName
    document.body.appendChild(anchor)
    anchor.click()
    anchor.remove()
    URL.revokeObjectURL(url)
}
