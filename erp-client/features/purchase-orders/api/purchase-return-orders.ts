import { apiGet } from "@/lib/api"
import type { BackendPurchaseReturnOrderPage } from "@/features/purchase-orders/api/purchase-return-order-wire-types"
import {
    purchaseReturnModeLabel,
    purchaseReturnOrderStatusLabel,
    purchaseReturnOrderStatusTone,
    stripPurchaseReturnApprovalField,
} from "@/features/purchase-orders/lib/purchase-return-order-no-approval"
import { secsToIso } from "@/features/purchase-orders/api/purchase-order-status"
import type { PurchaseReturnOrderRow } from "@/features/purchase-orders/types"

const PURCHASE_RETURN_ORDER_MAX_PAGE_SIZE = 100

/**
 * 把采购退货详情投影为列表/详情行。PurchaseReturnOrder 为 NO_APPROVAL，
 * 丢弃误带的审批字段；PENDING_EXECUTION 只映射为待执行。
 *
 * @param dto 采购退货 HTTP 载荷。
 */
export function projectPurchaseReturnOrder(
    dto: BackendPurchaseReturnOrderPage["items"][number],
): PurchaseReturnOrderRow {
    const row = stripPurchaseReturnApprovalField(dto)
    return {
        purchaseReturnOrderId: row.id,
        purchaseReturnNo: row.purchase_return_no,
        purchaseOrderId: row.purchase_order_id,
        salesReturnCaseId: row.sales_return_case_id ?? undefined,
        returnMode: row.return_mode,
        returnModeLabel: purchaseReturnModeLabel(row.return_mode),
        status: row.status,
        statusLabel: purchaseReturnOrderStatusLabel(row.status),
        statusTone: purchaseReturnOrderStatusTone(row.status),
        version: row.version,
        createdAt: secsToIso(row.created_at),
        allowedActions: ["VIEW_DETAIL"],
    }
}

export type PurchaseReturnOrderListResult = {
    rows: PurchaseReturnOrderRow[]
    emptyReason?: string | null
    scopeVersion?: string
    policyVersion?: number
    organizationVersion?: number
    scopeSummary?: string
}

/**
 * 按原采购单读取关联采购退货。PurchaseReturnOrder 为 NO_APPROVAL，
 * 列表投影不含审批绑定。
 *
 * @param purchaseOrderId 原采购单 ID。
 * @param scopeVersion 跨页必须回传的范围版本。
 */
export async function fetchPurchaseReturnOrders(
    purchaseOrderId: string,
    scopeVersion?: string,
): Promise<PurchaseReturnOrderListResult> {
    const page = await apiGet<BackendPurchaseReturnOrderPage>(
        "/admin/purchase-return-orders",
        {
            purchase_order_id: purchaseOrderId,
            scope_version: scopeVersion,
            page: 1,
            page_size: PURCHASE_RETURN_ORDER_MAX_PAGE_SIZE,
            sort_by: "created_at",
            sort_dir: "desc",
        },
    )
    return {
        rows: (page.items ?? []).map(projectPurchaseReturnOrder),
        emptyReason: page.empty_reason,
        scopeVersion: page.scope_version,
        policyVersion: page.policy_version,
        organizationVersion: page.organization_version,
        scopeSummary: page.scope_summary,
    }
}
