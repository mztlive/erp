/** 四个事项页面各有固定路由；查询条件只负责当前页面的筛选。 */
export const WORKSPACE_QUEUE_ROUTES = {
    inbox: { href: "/workspace/tasks", title: "待我处理" },
    started: { href: "/workspace/approvals", title: "我发起的审批" },
    history: { href: "/workspace/history", title: "处理记录" },
    copied: { href: "/workspace/copies", title: "抄送给我" },
} as const

export function workspaceQueueView(pathname: string) {
    return (
        Object.keys(
            WORKSPACE_QUEUE_ROUTES,
        ) as (keyof typeof WORKSPACE_QUEUE_ROUTES)[]
    ).find((view) => WORKSPACE_QUEUE_ROUTES[view].href === pathname)
}

const APPROVAL_SUB_VIEWS = new Set(["started", "mine", "blocked", "managed"])

/** 固定页面不能通过 view 参数变成其他页面；范围内待办仍属于任务页，审批四视图归属审批页。 */
export function workspaceQueueParams(
    pathname: string,
    params: URLSearchParams,
) {
    const next = new URLSearchParams(params)
    const view = workspaceQueueView(pathname)
    if (view && view !== "copied") {
        if (view === "started") {
            const rawApprovalView = next.get("view") ?? next.get("approvalView")
            if (rawApprovalView && APPROVAL_SUB_VIEWS.has(rawApprovalView)) {
                next.set("approvalView", rawApprovalView)
            } else {
                next.delete("approvalView")
            }
            next.set("view", "started")
        } else {
            next.delete("approvalView")
            next.set(
                "view",
                view === "inbox" && next.get("view") === "managed"
                    ? "managed"
                    : view,
            )
        }
    }
    return next
}

/** 生成独立页面的地址，移除页面身份和容器参数。 */
export function workspaceQueueLocation(
    view: string,
    params = new URLSearchParams(),
) {
    const key = view === "managed" ? "inbox" : view
    const route =
        WORKSPACE_QUEUE_ROUTES[key as keyof typeof WORKSPACE_QUEUE_ROUTES] ??
        WORKSPACE_QUEUE_ROUTES.inbox
    const next = new URLSearchParams(params)
    const approvalView = next.get("approvalView")
    next.delete("queue")
    next.delete("view")
    next.delete("approvalView")
    next.delete("_workspace_frame")
    if (view === "managed") next.set("view", "managed")
    if (view === "started") {
        if (
            approvalView &&
            approvalView !== "started" &&
            APPROVAL_SUB_VIEWS.has(approvalView)
        ) {
            next.set("view", approvalView)
        }
        next.delete("docType")
        next.delete("status")
    }
    const query = next.toString()
    return `${route.href}${query ? `?${query}` : ""}`
}
