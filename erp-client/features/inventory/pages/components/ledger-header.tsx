"use client"

import {
    ChevronDownIcon,
    DownloadIcon,
    LoaderCircleIcon,
    RefreshCwIcon,
} from "lucide-react"

import { PageActions } from "@/components/business"
import { listWorkspaceStyles } from "@/components/business/list-workspace"
import { formatDateTime } from "@/lib/datetime"

interface LedgerHeaderProps {
    isPhoneNarrow: boolean
    queriedAt: string
    excludedKindsNote?: string
    openingStockNote?: string
    canExport: boolean
    total: number
    isExporting?: boolean
    onRefresh: () => void
    onExport: () => void
}

export function LedgerHeader({
    isPhoneNarrow,
    queriedAt,
    excludedKindsNote,
    openingStockNote,
    canExport,
    total,
    isExporting = false,
    onRefresh,
    onExport,
}: LedgerHeaderProps) {
    return (
        <header className="flex flex-col gap-3 pb-4 md:flex-row md:items-start md:justify-between md:gap-6">
            <div className="min-w-0">
                <p className="mb-1 text-xs text-muted-foreground">库存</p>
                <h1 className={listWorkspaceStyles.title}>库存台账</h1>
                <div className="mt-1 flex flex-wrap items-baseline gap-x-3 gap-y-1 text-[13px] leading-[22px] text-muted-foreground">
                    <p>查看账面现存、预占与可用数量。</p>
                    {excludedKindsNote || openingStockNote ? (
                        <details className="group min-w-0 open:basis-full">
                            <summary
                                id="inventory-ledger-scope-description"
                                className="inline-flex cursor-pointer list-none items-center gap-1 rounded-sm text-xs hover:text-foreground focus-visible:outline-2 focus-visible:outline-offset-2 focus-visible:outline-foreground [&::-webkit-details-marker]:hidden"
                            >
                                库存范围说明
                                <ChevronDownIcon
                                    aria-hidden="true"
                                    className="size-3 transition-transform group-open:rotate-180"
                                />
                            </summary>
                            <p className="mt-2 max-w-2xl border-l-2 border-border pl-3 text-xs leading-relaxed">
                                {excludedKindsNote}
                                <span className="mt-1 block">
                                    {openingStockNote}
                                </span>
                            </p>
                        </details>
                    ) : null}
                </div>
                {isPhoneNarrow ? (
                    <p className="mt-1 text-xs text-muted-foreground">
                        移动端只读：可查看余额与流水。库存调整、列设置与全量导出请在桌面完成。
                    </p>
                ) : null}
            </div>
            <div className="flex shrink-0 flex-wrap items-center gap-x-3 gap-y-2 md:pt-1">
                <span className="text-xs text-muted-foreground" role="status">
                    {queriedAt ? (
                        <time dateTime={queriedAt}>
                            更新于{" "}
                            {formatDateTime(queriedAt, "full", "passthrough")}
                        </time>
                    ) : (
                        "正在查询"
                    )}
                </span>
                <PageActions
                    actions={[
                        {
                            actionKey: "refresh",
                            id: "inventory-ledger-refresh",
                            label: "刷新",
                            icon: RefreshCwIcon,
                            variant: "ghost",
                            onClick: onRefresh,
                        },
                        {
                            actionKey: "export",
                            id: "inventory-ledger-export",
                            label: isExporting ? "导出中…" : "导出",
                            icon: isExporting ? LoaderCircleIcon : DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled:
                                isExporting ||
                                !canExport ||
                                total === 0 ||
                                isPhoneNarrow,
                            onClick: onExport,
                        },
                    ]}
                />
            </div>
        </header>
    )
}
