import type { ContractComboboxItem } from "@/components/business/entity-comboboxes"
import { fetchSelectorList } from "@/lib/selector-list"

import type { ContractSearch } from "./types"

type ContractDto = Readonly<{
    id: string
    contract_no: string
    customer_id: string
    settlement_party_id: string
    status: string
    current_revision_id?: string | null
    current_revision?: ContractRevisionDto | null
}>

type ContractRevisionDto = Readonly<{
    id: string
    revision_no: number
    customer_name: string
    settlement_party_name: string
    valid_to?: string | null
}>

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

function contractItem(row: ContractDto): ContractComboboxItem {
    const revision = row.current_revision ?? undefined
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
    const page = await fetchSelectorList<ContractDto>("/admin/contracts", {
        q: input.query.trim() || undefined,
        customer_id: input.customerId || undefined,
        scope: input.scope,
        status: input.selectableOnly ? "EFFECTIVE" : undefined,
        sort_by: "created_at",
        sort_dir: "desc",
    })
    return page.items.map(contractItem)
}

export async function fetchContractOption(
    contractId: string,
    input: Omit<ContractSearch, "query"> = { purpose: "filter" },
): Promise<ContractComboboxItem | null> {
    if (!contractId) return null
    const rows = await searchContracts({ ...input, query: "" })
    return rows.find((row) => row.contractId === contractId) ?? null
}
