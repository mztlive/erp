/** 组装采购二次确认队列返回链接，保留当前查询参数。 */
export function buildReturnHref(searchParams: URLSearchParams) {
    const qs = searchParams.toString()
    return qs ? `/procurement/confirm?${qs}` : "/procurement/confirm"
}

/** W05 销售单详情跳转链接，携带 W07 来源与返回地址。 */
export function w05Href(
    salesOrderId: string,
    returnTo: string,
    workItemId?: string,
) {
    const params = new URLSearchParams({
        from: "W07",
        returnTo,
    })
    if (workItemId) params.set("sourceWorkItemId", workItemId)
    return `/sales/orders/${salesOrderId}?${params.toString()}`
}
