/**
 * 演示主数据 HTTP API。
 */
import { apiDelete, apiGet, apiPost } from "@/lib/api/client"

export type DemoCounts = {
    unit: number
    brand: number
    category: number
    warehouse: number
    customer: number
    supplier: number
    product: number
}

export type DemoStatus = {
    enabled: boolean
    planned: DemoCounts
    active: DemoCounts
    removed: DemoCounts
}

export type DemoChunkReport = {
    next_cursor: number
    total_steps: number
    done: boolean
    created: number
    restored: number
    skipped: number
    removed: number
    derived_removed: number
    notices: string[]
}

export type DemoFoundationReport = {
    accounts_created: number
    accounts_existing: number
    approvals_published: number
    approvals_existing: number
    approvals_mismatched: number
    notices: string[]
}

const REQUEST_TIMEOUT_MS = 60_000

export function fetchDemoMasterDataStatus(): Promise<DemoStatus> {
    return apiGet<DemoStatus>("/admin/demo-master-data")
}

export function ensureDemoFoundation(): Promise<DemoFoundationReport> {
    return apiPost<DemoFoundationReport>(
        "/admin/demo-master-data/foundation",
        {},
        { timeoutMs: REQUEST_TIMEOUT_MS },
    )
}

export function applyDemoMasterData(cursor: number): Promise<DemoChunkReport> {
    return apiPost<DemoChunkReport>(
        "/admin/demo-master-data",
        { cursor },
        { timeoutMs: REQUEST_TIMEOUT_MS },
    )
}

export function removeDemoMasterData(purge: boolean): Promise<DemoChunkReport> {
    const purgeFlag = purge ? "true" : "false"
    return apiDelete<DemoChunkReport>(
        `/admin/demo-master-data?purge=${purgeFlag}`,
        { timeoutMs: REQUEST_TIMEOUT_MS },
    )
}
