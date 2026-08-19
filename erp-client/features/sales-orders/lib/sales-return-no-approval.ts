import type {
    BackendSalesReturnCase,
    SalesReturnCaseRow,
    SalesReturnCaseType,
    SalesReturnRoute,
} from "@/features/sales-orders/api/sales-return-cases"

/** 销售退货单作为合同 DocumentType 的固定值。 */
export const SALES_RETURN_CASE_DOCUMENT_TYPE = "SalesReturnCase" as const

/** 工作项上的销售退货对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const SALES_RETURN_CASE_OBJECT_TYPE = "sales_return_case" as const

/** 合同 §4.3 对 SalesReturnCase 的固定政策。 */
export const SALES_RETURN_CASE_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：销售退货 HTTP DTO 不得携带审批绑定。 */
export const SALES_RETURN_CASE_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendSalesReturnCase,
    "approval"
> = true

/** 编译期证明：销售退货行投影不得嵌入审批区。 */
export const SALES_RETURN_CASE_ROW_HAS_NO_APPROVAL: ForbidKey<
    SalesReturnCaseRow,
    "approval"
> = true

/** 仓储/采购/财务待处理是履约分工态，不是审批复核。 */
export const SALES_RETURN_CASE_FULFILLMENT_DIVISION_STATUSES = [
    "PENDING_WAREHOUSE_ACCEPTANCE",
    "pending_warehouse_acceptance",
    "PENDING_PROCUREMENT",
    "pending_procurement",
    "PENDING_FINANCE",
    "pending_finance",
] as const

/** 销售退货业务动作白名单；不得混入审批决定或流程写入口。 */
const SALES_RETURN_CASE_BUSINESS_ACTIONS = new Set(["VIEW_DETAIL"])

const APPROVAL_REVIEW_LABELS = new Set([
    "审批复核",
    "待审批",
    "审批中",
    "待财务复核",
    "待仓储复核",
    "待采购复核",
])

/**
 * 判断当前任务是否属于销售退货单。SalesReturnCase 为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 SalesReturnCase / sales_return_case 时为 true。
 */
export const isSalesReturnCaseWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === SALES_RETURN_CASE_DOCUMENT_TYPE ||
    workItem?.businessObjectType === SALES_RETURN_CASE_OBJECT_TYPE

/**
 * 判断处理单状态是否为履约与执行分工态。
 *
 * `PENDING_WAREHOUSE_ACCEPTANCE` / `PENDING_PROCUREMENT` / `PENDING_FINANCE`
 * 只表示仓储验收、采购处理或财务处理，不是审批复核。
 *
 * @param status 服务端状态码，兼容 snake_case 与 SCREAMING_SNAKE。
 * @returns 属于履约分工态时为 true。
 */
export const isSalesReturnCaseFulfillmentDivisionStatus = (
    status?: string,
): boolean =>
    SALES_RETURN_CASE_FULFILLMENT_DIVISION_STATUSES.some(
        (code) => code === status,
    )

/**
 * 销售退货处理单状态永远不是审批复核。合同签署为 NO_APPROVAL，
 * 禁止把履约分工态当成 `IN_APPROVAL` / 待复核。
 *
 * @param _status 服务端状态码；任意值都不得视为审批复核。
 * @returns 恒为 false。
 */
export const isSalesReturnCaseApprovalReviewStatus = (
    _status?: string,
): boolean => false

/**
 * 把销售退货处理类型映射为用户可见中文，不上屏枚举原值。
 *
 * @param caseType 服务端处理类型。
 */
export const salesReturnCaseTypeLabel = (caseType?: string): string => {
    switch (caseType) {
        case "return":
            return "退货"
        case "reject":
            return "拒收"
        case "shortage":
            return "短少"
        case "service_failed":
            return "服务不通过"
        default:
            return "退货"
    }
}

/**
 * 把退货路线映射为用户可见中文，不上屏枚举原值。
 *
 * @param route 服务端退货路线。
 */
export const salesReturnRouteLabel = (route?: string): string => {
    switch (route) {
        case "company_warehouse":
            return "退公司仓"
        case "direct_to_supplier":
            return "直退供应商"
        case "no_physical_return":
            return "不发生实物退回"
        default:
            return "不发生实物退回"
    }
}

/**
 * 把销售退货处理单状态映射为用户可见中文。
 *
 * 待仓储验收 / 待采购处理 / 待财务处理是履约分工，不得写成审批复核。
 *
 * @param status 服务端状态码；兼容 snake_case 与 SCREAMING_SNAKE。
 */
export const salesReturnCaseStatusLabel = (status?: string): string => {
    switch (status) {
        case "DRAFT":
        case "draft":
            return "草稿"
        case "PENDING_WAREHOUSE_ACCEPTANCE":
        case "pending_warehouse_acceptance":
            return "待仓储验收"
        case "PENDING_PROCUREMENT":
        case "pending_procurement":
            return "待采购处理"
        case "PENDING_FINANCE":
        case "pending_finance":
            return "待财务处理"
        case "PROCESSING":
        case "processing":
            return "处理中"
        case "COMPLETED":
        case "completed":
            return "已完成"
        case "VOIDED":
        case "voided":
            return "作废"
        default:
            return "处理中"
    }
}

/**
 * 判断状态文案是否误写成审批复核。销售退货状态标签不得落入该集合。
 *
 * @param label 用户可见状态文案。
 * @returns 文案属于审批复核口径时为 true。
 */
export const salesReturnCaseStatusLabelIsApprovalReview = (
    label: string,
): boolean => APPROVAL_REVIEW_LABELS.has(label)

/**
 * 丢弃销售退货 DTO 上误带的审批字段。SalesReturnCase 为 NO_APPROVAL，
 * 禁止把绑定带入投影。
 *
 * @param dto 销售退货 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripSalesReturnCaseApprovalField<T extends object>(
    dto: T,
): Omit<T, "approval"> {
    if (!("approval" in dto)) {
        return dto
    }
    const { approval: _discarded, ...rest } = dto as T & {
        approval?: unknown
    }
    void _discarded
    return rest
}

/**
 * 销售退货允许动作是否只含业务查看入口，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为销售退货业务动作时为 true。
 */
export const salesReturnCaseActionsExcludeApproval = (
    actions: readonly string[],
): boolean =>
    actions.every((action) => SALES_RETURN_CASE_BUSINESS_ACTIONS.has(action))

export type { SalesReturnCaseType, SalesReturnRoute }
