/**
 * W06 客户验收 — 工作台读取（queryFn）。
 * 从 api/acceptance.ts 拆出；api/acceptance.ts 保持原导出名 re-export。
 */

import { apiGet } from "@/lib/api"
import type { ApiError } from "@/lib/api/errors"
import type {
    AcceptanceHistoryItem,
    CustomerAcceptanceWorkspaceView,
} from "@/features/sales-orders/lib/acceptance-types"
import {
    formatInstant,
    hasRemainingEligibleAcceptance,
    mapFactType,
    mapHistoryItem,
    mapSalesLine,
    type BackendAcceptanceDetail,
    type BackendAcceptanceHeader,
    type BackendEligibilityView,
    type PageView,
} from "@/features/sales-orders/lib/acceptance-mappers"
import { fetchSalesOrderDetail } from "@/features/sales-orders/api/sales-orders"
import type { WorkItemProjection } from "@/features/work-items/types"

export type FetchAcceptanceWorkspaceParams = {
    salesOrderId: string
    remainingOnly?: boolean
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
                "当前客户验收任务已变化、不可访问或与销售单不一致，请返回统一工作台刷新后重试。",
        }
    }
    return {
        workItem: { id: task.workItemId, expectedTaskVersion: taskVersion },
        blocker: null,
    }
}

/**
 * 只问有没有尚未验收完的履约事实。详情焦点横幅用，不拉草稿和工作台。
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
                            "卡券销售单不在客户验收登记；履约完成按销售单履约期限判断。",
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

    const remainingOnly = params.remainingOnly !== false
    let salesLines = (eligibility.sales_lines ?? []).map(mapSalesLine)
    if (remainingOnly) {
        salesLines = salesLines
            .map((line) => ({
                ...line,
                fulfillmentFacts: line.fulfillmentFacts.filter(
                    (f) => Number(f.eligibleQuantity) > 0,
                ),
            }))
            .filter(
                (line) =>
                    line.fulfillmentFacts.length > 0 ||
                    Number(line.netAcceptedQuantity) > 0,
            )
    }

    const allFacts = salesLines.flatMap((l) => l.fulfillmentFacts)
    const eligibleFacts = allFacts.filter((f) => Number(f.eligibleQuantity) > 0)
    const qtyByUnit = new Map<string, number>()
    for (const fact of eligibleFacts) {
        qtyByUnit.set(
            fact.unitCode,
            (qtyByUnit.get(fact.unitCode) ?? 0) + Number(fact.eligibleQuantity),
        )
    }

    const history = (eligibility.history ?? [])
        .map(mapHistoryItem)
        .filter((h): h is AcceptanceHistoryItem => h != null)

    // 草稿：取最新 DRAFT 验收单（若有）
    let draft: CustomerAcceptanceWorkspaceView["draft"] = null
    try {
        const draftPage = await apiGet<PageView<BackendAcceptanceHeader>>(
            "/admin/customer-acceptances",
            {
                sales_order_id: params.salesOrderId,
                status: "DRAFT",
                page: 1,
                page_size: 1,
                sort_by: "created_at",
                sort_dir: "desc",
            },
        )
        const header = draftPage.items[0]
        if (header) {
            const detail = await apiGet<BackendAcceptanceDetail>(
                `/admin/customer-acceptances/${header.id}`,
            )
            draft = {
                acceptanceDraftId: header.id,
                draftVersion: header.version,
                salesOrderId: params.salesOrderId,
                acceptedAt: formatInstant(header.accepted_at),
                comment: "",
                lines: detail.lines.map((line) => ({
                    salesOrderLineId: line.sales_order_line_id,
                    acceptedQuantity: line.accepted_quantity,
                    shortQuantity: line.short_quantity,
                    rejectedQuantity: line.rejected_quantity,
                    reason: line.reason ?? "",
                    allocations: detail.allocations
                        .filter(
                            (a) => a.customer_acceptance_line_id === line.id,
                        )
                        .map((a) => ({
                            fulfillmentLineId: a.fulfillment_line_id,
                            fulfillmentFactType: mapFactType(
                                a.fulfillment_fact_type,
                            ),
                            allocatedQuantity: a.allocated_quantity,
                        })),
                })),
                updatedAt: formatInstant(header.created_at),
            }
        }
    } catch {
        // 草稿读取失败不阻塞工作台
    }

    const allowedActions = workItemConfigBlocker
        ? []
        : ["CREATE_ACCEPTANCE", "POST_ACCEPTANCE", "SAVE_DRAFT"]
    if (!workItemConfigBlocker && history.some((h) => h.status === "POSTED")) {
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
        draft,
        history,
        permissions: {
            allowedActions,
            actionBlockers: workItemConfigBlocker
                ? [
                      {
                          action: "CREATE_ACCEPTANCE",
                          code: "WORK_ITEM_HANDLER_NOT_REGISTERED",
                          message: workItemConfigBlocker!,
                      },
                      {
                          action: "SAVE_DRAFT",
                          code: "WORK_ITEM_HANDLER_NOT_REGISTERED",
                          message: workItemConfigBlocker!,
                      },
                      {
                          action: "POST_ACCEPTANCE",
                          code: "WORK_ITEM_HANDLER_NOT_REGISTERED",
                          message: workItemConfigBlocker!,
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
