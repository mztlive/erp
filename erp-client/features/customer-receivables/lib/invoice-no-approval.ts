import type { BackendInvoice } from "@/features/customer-receivables/api/dto"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"

/** 发票作为合同 DocumentType 的固定值。 */
export const INVOICE_DOCUMENT_TYPE = "Invoice" as const

/** 工作项上的发票对象类型；与合同 DocumentType 并存时只认这两种字面量。 */
export const INVOICE_OBJECT_TYPE = "invoice" as const

/** 合同 §4.3 对 Invoice 的固定政策。 */
export const INVOICE_APPROVAL_REQUIREMENT = "NO_APPROVAL" as const

type ForbidKey<T, K extends string> = K extends keyof T ? never : true

/** 编译期证明：发票 HTTP DTO 不得携带审批绑定。 */
export const INVOICE_DTO_HAS_NO_APPROVAL: ForbidKey<
    BackendInvoice,
    "approval"
> = true

/** 编译期证明：发票行投影不得嵌入审批区。 */
export const INVOICE_ROW_HAS_NO_APPROVAL: ForbidKey<
    SalesInvoiceRow,
    "approval"
> = true

/** 发票业务动作白名单；不得混入审批决定或流程写入口。 */
const INVOICE_BUSINESS_ACTIONS = new Set([
    "VIEW_DETAIL",
    "CONTINUE_ALLOCATE",
    "ISSUE_RED_INVOICE",
    "REGISTER_INVOICE",
])

/**
 * 判断当前任务是否属于发票。发票为无需审批类型，不得当作审批待办。
 *
 * @param workItem 工作项投影；只读 `businessObjectType`。
 * @returns 对象类型为 Invoice / invoice 时为 true。
 */
export const isInvoiceWorkItem = (workItem?: {
    businessObjectType?: string
}): boolean =>
    workItem?.businessObjectType === INVOICE_DOCUMENT_TYPE ||
    workItem?.businessObjectType === INVOICE_OBJECT_TYPE

/**
 * 丢弃发票 DTO 上误带的审批字段。Invoice 为 NO_APPROVAL，禁止把绑定带入投影。
 *
 * @param dto 发票 HTTP 载荷。
 * @returns 不含 `approval` 的对象。
 */
export function stripInvoiceApprovalField<T extends object>(
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
 * 发票允许动作是否只含业务登记/分配/红票，不含审批入口。
 *
 * @param actions 行上的允许动作。
 * @returns 全部为发票业务动作时为 true。
 */
export const invoiceActionsExcludeApproval = (
    actions: readonly string[],
): boolean => actions.every((action) => INVOICE_BUSINESS_ACTIONS.has(action))
