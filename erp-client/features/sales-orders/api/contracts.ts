import type {
    SalesOrderListItem,
    SalesOrderOrigin,
} from "@/features/sales-orders/types"

// ─── 导出视图类型（保持 queries 契约） ───────────────────────────────────────

export type SalesOrderDetailView = SalesOrderListItem & {
    acceptance?: {
        acceptedQuantity: string
        note: string
        reference: string
        postedAt: string
    } | null
    permissionVersion: string
    sourceAsOf: string
    queriedAt: string
}

export type SalesOrdersListQuery = {
    page: number
    pageSize: number
    search?: string
    customerId?: string
    contractId?: string
    createdBy?: string
    nature?: "all" | "physical_service" | "card_voucher"
    /** 四个固定工作视图；"mine"/"createdByMe" 需要 `currentUserId`。 */
    summary?: "all" | "mine" | "createdByMe" | "exception"
    /** 当前登录人账号 id；"待我处理"/"我创建的" 视图按此过滤创建人。 */
    currentUserId?: string
    origin?: "all" | SalesOrderOrigin
    commercialStatus?: string
    reviewStatus?: string
    fulfillment?: string
    collection?: string
    invoice?: string
    closeStatus?: string
    createdFrom?: number
    createdTo?: number
    sortBy?:
        | "documentNumber"
        | "contractNumber"
        | "amountGross"
        | "ownerName"
        | "submittedAt"
    sortDir?: "asc" | "desc"
}

export type SalesOrderListView = {
    items: SalesOrderListItem[]
    total: number
    page: number
    pageSize: number
    queriedAt: string
}

export const PERMISSION_VERSION = "pv-w05-1"

// ─── 后端原始形状 ────────────────────────────────────────────────────────────

export type PageView<T> = {
    items: T[]
    total: number
    page: number
    page_size: number
}

/**
 * 服务端权威计算的当前阶段（替代前端字符串拼接）；列表与详情共用同一形状
 * ——责任人/时限走整页批量查询，不是逐行查询，列表接口也带得起。
 */
export type BackendStageSummary = {
    code: string
    label: string
    tone: string
    owner_role?: string | null
    owner_user_id?: string | null
    owner_user_name?: string | null
    due_at?: number | null
}

/** 服务端权威计算的结案资格。 */
export type BackendCloseEligibility = {
    fulfillment_complete: boolean
    receivable_settled: boolean
    invoice_complete: boolean
    eligible_to_close: boolean
    blockers: string[]
    note: string
}

export type BackendSalesOrderView = {
    id: string
    order_no: string
    business_type: "VOUCHER" | "GOODS_SERVICE" | string
    origin_system: "MALL" | "ERP" | string
    customer_id: string
    contract_id?: string | null
    commercial_status: string
    review_status: string
    fulfillment_progress: string
    collection_progress: string
    invoice_progress: string
    close_status: string
    effective_at?: number | null
    closed_at?: number | null
    version: number
    created_at: number
    updated_at: number
    stage: BackendStageSummary
}

export type BackendWorkingCopyLine = {
    id: string
    sales_order_line_id: string
    line_no: number
    line_type: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    sales_tax_rate: string
    item_name_snapshot: string
    spec_snapshot?: string | null
    unit_snapshot?: string | null
    sku_id?: string | null
    sku_revision_id?: string | null
    quantity?: string | null
    base_unit_code?: string | null
    unit_price_gross?: string | null
    face_value?: string | null
    card_count?: number | null
    transaction_amount?: string | null
    card_form?: string | null
    fulfillment_due_at?: number | null
}

export type BackendWorkingCopy = {
    id: string
    /** 乐观锁版本；`PUT .../working-copy` 的 `version` 按此比对，不是 `draft_version`。 */
    version: number
    working_purpose: string
    status: string
    draft_version: number
    content_hash: string
    editor_user_id: string
    business_type: string
    customer_name?: string
    contract_no?: string | null
    settlement_party_name?: string | null
    payment_term_code?: string
    payment_term_name?: string
    invoice_type?: string
    tax_point?: string
    project_name?: string | null
    business_remark?: string | null
    voucher_category_sku_id?: string | null
    voucher_expiry_at?: number | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    lines: BackendWorkingCopyLine[]
}

