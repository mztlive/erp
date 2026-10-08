import type {
    CustomerComboboxItem,
    SettlementPartyComboboxItem,
} from "@/components/business/entity-comboboxes"
import { searchCustomers } from "@/features/entity-selectors/api/customers"
import { searchParties } from "@/features/party-selector/api"
import type { ContractImportTask } from "@/features/contracts/api/upload"
import { getErrorMessage } from "@/lib/api/errors"

export type IdentityMatch<T> = { item?: T; message: string }
export type SalesContractMatches = {
    customer: IdentityMatch<CustomerComboboxItem>
    settlement: IdentityMatch<SettlementPartyComboboxItem>
}
export type SalesContractPrefill = {
    customerId: string
    customerName: string
    settlementPartyId: string
    settlementEntity: string
    paymentTerms: string
    invoiceType: string
    taxRatePercent: string
    file: { id: string; fileName: string }
}

// 目录返回完整、授权后的候选；只接受唯一法定全称，不猜简称，不取第一条。
function unique<T>(
    name: string | null | undefined,
    rows: readonly T[],
    label: (row: T) => string,
): IdentityMatch<T> {
    if (!name?.trim())
        return { message: "合同中未识别到明确名称，请选择系统记录" }
    const matches = rows.filter((row) => label(row).trim() === name.trim())
    if (matches.length === 1)
        return { item: matches[0], message: "已按法定名称匹配，请核对" }
    return {
        message:
            matches.length > 1
                ? "找到多个同名记录，请核对编号后选择"
                : "未找到名称一致的可用记录，请搜索并选择",
    }
}

export async function matchSalesContract(
    task: ContractImportTask,
): Promise<SalesContractMatches> {
    const customerName = task.draft?.fields.customer_name
    const settlementName = task.draft?.fields.settlement_name
    const [customers, parties] = await Promise.allSettled([
        customerName
            ? searchCustomers({
                  query: customerName,
                  scope: "assigned",
                  purpose: "form",
              })
            : Promise.resolve({ items: [] }),
        settlementName
            ? searchParties({ query: settlementName, purpose: "sales-order" })
            : Promise.resolve({ items: [] }),
    ])
    return {
        customer:
            customers.status === "fulfilled"
                ? unique(
                      customerName,
                      customers.value.items,
                      (row) => row.legalName,
                  )
                : {
                      message: getErrorMessage(
                          customers.reason,
                          "客户匹配失败，请在下拉框中重试查询",
                      ),
                  },
        settlement:
            parties.status === "fulfilled"
                ? unique(
                      settlementName,
                      parties.value.items,
                      (row) => row.displayName,
                  )
                : {
                      message: getErrorMessage(
                          parties.reason,
                          "结算主体匹配失败，请在下拉框中重试查询",
                      ),
                  },
    }
}
