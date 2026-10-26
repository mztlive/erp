export const RECEIPT_REGISTRATION_ROUTE =
    "/finance/customer-accounts/receipts/new"
const listPath = "/finance/customer-accounts"

/** 返回原列表筛选或来源销售单，拒绝外部地址与登记页循环。 */
export function receiptReturnHref(value?: string | null): string {
    if (!value?.startsWith("/") || value.startsWith("//")) return listPath
    let url: URL
    try {
        url = new URL(value, "http://erp.local")
    } catch {
        return listPath
    }
    if (url.origin !== "http://erp.local") return listPath
    if (url.pathname === listPath) {
        for (const key of ["sessionId", "register", "returnTo", "from"])
            url.searchParams.delete(key)
        return `${url.pathname}${url.search}`
    }
    if (
        /^\/sales\/orders\/[^/]+$/.test(url.pathname) &&
        !url.pathname.endsWith("/new")
    )
        return `${url.pathname}${url.search}${url.hash}`
    return listPath
}

/** 会话身份用于独立工作区标签，列表筛选只保存在返回地址。 */
export function receiptRegistrationHref(
    sessionId: string,
    returnTo?: string | null,
) {
    return `${RECEIPT_REGISTRATION_ROUTE}?${new URLSearchParams({ sessionId, returnTo: receiptReturnHref(returnTo) })}`
}
