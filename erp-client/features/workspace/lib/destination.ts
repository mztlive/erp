import { getWorkspaceById, type WorkspaceId } from "@/lib/workspace-registry"
import type { WorkspaceWorkItem } from "@/features/workspace/types"

import { buildHandlerHref } from "./handler-destination"

/**
 * 从本地工作面注册表解析应用内路径。服务端只返回工作面编号。
 */
export function resolveWorkspaceHref(
    workspaceId: WorkspaceId,
    query?: Record<string, string | undefined>,
): string {
    const entry = getWorkspaceById(workspaceId)
    const base = entry.navHref
    if (!query) return base

    const params = new URLSearchParams()
    for (const [key, value] of Object.entries(query)) {
        if (value) params.set(key, value)
    }

    const [path, existingQs] = base.split("?")
    if (existingQs) {
        const existing = new URLSearchParams(existingQs)
        existing.forEach((value, key) => {
            if (!params.has(key)) params.set(key, value)
        })
    }
    const qs = params.toString()
    return qs ? `${path}?${qs}` : path
}

/**
 * 非审批任务跳转目标工作面的地址。审批决定在本页提交，不跳第二套待办页。
 */
export function buildDocumentHref(
    item: Pick<
        WorkspaceWorkItem,
        | "handlerKey"
        | "destinationWorkspaceId"
        | "businessObjectType"
        | "businessObjectId"
        | "rootBusinessObjectId"
        | "workItemId"
        | "approvalProcessInstanceId"
        | "workItemType"
        | "queueContextId"
        | "routeContext"
    >,
): string | null {
    const href = buildHandlerHref({
        handlerKey: item.handlerKey,
        destinationWorkspaceId: item.destinationWorkspaceId,
        businessObjectType: item.businessObjectType,
        businessObjectId: item.businessObjectId,
        rootBusinessObjectId: item.rootBusinessObjectId,
        workItemId: item.workItemId,
        approvalInstanceId: item.approvalProcessInstanceId,
        trackingOnly: item.workItemType === "APPROVAL_INSTANCE",
        queueContextId: item.queueContextId,
        routeContext: item.routeContext,
    })
    if (!href || item.handlerKey === "document_approval") return href
    if (item.handlerKey === "procurement_order_creation") {
        return `/sales/orders/${encodeURIComponent(item.businessObjectId)}?from=workspace`
    }
    if (item.handlerKey === "fulfillment_operation") {
        const root = item.rootBusinessObjectId?.trim()
        if (!root || root === item.businessObjectId) return null
        const base =
            item.businessObjectType === "delivery"
                ? "/sales/orders"
                : "/procurement/orders"
        return `${base}/${encodeURIComponent(root)}?from=workspace`
    }
    const [path, query] = href.split("?", 2)
    const params = new URLSearchParams(query)
    // 查看单据不得自动打开付款、开票或登记表单；处理仍由当前任务的正式按钮发起。
    for (const key of ["session", "register", "mode", "action"])
        params.delete(key)
    return `${path}?${params.toString()}`
}

export function buildWarningHref(warning: {
    destinationWorkspaceId: WorkspaceId
    objectId?: string
}): string {
    return resolveWorkspaceHref(warning.destinationWorkspaceId, {
        objectId: warning.objectId,
    })
}
