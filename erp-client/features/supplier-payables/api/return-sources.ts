import { fetchCompleteList } from "@/lib/collect-pages"
import { fetchSupplierPartyId } from "@/features/entity-selectors/api/suppliers"
import {
    projectInvoice,
    projectPayment,
    type BackendInvoice,
    type BackendSupplierPayment,
} from "./mappers"
import type { PaymentRow, PurchaseInvoiceRow } from "../types"

/** 退货款和红票分别读取各自来源，不要求经办人同时具备付款、开票权限。 */
export async function fetchReturnSources(
    supplierId: string,
    invoice: boolean,
): Promise<{ payments: PaymentRow[]; invoices: PurchaseInvoiceRow[] }> {
    if (invoice) {
        const partyId = await fetchSupplierPartyId(supplierId)
        const page = await fetchCompleteList<BackendInvoice>(
            "/admin/invoices",
            {
                invoice_direction: "purchase",
                party_id: partyId,
                page: 1,
                page_size: 100,
                sort_by: "invoice_date",
                sort_dir: "desc",
            },
        )
        return { payments: [], invoices: page.items.map(projectInvoice) }
    }
    const page = await fetchCompleteList<BackendSupplierPayment>(
        "/admin/supplier-payments",
        {
            supplier_id: supplierId,
            page: 1,
            page_size: 100,
            sort_by: "paid_at",
            sort_dir: "desc",
        },
    )
    return { payments: page.items.map(projectPayment), invoices: [] }
}
