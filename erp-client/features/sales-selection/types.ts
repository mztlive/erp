export type SelectionForm = "SINGLE_SKU" | "PACKAGE"
export type SubmitMode = "BY_QUANTITY" | "MALL_REDEEM"
export type PoolSourceKind = "FILTER" | "SELECTION"
export type BookletStatus =
    | "DRAFT"
    | "PREPARING"
    | "PENDING_PUBLISH"
    | "PUBLISHED"
    | "SUBMITTED"
    | "CLOSED"
    | "VOIDED"
export type PublicPageKind = "SELECTING" | "RECEIPT" | "ENDED"

export type CreateTierInput = {
    name: string
    target_amount: string
    tolerance: string
    expected_count: number
    sku_count: number
}

export type PoolFilterSnapshot = {
    nationwide_only?: boolean
    q?: string
    product_kind?: string
    category_id?: string
    brand_id?: string
    supplier_id?: string
    supply_region?: string
    max_supplier_count?: number
    sales_price_min?: string
    sales_price_max?: string
}

export type BookletListItem = {
    id: string
    version: number
    customer_id: string
    customer_name: string
    /** 显式销售负责人；方案沿所属册继承，客户提交人不成为负责人。 */
    sales_owner_user_id: string
    /** 单据业务组织；范围按该口径解释。 */
    business_org_unit_id: string
    form: SelectionForm
    submit_mode: SubmitMode
    status: BookletStatus
    proposal_id?: string | null
    proposal_no?: string | null
    display_count?: number
    created_at: number
}

export type DisplayMemberView = {
    sku_id: string
    name: string
    specification: { name: string; value: string }[]
    unit: string
    price: string
}

export type DisplayItemView = {
    id: string
    item_id: string
    kind: "SINGLE_SKU" | "PACKAGE"
    removed: boolean
    tier_id?: string | null
    tier_name?: string | null
    name: string
    specification: { name: string; value: string }[]
    spec_label: string
    price: string
    price_gross: string
    target_delta?: string | null
    cover_asset_id?: string | null
    cover_image?: string | null
    unit?: string | null
    members: DisplayMemberView[]
    missing_image: boolean
}

export type PublicDisplayItemView = {
    item_id: string
    tier_name?: string | null
    name: string
    specification: { name: string; value: string }[]
    price: string
    cover_path?: string | null
    members: Omit<DisplayMemberView, "sku_id">[]
}

export type PublicReceiptView = {
    proposal_no: string
    submitted_at: number
    customer_name: string
    items: PublicChoiceView[]
    total_amount?: string | null
}

export type BookletView = {
    pool_filter?: PoolFilterSnapshot | null
    sku_ids?: string[]
    id: string
    version: number
    customer_id: string
    customer_name: string
    /** 显式销售负责人。 */
    sales_owner_user_id: string
    /** 负责人显示名；缺失账号时省略。 */
    sales_owner_name?: string | null
    /** 单据业务组织。 */
    business_org_unit_id: string
    form: SelectionForm
    submit_mode: SubmitMode
    status: BookletStatus
    pool_source_kind: PoolSourceKind
    tiers: Array<{
        tier_id: string
        name: string
        target_amount: string
        tolerance: string
        expected_count: number
        sku_count: number
    }>
    batch_id?: string | null
    eligibility_as_of?: string | null
    prepared_at?: number | null
    display_count: number
    removed_count: number
    missing_image_count: number
    last_prepare_failure?: string | null
    prepare_stage?: string | null
    completed_tier_count?: number | null
    tier_reports: Array<{
        tier_id: string
        expected_count: number
        actual_count: number
        stop_reason: string
        stop_label: string
        image_failures: number
    }>
    items: DisplayItem[]
    link_expires_at?: number | null
    link_revoked: boolean
    proposal_id?: string | null
    public_path?: string | null
}

export type PublicChoiceView = {
    item_id: string
    quantity?: number | null
    line_amount?: string | null
}

export type PublicPageView = {
    kind: PublicPageKind
    customer_name?: string | null
    form?: SelectionForm | null
    submit_mode?: SubmitMode | null
    session_version?: number | null
    items: PublicDisplayItemView[]
    choices: PublicChoiceView[]
    total_amount?: string | null
    receipt?: PublicReceiptView | null
    notices: string[]
}

