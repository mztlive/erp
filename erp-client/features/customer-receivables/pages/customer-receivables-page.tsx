"use client"

import { CustomerReceivablesWorkspace } from "@/features/customer-receivables/components/customer-receivables-workspace"
import { FinancialDraftEditDialog } from "@/features/financial-draft-edit/components/financial-draft-edit-dialog"

/** 客户往来独立页面。业务内容与销售单详情共用同一工作区。 */
export function CustomerReceivablesPage() {
    return (
        <>
            <CustomerReceivablesWorkspace />
            <FinancialDraftEditDialog side="customer" />
        </>
    )
}
