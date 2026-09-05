"use client"

import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { VIEW_LABEL } from "@/features/inventory/types"
import type { InventoryView } from "@/features/inventory/types"

interface LedgerViewTabsProps {
    view: InventoryView
    total: number
    onViewChange: (nextView: InventoryView) => void
}

const LEDGER_VIEWS = [
    "balance",
    "movement",
    "reservation",
    "adjustment",
] as const

export function LedgerViewTabs({
    view,
    total,
    onViewChange,
}: LedgerViewTabsProps) {
    return (
        <ListWorkspaceViews
            ariaLabel="库存台账工作视图"
            items={LEDGER_VIEWS.map((item) => ({
                id: `inventory-ledger-view-${item}`,
                label: VIEW_LABEL[item],
                count:
                    item === view ? total.toLocaleString("zh-CN") : undefined,
                active: item === view,
                onClick: () => onViewChange(item),
            }))}
        />
    )
}
