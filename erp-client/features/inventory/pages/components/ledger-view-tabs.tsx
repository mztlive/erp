"use client"

import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { ListWorkspaceViews } from "@/components/business/list-workspace"
import { VIEW_LABEL } from "@/features/inventory/types"
import type { InventoryView } from "@/features/inventory/types"

interface LedgerViewTabsProps {
    view: InventoryView
    onViewChange: (nextView: InventoryView) => void
}

const LEDGER_VIEWS = [
    "balance",
    "movement",
    "reservation",
    "adjustment",
] as const

export function LedgerViewTabs({ view, onViewChange }: LedgerViewTabsProps) {
    const profile = useAccountProfileQuery()
    const visibleViews = LEDGER_VIEWS.filter((item) =>
        hasPermission(profile.data?.permissions, `stock_${item}:list`),
    )
    if (!visibleViews.length) return null
    return (
        <ListWorkspaceViews
            ariaLabel="库存台账工作视图"
            variant="inline"
            items={visibleViews.map((item) => ({
                id: `inventory-ledger-view-${item}`,
                label: VIEW_LABEL[item],
                active: item === view,
                onClick: () => onViewChange(item),
            }))}
        />
    )
}
