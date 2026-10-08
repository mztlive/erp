import { apiGet, apiPost } from "@/lib/api"
import type { BackendSalesOrderDetail } from "@/features/sales-orders/api/contracts"

export type SalesOrderContractInput = {
    salesOrderId: string
    version: number
    contractId: string
    requestedContractRevisionId: string
}

/** 为无合同销售单首次绑定有效合同；后端在事务内重验关系及商业条款。 */
export async function supplementSalesOrderContract(
    input: SalesOrderContractInput,
): Promise<void> {
    await apiPost<BackendSalesOrderDetail>(
        `/admin/sales-orders/${encodeURIComponent(input.salesOrderId)}/contract`,
        {
            version: input.version,
            contract_id: input.contractId,
            requested_contract_revision_id: input.requestedContractRevisionId,
        },
    )
}

export type ContractBindingCheck = {
    basis: string
    matches: boolean
    items: {
        field: string
        label: string
        sales_value: string
        contract_value: string
        matches: boolean
    }[]
}

/** 展示服务器核对结果，不在浏览器推断或覆盖条款。 */
export async function fetchSalesOrderContractCheck(
    input: SalesOrderContractInput,
): Promise<ContractBindingCheck> {
    return apiGet<ContractBindingCheck>(
        `/admin/sales-orders/${encodeURIComponent(input.salesOrderId)}/contract-check`,
        {
            version: input.version,
            contract_id: input.contractId,
            requested_contract_revision_id: input.requestedContractRevisionId,
        },
        { cache: "no-store" },
    )
}
