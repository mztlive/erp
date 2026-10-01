import type { BackendInvoice } from "@/features/customer-receivables/api/dto"
import type { SalesInvoiceRow } from "@/features/customer-receivables/types"

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
