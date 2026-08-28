type HandlerFamily =
    | "approval"
    | "procurement"
    | "finance"
    | "fulfillment"
    | "exception"

type HandlerWorkspaceId =
    | "W01"
    | "W06"
    | "W05"
    | "W08"
    | "W10"
    | "W11"
    | "W12"
    | "W13"
    | "W17"
    | "W18"
    | "W21"
    | "W26"
    | "W27"
    | "W29"

export type HandlerRegistration = Readonly<{
    workItemTypeLabel: string
    family: HandlerFamily
    destinationWorkspaceId: HandlerWorkspaceId
    baseHref: string
}>

/**
 * 客户端受控处理器注册表。
 *
 * 服务端只提供固定 handler key 与工作面编号；客户端从本表解析本地路由，
 * 不接受服务端下发的任意 URL。
 */
export const HANDLER_REGISTRY: Readonly<Record<string, HandlerRegistration>> = {
    fulfillment_operation: {
        workItemTypeLabel: "履约处理",
        family: "fulfillment",
        destinationWorkspaceId: "W01",
        baseHref: "/workspace",
    },
    customer_acceptance_registration: {
        workItemTypeLabel: "客户验收登记",
        family: "fulfillment",
        destinationWorkspaceId: "W06",
        baseHref: "/sales/orders",
    },
    procurement_order_creation: {
        workItemTypeLabel: "待供给分配",
        family: "procurement",
        destinationWorkspaceId: "W08",
        baseHref: "/procurement/orders",
    },
    supplier_payment_execution: {
        workItemTypeLabel: "供应商付款处理",
        family: "finance",
        destinationWorkspaceId: "W12",
        baseHref: "/finance/supplier-accounts",
    },
    sales_invoice_execution: {
        workItemTypeLabel: "销项开票处理",
        family: "finance",
        destinationWorkspaceId: "W11",
        baseHref: "/finance/customer-accounts",
    },
    sales_change_impact_review: {
        workItemTypeLabel: "销售变更履约影响复核",
        family: "fulfillment",
        destinationWorkspaceId: "W05",
        baseHref: "/sales/orders",
    },
    sales_change_finance_review: {
        workItemTypeLabel: "销售变更财务复核",
        family: "finance",
        destinationWorkspaceId: "W05",
        baseHref: "/sales/orders",
    },
    po_review: {
        workItemTypeLabel: "采购单财务审核",
        family: "finance",
        destinationWorkspaceId: "W08",
        baseHref: "/procurement/orders",
    },
    card_funds: {
        workItemTypeLabel: "卡券票款复核",
        family: "finance",
        destinationWorkspaceId: "W13",
        baseHref: "/finance/card-funds-review",
    },
    card_funds_delta: {
        workItemTypeLabel: "卡券票款差异复核",
        family: "finance",
        destinationWorkspaceId: "W13",
        baseHref: "/finance/card-funds-review",
    },
    inventory_adj: {
        workItemTypeLabel: "库存调整复核",
        family: "fulfillment",
        destinationWorkspaceId: "W10",
        baseHref: "/inventory",
    },
    supplier_settlement: {
        workItemTypeLabel: "供应商结算复核",
        family: "finance",
        destinationWorkspaceId: "W27",
        baseHref: "/supplier-api/settlements",
    },
    import_business_confirmation: {
        workItemTypeLabel: "导入业务确认",
        family: "exception",
        destinationWorkspaceId: "W18",
        baseHref: "/governance/imports",
    },
    supplier_fulfillment_investigation: {
        workItemTypeLabel: "供应商履约异常调查",
        family: "exception",
        destinationWorkspaceId: "W26",
        baseHref: "/supplier-api/orders",
    },
    supplier_supply_exception: {
        workItemTypeLabel: "供应商供给异常",
        family: "exception",
        destinationWorkspaceId: "W21",
        baseHref: "/procurement/supplier-offerings",
    },
    integration_unknown: {
        workItemTypeLabel: "集成结果待确认",
        family: "exception",
        destinationWorkspaceId: "W29",
        baseHref: "/governance/integration-errors",
    },
    master_mapping_task: {
        workItemTypeLabel: "主数据映射任务",
        family: "exception",
        destinationWorkspaceId: "W17",
        baseHref: "/governance/mall-sync",
    },
    business_exception: {
        workItemTypeLabel: "业务异常",
        family: "exception",
        destinationWorkspaceId: "W29",
        baseHref: "/governance/integration-errors",
    },
}

