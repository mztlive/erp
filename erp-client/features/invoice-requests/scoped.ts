/** 开票申请范围行 wire 形态（M08；申请金额为行级事实始终返回）。 */

import type {
    FundsScopedPageWire,
    FundsScopedResultWire,
    FundsSummaryWire,
} from "@/lib/funds-scope"

export type ScopedInvoiceRequestWire = Readonly<{
    id: string
    request_no: string
    sales_order_id: string
    sales_order_no: string
    status: string
    created_at: number
    applicant_user_id: string
    handler_user_id: string | null
    amount: string
    permission_limited: boolean
    sales_owner_user_id: string | null
    business_org_unit_id: string | null
}>

export type InvoiceRequestScopeQuery = {
    page: number
    pageSize: number
    /** 跨页回传的范围版本；第二页起必填。 */
    scopeVersion?: string
    /** 关联销售单当前负责销售（逗号分隔稳定 ID）。 */
    salesOwnerUserIds?: string
    /** 申请人（逗号分隔稳定 ID）。 */
    applicantUserIds?: string
    /** 当前开票处理人（逗号分隔稳定 ID）。 */
    handlerUserIds?: string
    /** 关联销售单当前业务组织（逗号分隔）。 */
    orgUnitIds?: string
    includeDescendants?: boolean
    q?: string
    status?: string
    salesOrderId?: string
    customerId?: string
    receivableAccountId?: string
}

export type InvoiceRequestScopeListView = {
    requests: readonly ScopedInvoiceRequestWire[]
    total: number
    summary: FundsSummaryWire | undefined
    ownerOptions: readonly { value: string; label: string }[]
    scopeVersion: string | undefined
    policyVersion: number | undefined
    organizationVersion: number | undefined
    scopeSummary: string | undefined
    asOf: string | undefined
    emptyReason: string | null | undefined
    hasScope: boolean
}

export type FundsScopedInvoiceRequestPageWire =
    FundsScopedPageWire<ScopedInvoiceRequestWire>
export type FundsScopedInvoiceRequestResultWire =
    FundsScopedResultWire<ScopedInvoiceRequestWire>
