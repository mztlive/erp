type HandlerFamily = "approval" | "finance" | "fulfillment" | "exception"

type HandlerWorkspaceId =
    | "W03"
    | "W05"
    | "W07"
    | "W08"
    | "W10"
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
    procurement_confirmation: {
        workItemTypeLabel: "采购二次确认",
        family: "fulfillment",
        destinationWorkspaceId: "W07",
        baseHref: "/procurement/confirm",
    },
    low_margin_manager: {
        workItemTypeLabel: "低毛利销售审批",
        family: "approval",
        destinationWorkspaceId: "W05",
        baseHref: "/sales/orders",
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
    card_sales_manager_approval: {
        workItemTypeLabel: "卡券销售领导审批",
        family: "approval",
        destinationWorkspaceId: "W05",
        baseHref: "/sales/orders",
    },
    card_sales_operations_approval: {
        workItemTypeLabel: "卡券运营审批",
        family: "approval",
        destinationWorkspaceId: "W05",
        baseHref: "/sales/orders",
    },
    ownership_sales: {
        workItemTypeLabel: "归属迁移销售确认",
        family: "approval",
        destinationWorkspaceId: "W03",
        baseHref: "/sales/customers",
    },
    ownership_finance: {
        workItemTypeLabel: "归属迁移财务确认",
        family: "finance",
        destinationWorkspaceId: "W17",
        baseHref: "/governance/mall-sync",
    },
    inventory_adj: {
        workItemTypeLabel: "库存调整复核",
        family: "fulfillment",
        destinationWorkspaceId: "W10",
        baseHref: "/inventory",
    },
    finance_correction: {
        workItemTypeLabel: "财务纠错复核",
        family: "finance",
        destinationWorkspaceId: "W17",
        baseHref: "/governance/mall-sync",
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
    businessObjectId: string
    rootBusinessObjectId?: string
    workItemId: string
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
    if (!businessObjectId || !workItemId) return null

    const params = new URLSearchParams({
        from: "workspace",
        workItemId,
    })
    const queueContextId = requiredValue(item.queueContextId)
    if (queueContextId) params.set("queueContextId", queueContextId)

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
        case "W11":
            params.set("view", "receipt")
            params.set("previewKind", "receipt")
            params.set("previewId", businessObjectId)
            params.set("currentWorkItemId", workItemId)
            return withParams("/finance/customer-accounts", params)
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
    const queueContextId = requiredValue(item.queueContextId)
    if (!businessObjectId || !workItemId || !queueContextId) return null

    const params = new URLSearchParams({
        from: "workspace",
        workItemId,
        queueContextId,
    })

    switch (item.handlerKey) {
        case "card_sales_manager_approval":
        case "card_sales_operations_approval":
            params.set("section", "approval")
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
                params,
            )
        case "low_margin_manager":
            params.set("section", "procurement-rejection")
            return withParams(
                `${registration.baseHref}/${encodeURIComponent(businessObjectId)}`,
                params,
            )
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
