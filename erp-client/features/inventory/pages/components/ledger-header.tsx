"use client"

import { DownloadIcon, InfoIcon, RefreshCwIcon } from "lucide-react"

import { PageActions } from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceQuietAction,
    listWorkspaceStyles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    Popover,
    PopoverContent,
    PopoverTitle,
    PopoverTrigger,
} from "@/components/ui/popover"
import type { InventoryView } from "@/features/inventory/types"
import { formatDateTime } from "@/lib/datetime"
import { LedgerViewTabs } from "./ledger-view-tabs"

interface LedgerHeaderProps {
    view: InventoryView
    onViewChange: (view: InventoryView) => void
    queriedAt?: string
    excludedKindsNote?: string
    openingStockNote?: string
    canExport: boolean
    total: number
    isExporting: boolean
    listReady: boolean
    denied?: boolean
    onRefresh: () => void
    onExport: () => void
}

export function LedgerHeader({
    view,
    onViewChange,
    queriedAt,
    excludedKindsNote,
    openingStockNote,
    canExport,
    total,
    isExporting,
    listReady,
    denied = false,
    onRefresh,
    onExport,
}: LedgerHeaderProps) {
    return (
        <ListWorkspaceHeader
            eyebrow=""
            title="库存台账"
            views={<LedgerViewTabs view={view} onViewChange={onViewChange} />}
        >
            {queriedAt || excludedKindsNote || openingStockNote ? (
                <Popover>
                    <PopoverTrigger
                        id="inventory-ledger-scope-description"
                        title="库存范围说明"
                        aria-label="库存范围说明"
                        render={
                            <Button
                                type="button"
                                variant="ghost"
                                size="icon-sm"
                                className={listWorkspaceStyles.quietButton}
                            />
                        }
                    >
                        <InfoIcon aria-hidden="true" />
                    </PopoverTrigger>
                    <PopoverContent
                        align="end"
                        className="max-w-[calc(100vw-2rem)]"
                    >
                        <PopoverTitle>库存范围说明</PopoverTitle>
                        {excludedKindsNote ? <p>{excludedKindsNote}</p> : null}
                        {openingStockNote ? <p>{openingStockNote}</p> : null}
                        {queriedAt ? (
                            <time
                                dateTime={queriedAt}
                                className="num text-xs text-muted-foreground"
                            >
                                更新于{" "}
                                {formatDateTime(
                                    queriedAt,
                                    "full",
                                    "passthrough",
                                )}
                            </time>
                        ) : null}
                    </PopoverContent>
                </Popover>
            ) : null}
            <PageActions
                size="sm"
                className="gap-1.5"
                actions={[
                    listWorkspaceQuietAction({
                        actionKey: "refresh",
                        id: denied
                            ? "inventory-ledger-permission-retry"
                            : "inventory-ledger-refresh",
                        label: denied ? "重新检查权限" : "刷新",
                        icon: RefreshCwIcon,
                        onClick: onRefresh,
                    }),
                    listWorkspaceQuietAction({
                        hidden: !canExport,
                        actionKey: "export",
                        id: "inventory-ledger-export",
                        label: isExporting ? "导出中…" : "导出",
                        icon: DownloadIcon,
                        loading: isExporting,
                        disabled: isExporting || !listReady || total === 0,
                        onClick: onExport,
                    }),
                ]}
            />
        </ListWorkspaceHeader>
    )
}
