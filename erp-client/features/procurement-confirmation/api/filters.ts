/** W07 队列筛选契约。 */

export type QueueFilters = {
    scope: "mine" | "team"
    due?: "active" | "today" | "overdue"
    sort?: "due_at" | "submitted_at" | "priority"
    orderNo?: string
    currentWorkItemId?: string
    queueContextId?: string
}
