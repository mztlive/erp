/** W07 队列筛选契约。 */

export type QueueFilters = {
    scope: "mine" | "team"
    due?: "active" | "today" | "overdue"
    sort?: "due_at" | "submitted_at" | "priority"
    orderNo?: string
    currentWorkItemId?: string
    /** URL / 跨工作面回跳令牌；不得直接当作本页列表的 queue_context_id 提交。 */
    queueContextId?: string
}

const SERVER_QUEUE_CONTEXT_ID = /^[0-9a-f]{64}$/i

/**
 * 判断值是否为服务端按当前查询重算的队列上下文。
 * W02 导航哈希也是 64 位 hex，因此 URL 上的该字段仍不得直接提交；
 * 本函数只用来拦截 `queue:procurement-confirmation:*` 这类客户端占位符。
 */
export function isServerIssuedQueueContextId(
    value?: string | null,
): value is string {
    return typeof value === "string" && SERVER_QUEUE_CONTEXT_ID.test(value)
}
