import type { WorkspaceWorkItem } from "../types"

const ACCEPTANCE_REASON_CODES = new Set([
    "CUSTOMER_ACCEPTANCE_REQUIRED",
    "CUSTOMER_ACCEPTANCE_REOPENED_BY_REVERSAL",
])

export type WorkspaceAcceptanceDescriptor = Readonly<{
    salesOrderId: string
}>

export type WorkspaceAcceptanceTaskIdentity = Readonly<{
    workItemId: string
    workItemType: string
    handlerKey: string
    destinationWorkspaceId?: string
    businessObjectType: string
    businessObjectId: string
    status: string
    taskVersion: string
    allowedActions: readonly string[]
}>

/**
 * 把 W01 客户验收任务解析为唯一销售单。
 * 任务类型、处理器、对象类型、责任角色、工作面或原因码任一不一致时失败关闭。
 */
export function workspaceAcceptanceDescriptor(
    item: Pick<
        WorkspaceWorkItem,
        | "workItemType"
        | "handlerKey"
        | "businessObjectType"
        | "businessObjectId"
        | "ownerRole"
        | "reasonCode"
        | "destinationWorkspaceId"
    >,
): WorkspaceAcceptanceDescriptor | null {
    if (
        item.workItemType !== "CUSTOMER_ACCEPTANCE_REGISTRATION" ||
        item.handlerKey !== "customer_acceptance_registration"
    ) {
        return null
    }

    const objectType = item.businessObjectType.trim().toLowerCase()
    const salesOrderId = item.businessObjectId.trim()
    if (
        objectType !== "sales_order" ||
        item.ownerRole !== "sales_order_owner" ||
        item.destinationWorkspaceId !== "W06" ||
        !ACCEPTANCE_REASON_CODES.has(item.reasonCode ?? "") ||
        !salesOrderId
    ) {
        return null
    }

    return { salesOrderId }
}

/** 工作台任务冻结身份，交给 W06 作业面做正式命令校验。 */
export function workspaceAcceptanceTaskIdentity(
    item: Pick<
        WorkspaceWorkItem,
        | "workItemId"
        | "workItemType"
        | "handlerKey"
        | "destinationWorkspaceId"
        | "businessObjectType"
        | "businessObjectId"
        | "status"
        | "taskVersion"
        | "allowedActions"
    >,
): WorkspaceAcceptanceTaskIdentity {
    return {
        workItemId: item.workItemId,
        workItemType: item.workItemType,
        handlerKey: item.handlerKey,
        destinationWorkspaceId: item.destinationWorkspaceId,
        businessObjectType: item.businessObjectType,
        businessObjectId: item.businessObjectId,
        status: item.status,
        taskVersion: item.taskVersion,
        allowedActions: item.allowedActions,
    }
}
