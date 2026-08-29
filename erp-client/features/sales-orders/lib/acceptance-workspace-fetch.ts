/**
 * W06 客户验收 — 工作台读取（queryFn）。
 * 进度表需要全部交付事实；待验数量由客户端从 eligibleQuantity 推导。
 */

import { apiGet } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    AcceptanceHistoryItem,
    CustomerAcceptanceWorkspaceView,
} from "@/features/sales-orders/lib/acceptance-types"
import {
    hasRemainingEligibleAcceptance,
    mapHistoryItem,
    mapSalesLine,
    type BackendEligibilityView,
} from "@/features/sales-orders/lib/acceptance-mappers"
import { parseQty } from "@/features/sales-orders/lib/acceptance-model"
import { fetchSalesOrderDetail } from "@/features/sales-orders/api/sales-orders"
import type { WorkItemProjection } from "@/features/work-items/types"

export type FetchAcceptanceWorkspaceParams = {
    salesOrderId: string
    workItemId?: string | null
    workItem?: WorkItemProjection
}

function acceptanceTaskContext(params: FetchAcceptanceWorkspaceParams): {
    workItem: CustomerAcceptanceWorkspaceView["workItem"]
    blocker: string | null
} {
    const requestedId = params.workItemId?.trim()
    if (!requestedId) return { workItem: null, blocker: null }

    const task = params.workItem
    const taskVersion = Number(task?.taskVersion)
    const valid =
        task?.workItemId === requestedId &&
        task.workItemType === "CUSTOMER_ACCEPTANCE_REGISTRATION" &&
        task.handlerKey === "customer_acceptance_registration" &&
        task.destinationWorkspaceId === "W06" &&
        task.businessObjectType === "sales_order" &&
        task.businessObjectId === params.salesOrderId &&
        task.status === "OPEN" &&
        task.allowedActions.includes("PROCESS") &&
        Number.isSafeInteger(taskVersion) &&
        taskVersion > 0

    if (!valid) {
        return {
            workItem: null,
            blocker:
                "当前客户验收任务已变化或与本单不一致，请返回工作台刷新后再登记。",
        }
    }
    return {
        workItem: { id: task.workItemId, expectedTaskVersion: taskVersion },
        blocker: null,
    }
}

/**
 * 只问有没有尚未验收完的履约事实。详情焦点横幅用，不拉工作台。
 */
export async function fetchHasEligibleAcceptance(
    salesOrderId: string,
): Promise<boolean> {
    try {
        const eligibility = await apiGet<BackendEligibilityView>(
            "/admin/customer-acceptances/eligible",
            { sales_order_id: salesOrderId },
        )
        return hasRemainingEligibleAcceptance(eligibility)
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.status === 404) return false
        throw err
    }
}

export async function fetchCustomerAcceptanceWorkspace(
    params: FetchAcceptanceWorkspaceParams,
): Promise<CustomerAcceptanceWorkspaceView | null> {
    const order = await fetchSalesOrderDetail(params.salesOrderId)
    if (!order) return null

    const taskContext = acceptanceTaskContext(params)
    const workItemConfigBlocker = taskContext.blocker
    const factsUpdatedAt = order.sourceAsOf || new Date().toISOString()

    if (order.nature === "card_voucher") {
        return {
            salesOrder: {
                id: order.id,
                salesOrderNo: order.documentNumber,
                businessType: "CARD_VOUCHER",
                customerLabel: order.customerName,
                commercialStatus: order.primaryStatus.label,
                commercialStatusTone: order.primaryStatus.tone,
                fulfillmentProgress: order.fulfillment.label,
                collectionProgress: order.collection.label,
                invoiceProgress: order.invoicing.label,
                lockVersion: order.lockVersion ?? order.version,
                factsUpdatedAt,
            },
            freshness: { factsUpdatedAt, state: "fresh" },
            metrics: {
                eligibleFulfillmentCount: 0,
                eligibleQuantityByUnit: [],
                overdueLineCount: 0,
            },
            salesLines: [],
            draft: null,
            history: [],
            permissions: {
                allowedActions: [],
                actionBlockers: [
                    {
                        action: "CREATE_ACCEPTANCE",
                        code: "CARD_VOUCHER_NOT_SUPPORTED",
                        message:
                            "卡券销售单不用做客户验收；履约完成按销售单履约期限判断。",
                    },
                ],
                fieldVisibility: { customerName: "full" },
            },
            workItem: taskContext.workItem,
            workItemConfigBlocker,
        }
    }

    let eligibility: BackendEligibilityView
    try {
        eligibility = await apiGet<BackendEligibilityView>(
            "/admin/customer-acceptances/eligible",
            { sales_order_id: params.salesOrderId },
        )
    } catch (err) {
        const apiErr = err as ApiError
        if (apiErr?.status === 404) return null
        throw err
    }

    const salesLines = (eligibility.sales_lines ?? []).map(mapSalesLine)
    const eligibleFacts = salesLines
        .flatMap((line) => line.fulfillmentFacts)
        .filter((fact) => parseQty(fact.eligibleQuantity) > 0)
    const qtyByUnit = new Map<string, number>()
    for (const fact of eligibleFacts) {
        qtyByUnit.set(
            fact.unitCode,
            (qtyByUnit.get(fact.unitCode) ?? 0) + Number(fact.eligibleQuantity),
        )
    }

    const history = (eligibility.history ?? [])
        .map(mapHistoryItem)
        .filter((item): item is AcceptanceHistoryItem => item != null)

    const allowedActions = workItemConfigBlocker
        ? []
        : ["CREATE_ACCEPTANCE", "POST_ACCEPTANCE"]
    if (
        !workItemConfigBlocker &&
        history.some((item) => item.status === "POSTED")
    ) {
        allowedActions.push("REVERSE_ACCEPTANCE")
    }

    return {
        salesOrder: {
            id: order.id,
            salesOrderNo: order.documentNumber,
            businessType: "GOODS_SERVICE",
            customerLabel: order.customerName,
            commercialStatus: order.primaryStatus.label,
            commercialStatusTone: order.primaryStatus.tone,
            fulfillmentProgress: order.fulfillment.label,
            collectionProgress: order.collection.label,
            invoiceProgress: order.invoicing.label,
            lockVersion: order.lockVersion ?? order.version,
            factsUpdatedAt,
        },
        freshness: { factsUpdatedAt, state: "fresh" },
        metrics: {
            eligibleFulfillmentCount: eligibleFacts.length,
            eligibleQuantityByUnit: [...qtyByUnit.entries()].map(
                ([unitCode, quantity]) => ({
                    unitCode,
                    quantity: String(quantity),
                }),
            ),
            overdueLineCount: 0,
        },
        salesLines,
        draft: null,
        history,
        permissions: {
            allowedActions,
            actionBlockers: workItemConfigBlocker
                ? [
                      {
                          action: "CREATE_ACCEPTANCE",
                          code: "WORK_ITEM_HANDLER_NOT_REGISTERED",
                          message: workItemConfigBlocker,
                      },
                      {
                          action: "POST_ACCEPTANCE",
                          code: "WORK_ITEM_HANDLER_NOT_REGISTERED",
                          message: workItemConfigBlocker,
                      },
                  ]
                : [],
            fieldVisibility: {
                customerName: "full",
                customerContact: "full",
            },
        },
        workItem: taskContext.workItem,
        workItemConfigBlocker,
    }
}
