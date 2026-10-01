import type {
    BackendSalesReturnCase,
    SalesReturnCaseRow,
    SalesReturnCaseType,
    SalesReturnRoute,
} from "@/features/sales-orders/api/sales-return-cases"

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

export type { SalesReturnCaseType, SalesReturnRoute }
