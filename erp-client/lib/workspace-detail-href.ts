export type WorkspaceDetailKind = "tasks" | "approvals" | "copies"

/** 返回地址只允许工作台队列，移除旧的详情参数，避免返回详情自身。 */
export function workspaceQueueHref(
    value?: string | null,
    fallback = "/workspace",
): string {
    if (!value?.startsWith("/workspace")) return fallback
    const url = new URL(value, "https://workspace.invalid")
    if (
        url.origin !== "https://workspace.invalid" ||
        url.pathname !== "/workspace"
    )
        return fallback
    url.searchParams.delete("currentWorkItemId")
    url.searchParams.delete("copyReceiptId")
    return `${url.pathname}${url.search}`
}

/** 任务、审批和抄送拥有独立地址；队列上下文仅用于返回和连续办理。 */
export function buildWorkspaceDetailHref(
    kind: WorkspaceDetailKind,
    id: string,
    options: { returnTo?: string; autoNext?: boolean } = {},
): string {
    const params = new URLSearchParams()
    if (options.returnTo)
        params.set("returnTo", workspaceQueueHref(options.returnTo))
    if (options.autoNext) params.set("autoNext", "1")
    const query = params.toString()
    return `/workspace/${kind}/${encodeURIComponent(id)}${query ? `?${query}` : ""}`
}
