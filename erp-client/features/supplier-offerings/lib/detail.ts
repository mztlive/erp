import { formatDateTime, unixSecondsToIsoOrEmpty } from "@/lib/datetime"
import type { SupplierOfferingView } from "../types"

const LIST_PATH = "/procurement/supplier-offerings"

/** 校验同站安全返回路径（支持供给列表、商品详情、供应商详情、任务队列等）。 */
export function offeringListHref(returnTo?: string | null) {
    if (!returnTo) return LIST_PATH
    const trimmed = returnTo.trim()
    if (!trimmed.startsWith("/") || trimmed.startsWith("//")) return LIST_PATH
    try {
        const url = new URL(trimmed, "https://erp.invalid")
        if (url.origin !== "https://erp.invalid") return LIST_PATH
        url.searchParams.delete("_workspace_frame")
        return `${url.pathname}${url.search}${url.hash}`
    } catch {
        return LIST_PATH
    }
}

export function resolveOfferingBackContext(rawReturnTo?: string | null): {
    href: string
    label: string
    hasReturnTo: boolean
    taskMode: boolean
} {
    const trimmed = rawReturnTo?.trim()
    const hasCustomReturn = Boolean(
        trimmed && trimmed.startsWith("/") && !trimmed.startsWith("//"),
    )
    const href = offeringListHref(rawReturnTo)
    const [pathname = "", queryString = ""] = href.split("?")
    const taskMode = Boolean(new URLSearchParams(queryString).get("workItemId"))

    if (taskMode) {
        return {
            href,
            label: "供应停止核对",
            hasReturnTo: true,
            taskMode: true,
        }
    }
    if (!hasCustomReturn) {
        return {
            href: LIST_PATH,
            label: "供应商供给",
            hasReturnTo: false,
            taskMode: false,
        }
    }
    if (pathname.startsWith("/master-data/products/")) {
        return {
            href,
            label: "返回商品资料",
            hasReturnTo: true,
            taskMode: false,
        }
    }
    if (pathname.startsWith("/master-data/suppliers/")) {
        return {
            href,
            label: "返回供应商资料",
            hasReturnTo: true,
            taskMode: false,
        }
    }
    if (pathname === LIST_PATH) {
        return {
            href,
            label: "供应商供给",
            hasReturnTo: true,
            taskMode: false,
        }
    }
    return {
        href,
        label: "返回上一页",
        hasReturnTo: true,
        taskMode: false,
    }
}

export function offeringDetailHref(id: string, returnTo: string) {
    return `${LIST_PATH}/${encodeURIComponent(id)}?${new URLSearchParams({ returnTo: offeringListHref(returnTo) })}`
}

/** 日期只用于展示条款期间，不据此推导下单资格。失效日当天起不再有效。 */
export function offeringValidity(
    terms: Pick<SupplierOfferingView, "valid_from" | "valid_to">,
) {
    if (!terms.valid_from) return "未维护有效期"
    return terms.valid_to
        ? `${terms.valid_from} 起，${terms.valid_to} 失效`
        : `${terms.valid_from} 起长期有效`
}

export function offeringTime(value?: number | null) {
    if (value == null) return "未提供"
    const iso = unixSecondsToIsoOrEmpty(value)
    if (!iso) return "未提供"
    return formatDateTime(iso, "full")
}
