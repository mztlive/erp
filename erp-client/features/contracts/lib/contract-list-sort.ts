import type { SortingState } from "@tanstack/react-table"

import type { ContractListRow } from "@/features/contracts/types"

/** 表头排序列 → 全量排序键（对整表排序后再分页，杜绝「当前页伪排序」）。 */
export function sortRows(
    rows: readonly ContractListRow[],
    sorting: SortingState,
): ContractListRow[] {
    const sorted = [...rows]
    if (sorting.length === 0) {
        // 默认：将到期优先，再按有效期止升序（与列表描述文案一致）。
        return sorted.sort((a, b) => {
            if (a.expiringWithin30Days !== b.expiringWithin30Days) {
                return a.expiringWithin30Days ? -1 : 1
            }
            return a.validTo.localeCompare(b.validTo)
        })
    }
    const { id, desc } = sorting[0]
    const dir = desc ? -1 : 1
    return sorted.sort((a, b) => {
        let cmp = 0
        switch (id) {
            case "contractNo":
                cmp = a.contractNo.localeCompare(b.contractNo)
                break
            case "customer":
                cmp = a.customer.displayName.localeCompare(
                    b.customer.displayName,
                )
                break
            case "settlement":
                cmp = a.settlementParty.displayName.localeCompare(
                    b.settlementParty.displayName,
                )
                break
            case "validity":
                cmp = a.validTo.localeCompare(b.validTo)
                break
            case "revision":
                cmp = a.revisionNo - b.revisionNo
                break
            case "sales":
                cmp = a.salesOrderCount - b.salesOrderCount
                break
            case "owner":
                cmp = a.ownerLabel.localeCompare(b.ownerLabel)
                break
        }
        return cmp * dir
    })
}
