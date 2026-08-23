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
 * 非审批任务的「打开单据」地址。审批决定在本页提交，不跳第二套待办页。
 */
export function buildDocumentHref(item: WorkspaceWorkItem): string | null {
    return buildHandlerHref({
        handlerKey: item.handlerKey,
        destinationWorkspaceId: item.destinationWorkspaceId,
        businessObjectId: item.businessObjectId,
        rootBusinessObjectId: item.rootBusinessObjectId,
        workItemId: item.workItemId,
        queueContextId: item.queueContextId,
        routeContext: item.routeContext,
    })
}

export function buildWarningHref(warning: {
    destinationWorkspaceId: WorkspaceId
    objectId?: string
}): string {
    return resolveWorkspaceHref(warning.destinationWorkspaceId, {
        objectId: warning.objectId,
    })
}
