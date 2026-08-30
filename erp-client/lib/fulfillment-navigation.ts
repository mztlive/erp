/** 返回 W01 履约任务队列；query 只用于服务端安全摘要字段检索。 */
export function fulfillmentTasksHref(query?: string): string {
    const params = new URLSearchParams({
        family: "fulfillment",
        type: "FULFILLMENT_OPERATION",
    })
    if (query?.trim()) params.set("q", query.trim())
    return `/workspace?${params.toString()}`
}
