import { scopeText } from "@/lib/ui-text"

import { MoneyValue } from "./values"

/** 资金范围列表的整单/未分配双行金额：受限时两行一起置空并说明原因。 */
export function wholeUnallocatedCell({
    unallocated,
    whole,
    permissionLimited,
}: {
    unallocated: string | null
    whole: string | null
    permissionLimited: boolean
}) {
    const unavailableReason = permissionLimited
        ? scopeText.wholeRestricted
        : undefined
    return (
        <div className="text-right">
            <div>
                <span className="mr-2 text-xs text-muted-foreground">
                    未分配
                </span>
                <MoneyValue
                    value={unallocated}
                    unavailableReason={unavailableReason}
                />
            </div>
            <div className="mt-1">
                <span className="mr-2 text-xs text-muted-foreground">整单</span>
                <MoneyValue
                    value={whole}
                    unavailableReason={unavailableReason}
                />
            </div>
        </div>
    )
}
