/** 销售接口白名单：不包含供应商、原始报价、内部附件或收款资料。 */
export type InquiryStatus =
    | "draft"
    | "awaiting_procurement"
    | "procurement_processing"
    | "awaiting_sales_supplement"
    | "awaiting_sales"
    | "confirmed"
    | "closed"
export type InquiryFulfillmentKind = "physical" | "virtual" | "service"
export type InquiryConclusion =
    | "available"
    | "partially_available"
    | "unavailable"
    | "pending"
export type InquiryAllocation = {
    allocation_id: string
    recipient: string
    phone: string
    region_code: string
    region_name: string
    address: string
    quantity: string
}
export type InquiryLine = {
    line_id: string
    sku_id: string
    sku_revision_id: string
    sku_code: string
    product_name: string
    specification: string
    unit_code: string
    quantity_scale: number
    quantity: string
    fulfillment_kind: InquiryFulfillmentKind
    delivery_due_at: number
    delivery_terms: string
    sales_reference_price_gross?: string | null
    allocations: InquiryAllocation[]
}
export type InquiryRequirement = { lines: InquiryLine[]; note: string }
export type InquiryPrice = {
    price_id: string
    unit_price_gross: string | null
    tax_rate: string | null
    valid_until: number
}
export type InquiryFeedbackLine = {
    line_id: string
    conclusion: InquiryConclusion
    available_quantity: string
    prices: InquiryPrice[]
    public_note: string
}
export type InquiryFee = {
    fee_id: string
    name: string
    amount_gross: string
    tax_rate: string
    line_ids: string[]
    valid_until: number
    public_note: string
}
export type InquiryFeedbackData = {
    lines: InquiryFeedbackLine[]
    fees: InquiryFee[]
}
export type InquiryFeedback = {
    round: number
    requirement_version: number
    published_by: string
    published_at: number
    data: InquiryFeedbackData
}
export type InquiryReservationState = "reserved" | "handled" | "released"
export type InquiryFeeAllocation = {
    allocation_id: string
    sales_order_id: string
    feedback_round: number
    fee_id: string
    amount: string
}
/** 对销售开放的询价费用进度，不包含采购单、采购行或收款身份。 */
export type InquiryFeeReservation = {
    allocation_id: string
    fee_id: string
    sales_order_id: string
    amount: string
    state: InquiryReservationState
}
export type InquiryQuantityReservation = {
    reservation_id: string
    sales_order_id: string
    requirement_version: number
    feedback_round: number
    selections: {
        line_id: string
        allocation_id?: string | null
        price_id?: string | null
        quantity: string
    }[]
    state: InquiryReservationState
}
export type InquiryFeeSalesSource = {
    id: string
    order_no: string
    status_label: string
    eligible_for_fee_allocation: boolean
}
export type InquiryFeeStatus = {
    fee_id: string
    feedback_round: number
    unavailable_reason: string | null
}
export type InquirySalesProgress = {
    transfer_status: "not_started" | "partial" | "complete"
    total_lines: number
    completed_lines: number
    quote_validity: "unavailable" | "valid" | "partially_expired" | "expired"
    can_create_sales: boolean
}
export type SalesInquiry = {
    id: string
    version: number
    inquiry_no: string
    customer_id: string
    customer_name: string
    sales_owner_user_id: string
    business_org_unit_id: string
    shared_fee_owner_user_id?: string | null
    shared_fee_owner_name?: string | null
    deadline_at: number
    status: InquiryStatus
    requirement_version: number
    feedback_round: number
    draft: InquiryRequirement
    requirements: {
        version: number
        submitted_at: number
        change_reason: string
        data: InquiryRequirement
    }[]
    feedbacks: InquiryFeedback[]
    acceptances: {
        requirement_version: number
        feedback_round: number
        accepted_at: number
    }[]
    reinquiries: {
        at: number
        previous_feedback_round: number
        data: {
            reason: string
            concerns: string
            line_ids: string[]
            fee_ids: string[]
        }
    }[]
    events: { kind: string; reason: string; at: number }[]
    sales_progress: InquirySalesProgress
    copied_from_inquiry_id: string | null
    quantity_ledger: InquiryQuantityReservation[]
    fee_allocations: InquiryFeeAllocation[]
    fee_ledger: InquiryFeeReservation[]
    fee_statuses: InquiryFeeStatus[]
}
export type ProcurementInquiry = Omit<
    SalesInquiry,
    | "sales_owner_user_id"
    | "business_org_unit_id"
    | "acceptances"
    | "reinquiries"
    | "events"
    | "sales_progress"
    | "copied_from_inquiry_id"
    | "quantity_ledger"
    | "fee_ledger"
    | "fee_allocations"
> & {
    draft: InquiryRequirement
    reinquiry: {
        at: number
        previous_feedback_round: number
        reason: string
        concerns: string
        all_items: boolean
        line_ids: string[]
        fee_ids: string[]
    } | null
    requirements: SalesInquiry["requirements"]
    fee_allocations: InquiryFeeAllocation[]
    fee_ledger: InquiryFeeReservation[]
    fee_statuses: InquiryFeeStatus[]
    feedback_work: {
        requirement_version: number
        actor_id: string
        line_ids: string[]
        data: InquiryFeedbackData
        saved_at: number
        frozen: boolean
    }[]
    work_items: {
        id: string
        version: number
        owner_user_id: string
        line_ids: string[]
        status: "OPEN" | "COMPLETED"
        subject_version: string
    }[]
}
export type InquiryListQuery = {
    task_status?: "open" | "completed"
    q?: string
    status?: string
    page: number
    page_size: number
    scope_version?: string
}
export type InquiryList = {
    items: (SalesInquiry | ProcurementInquiry)[]
    total: number
    page: number
    page_size: number
    scope_version: string
}

export const INQUIRY_STATUS_LABEL: Record<InquiryStatus, string> = {
    draft: "草稿",
    awaiting_procurement: "采购处理中",
    procurement_processing: "采购处理中",
    awaiting_sales_supplement: "待销售补充",
    awaiting_sales: "待销售处理",
    confirmed: "已确认",
    closed: "已关闭",
}
export const INQUIRY_CONCLUSION_LABEL: Record<InquiryConclusion, string> = {
    available: "可供应",
    partially_available: "部分可供应",
    unavailable: "不供应",
    pending: "待确认",
}
