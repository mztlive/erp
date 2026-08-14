/** W18 导入与期初 · 单字段/枚举映射纯函数。 */

import type {
    ImportBatchStatus,
    ImportConfirmationView,
    ImportIssueCode,
    ImportObjectCode,
    ImportPipelineStage,
    IssueRowStatus,
} from "@/features/import-opening/types"
import { OBJECT_CODE_LABEL } from "@/features/import-opening/types"
import type {
    BackendBatchListItem,
    BackendConfirmation,
    BackendRow,
} from "./dto"

export function instantToIso(secs: number | null | undefined): string {
    if (secs == null || !Number.isFinite(secs)) return ""
    return new Date(secs * 1000).toISOString()
}

export function mapBatchStatus(s: BackendBatchListItem["status"]): {
    status: ImportBatchStatus
    stage: ImportPipelineStage
} {
    switch (s) {
        case "pending_validation":
            return { status: "RECEIVING", stage: "RECEIVE" }
        case "validating":
            return { status: "VALIDATING", stage: "VALIDATE" }
        case "pending_confirmation":
            return { status: "AWAITING_CONFIRMATION", stage: "CONFIRM" }
        case "ready_to_apply":
            return { status: "READY_TO_APPLY", stage: "CONFIRM" }
        case "importing":
            return { status: "APPLYING", stage: "APPLY" }
        case "completed":
            return { status: "SUCCEEDED", stage: "RESULT" }
        case "partial_failed":
            return { status: "PARTIAL_SUCCESS", stage: "RESULT" }
        case "failed":
            return { status: "FAILED", stage: "RESULT" }
        default:
            return { status: "FAILED", stage: "RESULT" }
    }
}

/** 前端筛选 status → 后端 status（无法精确覆盖的前端细态映射到最近后端态） */
export function toBackendStatusFilter(
    status?: string,
): BackendBatchListItem["status"] | undefined {
    if (!status || status === "all") return undefined
    const map: Record<string, BackendBatchListItem["status"]> = {
        RECEIVING: "pending_validation",
        SCANNING: "pending_validation",
        VALIDATING: "validating",
        TRIAL_READY: "validating",
        AWAITING_CONFIRMATION: "pending_confirmation",
        CONFIRMATION_BLOCKED: "pending_confirmation",
        READY_TO_APPLY: "ready_to_apply",
        APPLYING: "importing",
        PARTIAL_SUCCESS: "partial_failed",
        SUCCEEDED: "completed",
        FAILED: "failed",
        CANCELLED: "failed",
        pending_validation: "pending_validation",
        validating: "validating",
        pending_confirmation: "pending_confirmation",
        ready_to_apply: "ready_to_apply",
        importing: "importing",
        completed: "completed",
        partial_failed: "partial_failed",
        failed: "failed",
    }
    return map[status]
}

export function parseObjectSet(raw: string): ImportObjectCode[] {
    if (!raw.trim()) return []
    return raw
        .split(/[,;|/\s]+/)
        .map((p) => p.trim().toUpperCase())
        .filter(Boolean)
        .map((p) => {
            const known = p as ImportObjectCode
            return known in OBJECT_CODE_LABEL
                ? known
                : ("CUSTOMER" as ImportObjectCode)
        })
}

export function mapScope(raw: string): ImportConfirmationView["scope"] {
    const u = raw.trim().toUpperCase()
    if (u.includes("SALES") || u.includes("销售")) return "SALES"
    if (u.includes("PROCURE") || u.includes("采购")) return "PROCUREMENT"
    if (u.includes("OPERAT") || u.includes("运营")) return "OPERATIONS"
    if (u.includes("WARE") || u.includes("仓储") || u.includes("仓"))
        return "WAREHOUSE"
    if (u.includes("FIN") || u.includes("财务")) return "FINANCE"
    return "OPERATIONS"
}

export function mapConfirmResult(
    status: BackendConfirmation["status"],
): ImportConfirmationView["result"] {
    switch (status) {
        case "CONFIRMED":
            return "CONFIRMED"
        case "REJECTED":
            return "REJECTED"
        case "INVALIDATED":
            return "INVALIDATED"
        default:
            return "PENDING"
    }
}

export function mapIssueCode(code?: string | null): ImportIssueCode {
    if (!code) return "MAPPING_CONFLICT"
    const u = code.toUpperCase()
    const known: ImportIssueCode[] = [
        "CUSTOMER_NOT_FOUND",
        "AMOUNT_PRECISION",
        "BASELINE_DATE_MISMATCH",
        "HISTORY_FLOW_FORBIDDEN",
        "CARD_DRAFT_EXCLUDED",
        "MAPPING_CONFLICT",
        "QUALIFICATION_EXPIRED",
        "STOCK_QTY_INVALID",
    ]
    const hit = known.find((k) => u.includes(k) || k === u)
    return hit ?? "MAPPING_CONFLICT"
}

export function mapObjectType(raw: string): ImportObjectCode {
    const u = raw.trim().toUpperCase()
    if (u in OBJECT_CODE_LABEL) return u as ImportObjectCode
    if (u.includes("CUSTOMER") || u.includes("客户")) return "CUSTOMER"
    if (u.includes("CONTRACT") || u.includes("合同")) return "CONTRACT"
    if (u.includes("SUPPLIER") || u.includes("供应")) return "SUPPLIER"
    if (u.includes("WARE") || u.includes("仓")) return "WAREHOUSE"
    if (u.includes("STOCK") || u.includes("库存")) return "OPENING_STOCK"
    if (u.includes("SKU")) return "SKU"
    if (u.includes("CARD") && u.includes("CAT")) return "CARD_CATEGORY"
    if (u.includes("SALES")) return "CARD_SALES_ORDER"
    if (u.includes("AR") || u.includes("应收")) return "CARD_OPENING_AR"
    return "CUSTOMER"
}

export function mapRowStatus(row: BackendRow): IssueRowStatus | null {
    if (row.mapping_status === "conflict") return "CONFLICT"
    if (row.mapping_status === "pending_mapping") return "PENDING_MAPPING"
    if (row.import_status === "failed") return "FAILED"
    if (row.import_status === "skipped") return "SKIPPED"
    if (row.parse_status === "invalid") return "FAILED"
    return null
}
