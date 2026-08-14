import type { ContractComboboxItem } from "@/components/business/entity-comboboxes"
import { apiGet } from "@/lib/api"
import type { Page } from "@/lib/api/paging"

import { OPTION_PAGE_SIZE } from "./shared"
import type { ContractSearch } from "./types"

type ContractDto = Readonly<{
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    status: string
    current_revision_id?: string | null
}>

type ContractRevisionDto = Readonly<{
    id: string
    revision_no: number
    customer_name: string
    settlement_party_name: string
    valid_to?: string | null
}>

type ContractDetailDto = ContractDto &
    Readonly<{ revisions: readonly ContractRevisionDto[] }>

function contractStatus(status: string) {
    switch (status.toUpperCase()) {
        case "EFFECTIVE":
            return { label: "生效中", tone: "success" as const }
        case "TERMINATED":
            return { label: "已终止", tone: "destructive" as const }
        default:
            return { label: "已到期", tone: "neutral" as const }
    }
}

async function contractItem(row: ContractDto): Promise<ContractComboboxItem> {
    let revision: ContractRevisionDto | undefined
    try {
        const detail = await apiGet<ContractDetailDto>(
            `/admin/contracts/${encodeURIComponent(row.id)}`,
        )
        revision =
            detail.revisions.find(
                (item) => item.id === detail.current_revision_id,
            ) ?? detail.revisions[0]
    } catch {
        // 合同稳定编号和状态仍可用于选择；修订摘要按权限降级。
    }
    const status = contractStatus(row.status)
    return {
        contractId: row.id,
        contractNo: row.contract_no,
        customerName: revision?.customer_name ?? row.customer_id,
        statusLabel: status.label,
        statusTone: status.tone,
        revisionNo: revision?.revision_no,
        validTo: revision?.valid_to ?? undefined,
        settlementPartyName: revision?.settlement_party_name,
    }
}

export async function searchContracts(
    input: ContractSearch,
): Promise<readonly ContractComboboxItem[]> {
    const page = await apiGet<Page<ContractDto>>("/admin/contracts", {
        contract_no: input.query.trim() || undefined,
        customer_id: input.customerId || undefined,
        scope: input.scope,
        status: input.selectableOnly ? "EFFECTIVE" : undefined,
        page: 1,
        page_size: OPTION_PAGE_SIZE,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return Promise.all(page.items.map(contractItem))
}

export async function fetchContractOption(
    contractId: string,
    input: Pick<ContractSearch, "scope"> = {},
): Promise<ContractComboboxItem | null> {
    if (!contractId) return null
    try {
        const row = await apiGet<ContractDto>(
            `/admin/contracts/${encodeURIComponent(contractId)}`,
        )
        if (input.scope === "assigned") {
            const page = await apiGet<Page<ContractDto>>("/admin/contracts", {
                customer_id: row.customer_id,
                scope: "assigned",
                page: 1,
                page_size: 1,
            })
            if (page.total <= 0) {
                return null
            }
        }
        return contractItem(row)
    } catch {
        return null
    }
}
