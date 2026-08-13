/** W12 供应商往来 · URL 查询参数解析（纯函数）。 */

import type { SupplierAccountsView } from "@/features/supplier-payables/types"

export function parseView(raw: string | null): SupplierAccountsView {
    if (
        raw === "payment" ||
        raw === "purchase_invoice" ||
        raw === "unallocated" ||
        raw === "payable"
    ) {
        return raw
    }
    return "payable"
}