export type ProposalView = {
    id: string
    proposal_no: string
    customer_id: string
    customer_name: string
    booklet_id: string
    /** 继承所属册的显式销售负责人。 */
    sales_owner_user_id: string
    /** 继承所属册的业务组织。 */
    business_org_unit_id: string
    form: SelectionForm
    submit_mode: SubmitMode
    submitted_at: number
    source: string
    total_amount?: string | null
    display_lines: Array<{
        display_item_id: string
        tier_id?: string | null
        quantity?: number | null
        unit_price: string
        line_amount?: string | null
        cover_asset_id?: string | null
    }>
    sku_lines: Array<{
        specification?: { name: string; value: string }[]
        unit?: string
        display_item_id: string
        name: string
        quantity?: number | null
        unit_price: string
        line_amount?: string | null
    }>
}

export const BOOKLET_STATUS_LABEL: Record<BookletStatus, string> = {
    DRAFT: "草稿",
    PREPARING: "准备中",
    PENDING_PUBLISH: "待发布",
    PUBLISHED: "已发布",
    SUBMITTED: "已提交",
    CLOSED: "已关闭",
    VOIDED: "已作废",
}

export const FORM_LABEL: Record<SelectionForm, string> = {
    SINGLE_SKU: "单品",
    PACKAGE: "套餐",
}

export const SUBMIT_MODE_LABEL: Record<SubmitMode, string> = {
    BY_QUANTITY: "按份采购",
    MALL_REDEEM: "商城兑换",
}

export const SELECTION_FORM_LABEL = FORM_LABEL
export const POOL_SOURCE_LABEL: Record<PoolSourceKind, string> = {
    FILTER: "当前筛选",
    SELECTION: "当前勾选",
}

export type BookListQuery = {
    q?: string
    customer_id?: string
    selection_form?: SelectionForm | "ALL"
    submit_mode?: SubmitMode | "ALL"
    status?: BookletStatus | "ALL"
    page?: number
    page_size?: number
    /** 当前业务负责人；逗号分隔的稳定人员 ID，进入 URL 与 QueryKey。 */
    owner_user_ids?: string
    /** 当前组织筛选；逗号分隔的组织 ID，进入 URL 与 QueryKey。 */
    org_unit_ids?: string
    /** 是否包含有效下级；缺省为 false。 */
    include_descendants?: boolean
    /** 跨页范围版本；由列表基线自动携带，不手填。 */
    scope_version?: string
}

/**
 * 负责人候选：只含稳定 ID 与显示名（含账号状态后缀）。
 * 来自当页同一授权快照，不授予命令资格。
 */
export type SelectionOwnerOption = {
    value: string
    label: string
}

/** 选品册列表结果：分页、候选、范围版本与无范围标记。 */
export type BookListResult = {
    rows: SelectionBook[]
    total: number
    ownerOptions: SelectionOwnerOption[]
    scopeVersion: string
    policyVersion: number
    organizationVersion: number
    /** 无范围时为 true；前端据此与筛空区分展示。 */
    noScope: boolean
}

export type SelectionBook = BookletListItem & {
    selection_form: SelectionForm
    book_id?: string
}

export type SelectionBookDetail = BookletView & {
    book_id: string
    selection_form: SelectionForm
    source_kind: PoolSourceKind
    public_url?: string | null
    proposal_no?: string | null
    updated_at?: number | string
}

export type CreateBookInput = {
    customer_id: string
    /** 显式销售负责人；必填，客户提交人不成为负责人。 */
    sales_owner_user_id: string
    /** 业务组织；必填，取负责人有效主属组织。 */
    business_org_unit_id: string
    selection_form: SelectionForm
    submit_mode: SubmitMode
    source_kind: PoolSourceKind
    filter?: PoolFilterSnapshot
    sku_ids?: readonly string[]
    tiers?: CreateTierInput[]
    idempotency_key: string
}

export type TierRuleInput = CreateTierInput & { tier_id?: string }

export type DisplayItem = {
    id: string
    item_id: string
    kind: "SINGLE_SKU" | "PACKAGE"
    removed: boolean
    tier_id?: string | null
    tier_name?: string | null
    name: string
    specification: { name: string; value: string }[]
    spec_label: string
    price: string
    price_gross: string
    target_delta?: string | null
    cover_asset_id?: string | null
    cover_image?: string | null
    unit?: string | null
    members: DisplayMemberView[]
    missing_image: boolean
}

export type SessionSelection = {
    item_id: string
    quantity?: number | null
}

export type SelectionProposal = ProposalView
export type PublicSelection = PublicPageView
export type PublicDisplayItem = PublicDisplayItemView
export type SaveSessionInput = {
    token: string
    expected_session_version: number
    idempotency_key: string
    choices: SessionSelection[]
}
export type SubmitSelectionInput = {
    token: string
    expected_session_version: number
    idempotency_key: string
}
