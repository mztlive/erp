import { apiGet, type ApiError } from "@/lib/api"
import { PAYMENT_TERM_OPTIONS } from "@/lib/business-options"
import type {
    ContractAction,
    ContractAttachmentView,
    ContractListRow,
    ContractStatus,
} from "@/features/contracts/types"
import type {
    BackendCustomerDetail,
    BackendFileAsset,
    BackendPartyView,
} from "@/features/contracts/api/wire-types"

export function isApiError(error: unknown): error is ApiError {
    return (
        typeof error === "object" &&
        error !== null &&
        "kind" in error &&
        "message" in error
    )
}

export function tsToIso(seconds: number | undefined | null): string {
    if (seconds == null || !Number.isFinite(seconds)) {
        return new Date().toISOString()
    }
    return new Date(seconds * 1000).toISOString()
}

export function asContractStatus(raw: string): ContractStatus {
    if (raw === "TERMINATED" || raw === "EXPIRED" || raw === "EFFECTIVE") {
        return raw
    }
    return "EFFECTIVE"
}

export function daysUntil(dateStr: string): number {
    const end = new Date(`${dateStr}T23:59:59`)
    const now = new Date()
    return Math.ceil((end.getTime() - now.getTime()) / (24 * 60 * 60 * 1000))
}

export function isExpiringWithin30Days(
    status: ContractStatus,
    validTo?: string | null,
): boolean {
    if (status !== "EFFECTIVE" || !validTo) return false
    const d = daysUntil(validTo)
    return d >= 0 && d <= 30
}

export function paymentTermCodeFromLabel(label: string): string {
    const found = PAYMENT_TERM_OPTIONS.find(
        (o) => o.label === label || o.value === label,
    )
    return found?.value ?? "CONTRACT"
}

export function paymentTermDays(code: string): number | undefined {
    if (code.includes("15")) return 15
    if (code.includes("30")) return 30
    return undefined
}

export function mapScanStatus(
    status: string,
): ContractAttachmentView["securityState"] {
    switch (status) {
        case "passed":
            return "done"
        case "quarantined":
        case "rejected":
            return "quarantined"
        default:
            return "processing"
    }
}

export function baseActions(status: ContractStatus): {
    allowedActions: ContractAction[]
    actionBlockers: ContractListRow["actionBlockers"]
    selectable: boolean
    selectableBlocker?: string
} {
    if (status === "EFFECTIVE") {
        return {
            allowedActions: [
                "UPLOAD_CONTRACT_PDF",
                "PRINT",
                "CREATE_SALES_ORDER",
                "EXPORT",
            ],
            actionBlockers: [],
            selectable: true,
        }
    }
    if (status === "EXPIRED") {
        return {
            allowedActions: ["PRINT", "EXPORT"],
            actionBlockers: [
                {
                    action: "CREATE_SALES_ORDER",
                    code: "CONTRACT_EXPIRED",
                    message: "合同已到期，不能引用到新销售单",
                },
            ],
            selectable: false,
            selectableBlocker: "合同已到期",
        }
    }
    return {
        allowedActions: ["PRINT", "EXPORT"],
        actionBlockers: [
            {
                action: "CREATE_SALES_ORDER",
                code: "CONTRACT_TERMINATED",
                message: "合同已终止，不能引用到新销售单",
            },
        ],
        selectable: false,
        selectableBlocker: "合同已终止",
    }
}

export async function loadCustomerBrief(customerId: string): Promise<{
    customerId: string
    customerNo: string
    displayName: string
    partyId: string
    ownerLabel: string
} | null> {
    try {
        const c = await apiGet<BackendCustomerDetail>(
            `/admin/customers/${customerId}`,
        )
        return {
            customerId: c.id,
            customerNo: c.customer_no,
            displayName: c.legal_name?.trim() || c.customer_no,
            partyId: c.party_id,
            ownerLabel:
                c.owner_user_name?.trim() || c.owner_user_id?.trim() || "—",
        }
    } catch {
        return null
    }
}

export async function loadPartyName(partyId: string): Promise<string> {
    try {
        const p = await apiGet<BackendPartyView>(`/admin/parties/${partyId}`)
        return p.party_no
    } catch {
        return partyId
    }
}

export async function loadFileAsset(
    fileId: string,
): Promise<BackendFileAsset | null> {
    try {
        return await apiGet<BackendFileAsset>(`/admin/file-assets/${fileId}`)
    } catch {
        return null
    }
}
