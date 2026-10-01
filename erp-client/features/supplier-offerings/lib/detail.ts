import type { SupplierOfferingView } from "../types"

const LIST_PATH = "/procurement/supplier-offerings"

/** 返回入口只接受同站供给列表，保留筛选、页码与来源上下文。 */
export function offeringListHref(returnTo?: string | null) {
    if (!returnTo) return LIST_PATH
    try {
        const url = new URL(returnTo, "https://erp.invalid")
        return url.origin === "https://erp.invalid" &&
            url.pathname === LIST_PATH
            ? `${url.pathname}${url.search}`
            : LIST_PATH
    } catch {
        return LIST_PATH
    }
}

export function offeringDetailHref(id: string, returnTo: string) {
    return `${LIST_PATH}/${encodeURIComponent(id)}?${new URLSearchParams({ returnTo: offeringListHref(returnTo) })}`
}

/** 日期只用于展示条款期间，不据此推导下单资格。 */
export function offeringValidity(
    terms: Pick<SupplierOfferingView, "valid_from" | "valid_to">,
) {
    if (!terms.valid_from) return "未维护有效期"
    return `${terms.valid_from} 至 ${terms.valid_to || "长期有效"}`
}

export function offeringTime(value?: number | null) {
    if (value == null) return "未提供"
    return new Intl.DateTimeFormat("zh-CN", {
        timeZone: "Asia/Shanghai",
        year: "numeric",
        month: "2-digit",
        day: "2-digit",
        hour: "2-digit",
        minute: "2-digit",
    }).format(new Date(value * 1000))
}