/** 返回 handler 注册；未知 handler 不提供降级入口。 */
export function getHandlerRegistration(
    handlerKey: string,
): HandlerRegistration | undefined {
    return HANDLER_REGISTRY[handlerKey]
}

/** 仅接受客户端注册且与服务端工作面目标完全一致的处理器。 */
export function isRegisteredHandlerDestination(
    handlerKey: string,
    destinationWorkspaceId?: string,
): boolean {
    return (
        getHandlerRegistration(handlerKey)?.destinationWorkspaceId ===
        destinationWorkspaceId
    )
}

export type HandlerNavigationInput = Readonly<{
    handlerKey: string
    destinationWorkspaceId?: string
    businessObjectType?: string
    businessObjectId: string
    rootBusinessObjectId?: string
    workItemId: string
    approvalInstanceId?: string
    trackingOnly?: boolean
    queueContextId?: string
    routeContext?: Readonly<{ confirmationScope?: string }>
}>

/**
 * 通用单据审批的打开单据地址。工作面由服务端 destination 决定，不走已废止的确认队列。
 *
 * @param item 工作台当前任务导航上下文。
 * @returns 受控单据路径；工作面未知或缺对象身份时关闭。
 */
function buildDocumentApprovalHref(
    item: HandlerNavigationInput,
): string | null {
    const businessObjectId = requiredValue(item.businessObjectId)
    const workItemId = requiredValue(item.workItemId)
    const approvalInstanceId = requiredValue(item.approvalInstanceId)
    if (
        !businessObjectId ||
        (item.trackingOnly ? !approvalInstanceId : !workItemId)
    ) {
        return null
    }

    const params = new URLSearchParams({ from: "workspace" })
    if (item.trackingOnly && approvalInstanceId) {
        params.set("approvalInstanceId", approvalInstanceId)
    } else if (workItemId) {
        params.set("workItemId", workItemId)
    }
    const queueContextId = requiredValue(item.queueContextId)
    if (!item.trackingOnly && queueContextId) {
        params.set("queueContextId", queueContextId)
    }

    switch (item.destinationWorkspaceId) {
        case "W05":
            params.set("section", "approval")
            return withParams(
                `/sales/orders/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "W08":
            params.set("mode", "review")
            return withParams(
                `/procurement/orders/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "W10":
            params.set("adjustmentId", businessObjectId)
            if (workItemId && !item.trackingOnly) {
                params.set("currentWorkItemId", workItemId)
            }
            return withParams("/inventory", params)
        case "W11":
            if (!customerApprovalPreviewKind(item.businessObjectType)) {
                return null
            }
            params.set("view", "receipt")
            params.set(
                "previewKind",
                customerApprovalPreviewKind(item.businessObjectType) ??
                    "receipt",
            )
            params.set("previewId", businessObjectId)
            if (workItemId && !item.trackingOnly) {
                params.set("currentWorkItemId", workItemId)
            }
            return withParams("/finance/customer-accounts", params)
        case "W12": {
            const previewKind = supplierApprovalPreviewKind(
                item.businessObjectType,
            )
            if (!previewKind) return null
            params.set(
                "view",
                previewKind === "payment" ? "payment" : "payable",
            )
            params.set("previewKind", previewKind)
            params.set("detailId", businessObjectId)
            if (workItemId && !item.trackingOnly) {
                params.set("currentWorkItemId", workItemId)
            }
            return withParams("/finance/supplier-accounts", params)
        }
        default:
            return null
    }
}

/** 解析客户侧审批单据在 W11 使用的详情类型。 */
function customerApprovalPreviewKind(
    documentType?: string,
): "receipt" | "refund" | "reversal" | null {
    switch (documentType) {
        case "customer_receipt":
            return "receipt"
        case "customer_refund":
            return "refund"
        case "receipt_reversal":
            return "reversal"
        default:
            return null
    }
}

/** 解析供应商侧审批单据在 W12 使用的详情类型。 */
function supplierApprovalPreviewKind(
    documentType?: string,
): "payment" | "refund" | "reversal" | null {
    switch (documentType) {
        case "supplier_payment":
            return "payment"
        case "supplier_refund":
            return "refund"
        case "payment_reversal":
            return "reversal"
        default:
            return null
    }
}

function requiredValue(value?: string): string | null {
    const normalized = value?.trim()
    return normalized ? normalized : null
}

function withParams(path: string, params: URLSearchParams): string {
    const [pathname, existingQuery] = path.split("?", 2)
    if (existingQuery) {
        const existing = new URLSearchParams(existingQuery)
        existing.forEach((value, key) => {
            if (!params.has(key)) params.set(key, value)
        })
    }
    return `${pathname}?${params.toString()}`
}

/**
 * 为工作台当前任务构造受控「打开单据」链接。
 *
 * 链接只携带稳定业务对象、任务和队列上下文；任务版本与允许动作必须由
 * 目标工作面重新查询。任一必需上下文缺失或 handler/工作面不匹配时失败关闭。
 */
export function buildHandlerHref(item: HandlerNavigationInput): string | null {
    if (item.handlerKey === "document_approval") {
        return buildDocumentApprovalHref(item)
    }

    const registration = getHandlerRegistration(item.handlerKey)
    if (
        !registration ||
        registration.destinationWorkspaceId !== item.destinationWorkspaceId
    ) {
        return null
    }

    const businessObjectId = requiredValue(item.businessObjectId)
    const workItemId = requiredValue(item.workItemId)
    if (!businessObjectId || !workItemId) return null

    if (item.handlerKey === "procurement_order_creation") {
        return withParams(
            registration.baseHref,
            new URLSearchParams({
                action: "create",
                salesOrderId: businessObjectId,
                workItemId,
            }),
        )
    }

    if (item.handlerKey === "fulfillment_operation") {
        return withParams(
            registration.baseHref,
            new URLSearchParams({
                family: "fulfillment",
                currentWorkItemId: workItemId,
            }),
        )
    }

    if (item.handlerKey === "customer_acceptance_registration") {
        const params = new URLSearchParams({
            section: "acceptance",
            from: "W01",
            workItemId,
            returnTo: `/workspace?currentWorkItemId=${encodeURIComponent(workItemId)}`,
        })
        const queueContextId = requiredValue(item.queueContextId)
        if (queueContextId) params.set("queueContextId", queueContextId)
        return withParams(
            `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
            params,
        )
    }

    const queueContextId = requiredValue(item.queueContextId)
    if (!queueContextId) return null

    const params = new URLSearchParams({
        from: "workspace",
        workItemId,
        queueContextId,
    })

    switch (item.handlerKey) {
        case "sales_change_impact_review":
        case "sales_change_finance_review": {
            const salesOrderId = requiredValue(item.rootBusinessObjectId)
            if (!salesOrderId || salesOrderId === businessObjectId) return null
            params.set("section", "change-review")
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(salesOrderId)}`,
                params,
            )
        }
        case "po_review":
            params.set("mode", "review")
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "supplier_payment_execution": {
            const purchaseOrderId = requiredValue(item.rootBusinessObjectId)
            if (!purchaseOrderId || purchaseOrderId === businessObjectId) {
                return null
            }
            params.set("from", "W01")
            params.set("view", "payable")
            params.set("session", "payment")
            params.set("purchaseOrderId", purchaseOrderId)
            params.set("detailId", businessObjectId)
            params.set("previewKind", "payable")
            params.set("currentWorkItemId", workItemId)
            return withParams(registration.baseHref, params)
        }
        case "sales_invoice_execution": {
            const salesOrderId = requiredValue(item.rootBusinessObjectId)
            if (!salesOrderId || salesOrderId === businessObjectId) {
                return null
            }
            params.set("from", "W01")
            params.set("view", "sales_invoice")
            params.set("register", "invoice")
            params.set("receivableAccountId", businessObjectId)
            params.set("salesOrderId", salesOrderId)
            params.set("previewKind", "receivable")
            params.set("previewId", businessObjectId)
            params.set("currentWorkItemId", workItemId)
            return withParams(registration.baseHref, params)
        }
        case "supplier_settlement":
            params.set("section", "review")
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "supplier_fulfillment_investigation":
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "supplier_supply_exception":
            return withParams(registration.baseHref, params)
        case "master_mapping_task":
            params.set("view", "mapping")
            params.set("mappingTaskId", businessObjectId)
            return withParams(registration.baseHref, params)
        case "integration_unknown":
        case "business_exception":
            params.set("resolveWorkItemId", workItemId)
            return withParams(registration.baseHref, params)
        case "import_business_confirmation": {
            const confirmationScope = requiredValue(
                item.routeContext?.confirmationScope,
            )
            if (!confirmationScope) return null
            params.set("section", "confirm")
            params.set("batchId", businessObjectId)
            params.set("confirmationScope", confirmationScope)
            return withParams(registration.baseHref, params)
        }
        default:
            params.set("currentWorkItemId", workItemId)
            return withParams(registration.baseHref, params)
    }
}
