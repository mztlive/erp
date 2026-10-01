/** 销售退货处理类型；与后端 `CaseType` snake_case 对齐。 */
export type SalesReturnCaseType =
    | "return"
    | "reject"
    | "shortage"
    | "service_failed"

/** 退货路线；与后端 `ReturnRoute` snake_case 对齐。 */
export type SalesReturnRoute =
    | "company_warehouse"
    | "direct_to_supplier"
    | "no_physical_return"

/** SalesReturnCase 为 NO_APPROVAL：明细 DTO 不得携带审批绑定。 */
export type BackendSalesReturnLine = {
    id: string
    sales_order_line_id: string
    requested_quantity: string
    received_quantity?: string | null
    quality_result?: string | null
    restockable_quantity?: string | null
}

/** SalesReturnCase 为 NO_APPROVAL：创建/详情 DTO 不得携带审批绑定。 */
export type BackendSalesReturnCase = {
    id: string
    return_no: string
    sales_order_id: string
    acceptance_id?: string | null
    case_type: SalesReturnCaseType
    reason: string
    discovered_at: number
    return_route: SalesReturnRoute
    status: string
    version: number
    created_at: number
    lines: BackendSalesReturnLine[]
}

/** SalesReturnCase 为 NO_APPROVAL：行投影不得嵌入审批区。 */
export type SalesReturnCaseRow = Readonly<{
    id: string
    returnNo: string
    salesOrderId: string
    acceptanceId?: string
    caseType: SalesReturnCaseType
    caseTypeLabel: string
    reason: string
    discoveredAt: number
    returnRoute: SalesReturnRoute
    returnRouteLabel: string
    status: string
    statusLabel: string
    version: number
    createdAt: number
    allowedActions: readonly string[]
    lines: readonly {
        id: string
        salesOrderLineId: string
        requestedQuantity: string
        receivedQuantity?: string
        qualityResult?: string
        restockableQuantity?: string
    }[]
}>
