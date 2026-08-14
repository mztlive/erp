import {
    apiGet,
    getApiBaseUrl,
    getToken,
    notifyUnauthorized,
    type ApiError,
} from "@/lib/api"
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
            ownerLabel: c.owner_user_id ?? "—",
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

/**
 * multipart 上传：lib/api 仅 JSON，故用原生 fetch + 鉴权头。
 */
export async function uploadFileAsset(file: File): Promise<BackendFileAsset> {
    const form = new FormData()
    form.append("file", file, file.name)
    form.append("sensitivity_class", "sensitive")
    form.append("retention_class", "long_term")
    form.append("usage", "attachment")

    const headers: Record<string, string> = {}
    const token = getToken()
    if (token) headers.Authorization = `Bearer ${token}`

    const timeoutMs = 60_000
    let res: Response
    try {
        res = await fetch(`${getApiBaseUrl()}/admin/file-assets/upload`, {
            method: "POST",
            headers,
            body: form,
            signal: AbortSignal.timeout(timeoutMs),
        })
    } catch (cause) {
        const err: ApiError = {
            kind: "Network",
            message: "网络请求失败或连接超时",
            cause,
        }
        throw err
    }

    const text = await res.text()
    let parsed: unknown
    try {
        parsed = text ? JSON.parse(text) : null
    } catch (cause) {
        const err: ApiError = {
            kind: "Parse",
            message: "响应数据解析失败",
            cause,
            responseData: text,
        }
        throw err
    }

    const envelope = parsed as {
        success?: boolean
        status?: number
        errorMessage?: string
        data?: BackendFileAsset | null
    } | null

    if (res.status === 401 || envelope?.status === 401) {
        // 与 lib/api/client 一致：清 token 并触发跳转登录
        notifyUnauthorized()
        const err: ApiError = {
            kind: "Auth",
            message: "登录状态已失效，请重新登录",
            status: 401,
            responseData: parsed,
        }
        throw err
    }

    if (!res.ok) {
        const err: ApiError = {
            kind: res.status === 400 ? "Validation" : "Http",
            message:
                envelope?.errorMessage ||
                (res.status === 400
                    ? "请求未通过业务校验"
                    : `请求失败（HTTP ${res.status}）`),
            status: res.status,
            responseData: parsed,
        }
        throw err
    }

    if (envelope && envelope.success === false) {
        const err: ApiError = {
            kind: "Validation",
            message: envelope.errorMessage || "请求未通过业务校验",
            status: envelope.status,
            responseData: envelope,
        }
        throw err
    }

    const data = envelope?.data
    if (!data?.id) {
        const err: ApiError = {
            kind: "Parse",
            message: "上传响应缺少文件资产 ID",
            responseData: parsed,
        }
        throw err
    }
    return data
}