export type BackendSubmission = {
    id: string
    submission_no: number
    status: string
    business_type: string
    customer_name?: string
    contract_no?: string | null
    settlement_party_name?: string | null
    payment_term_code?: string
    payment_term_name?: string
    invoice_type?: string
    tax_point?: string
    project_name?: string | null
    business_remark?: string | null
    voucher_category_sku_id?: string | null
    voucher_expiry_at?: number | null
    gross_amount: string
    net_amount: string
    tax_amount: string
    submitted_by: string
    submitted_at: number
    created_at: number
    lines?: BackendWorkingCopyLine[]
}

export type BackendRevision = {
    id: string
    revision_no: number
    revision_source: string
    content_hash: string
    gross_amount: string
    net_amount: string
    tax_amount: string
    effective_at: number
    created_at: number
}

/** 开放中的采购驳回摘要（销售单详情内嵌，不依赖采购队列权限）。 */
export type BackendOpenProcurementRejection = {
    procurement_confirmation_id: string
    submission_id: string
    reject_reason_code?: string | null
    comment?: string | null
    handled_by?: string | null
    handled_at?: number | null
}

export type BackendSalesOrderDetail = {
    id: string
    order_no: string
    business_type: string
    origin_system: string
    customer_id: string
    contract_id?: string | null
    settlement_party_id: string
    commercial_status: string
    review_status: string
    fulfillment_progress: string
    collection_progress: string
    invoice_progress: string
    close_status: string
    current_revision_id?: string | null
    effective_at?: number | null
    version: number
    created_at: number
    owner_user_id: string
    owner_user_name?: string | null
    lines: Array<{ id: string; line_no: number; line_status: string }>
    working_copy?: BackendWorkingCopy | null
    submissions: BackendSubmission[]
    revisions: BackendRevision[]
    stage: BackendStageSummary
    close_eligibility: BackendCloseEligibility
    can_start_sales_change_order: boolean
    change_order_blocker?: string | null
    open_procurement_rejection?: BackendOpenProcurementRejection | null
}

export type BackendContractDetail = {
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    status: string
    current_revision_id?: string | null
    created_at: number
    version: number
    revisions: Array<{
        id: string
        revision_no: number
        contract_pdf_file_id?: string | null
        customer_name: string
        settlement_party_name: string
        payment_term_code: string
        payment_term_name: string
        invoice_type: string
        tax_point: string
        valid_from: string
        valid_to?: string | null
        signed_at: string
        created_at: number
    }>
}

export type BackendCustomerDetail = {
    id: string
    party_id: string
    customer_no: string
    legal_name?: string | null
}

export type BackendPartyContact = {
    id: string
    contact_name: string
    is_default: boolean
    status: string
}

export type BackendSalesOrderReview = {
    id: string
    sales_order_id: string
    submission_id: string
    review_stage: string
    status: string
    reviewer_id?: string | null
    reviewed_at?: number | null
    created_at: number
}

export type BackendProcurementConfirmation = {
    id: string
    sales_order_id: string
    submission_id: string
    status: string
    handled_by?: string | null
    handled_at?: number | null
    created_at: number
}

export type BackendSalesChangeOrder = {
    id: string
    sales_order_id: string
    base_revision_id: string
    change_type: string
    status: string
    current_submission_id?: string | null
    version: number
    created_at: number
}

export type BackendWorkItem = {
    id: string
    work_item_type: string
    business_object_type: string
    business_object_id: string
    subject_version?: string | null
    status: string
    owner_role?: string | null
    owner_user_id?: string | null
    version: number
    impact_summary?: string | null
    created_at: number
}

export type BackendBackgroundJob = {
    id: string
    job_no?: string
    status?: string
    total_count?: number
    created_at?: number
    version?: number
}

export type ProcurementResolutionOutcome = {
    outcome: "CHANGED_TERMS_RESUBMITTED" | "VOIDED_AFTER_PROCUREMENT_REJECTION"
    reference: string
    detail: string
    newSubmissionNo?: number
    newSubjectHash?: string
    newWorkItemId?: string
    reviewStatus?: "REJECTED" | "RESOLVED" | "VOIDED"
    primaryStatusLabel?: string
}

export type CardApprovalCompleteResult = {
    outcome:
        | "MANAGER_APPROVED"
        | "OPERATIONS_APPROVED_AND_EFFECTIVE"
        | "REJECTED_TO_SALES"
    reference: string
    detail: string
    nextWorkItemId?: string
    primaryStatusLabel?: string
}

export type ExportJobResult = {
    jobId: string
    status: "queued" | "running" | "succeeded" | "failed"
    rowCount: number
    permissionVersion: string
    createdAt: string
    downloadLabel: string
}
