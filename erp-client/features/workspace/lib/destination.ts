import type { WorkspaceWorkItem } from "@/features/workspace/types"
import { hasPermission } from "@/lib/permissions"
import { workspaceQueueHref } from "@/lib/workspace-detail-href"

import { buildHandlerHref } from "./handler-destination"

/** 驳回审批提供原销售单编辑入口；保存与重提资格由目标页按当前单据重验。 */
export function buildSalesOrderReworkHref(
    item: WorkspaceWorkItem,
    grantedPermissions: readonly string[],
    returnTo: string | null,
): string | null {
    if (
        !["APPROVAL_INSTANCE", "DOCUMENT_APPROVAL"].includes(
            item.workItemType,
        ) ||
        !["sales_order", "voucher_sales_order"].includes(
            item.businessObjectType,
        ) ||
        item.approval?.status !== "REJECTED" ||
        ![
            "sales_order:detail",
            "sales_order:update",
            "sales_order:submit",
        ].every((permission) => hasPermission(grantedPermissions, permission))
    )
        return null

    const href = buildDocumentHref({ ...item, handlerKey: "approval_rework" })
    if (!href) return null
    const [path, query] = href.split("?", 2)
    const params = new URLSearchParams(query)
    params.set(
        "returnTo",
        workspaceQueueHref(
            returnTo,
            item.workItemType === "APPROVAL_INSTANCE"
                ? "/workspace/approvals"
                : "/workspace/tasks",
        ),
    )
    return `${path}?${params.toString()}`
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
