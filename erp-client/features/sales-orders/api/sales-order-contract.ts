import { apiPost } from "@/lib/api"
import type { BackendSalesOrderDetail } from "@/features/sales-orders/api/contracts"

/** 为无合同销售单首次绑定有效合同；后端核对版本、客户与结算主体。 */
export async function supplementSalesOrderContract(input: {
    salesOrderId: string
    version: number
    contractId: string
    requestedContractRevisionId: string
}): Promise<void> {
    await apiPost<BackendSalesOrderDetail>(
        `/admin/sales-orders/${encodeURIComponent(input.salesOrderId)}/contract`,
        {
            version: input.version,
            contract_id: input.contractId,
            requested_contract_revision_id: input.requestedContractRevisionId,
        },
    )
}
