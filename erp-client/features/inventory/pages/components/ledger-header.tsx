"use client"

import { DownloadIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react"

import { DataFreshness, PageActions, PageHeader } from "@/components/business"
import { formatDateTime } from "@/lib/datetime"

interface LedgerHeaderProps {
    isPhoneNarrow: boolean
    queriedAt: string
    canExport: boolean
    total: number
    isExporting?: boolean
    onRefresh: () => void
    onExport: () => void
}

export function LedgerHeader({
    isPhoneNarrow,
    queriedAt,
    canExport,
    total,
    isExporting = false,
    onRefresh,
    onExport,
}: LedgerHeaderProps) {
    return (
        <PageHeader
            title="库存台账"
            description={
                isPhoneNarrow
                    ? "移动端只读：可查看余额与流水。库存调整、列设置与全量导出请在桌面完成。"
                    : "按仓库与 SKU 查看账面现存、有效预占与可用数量；追溯流水与销售预占。不可直接编辑库存或释放预占。"
            }
            metadata={
                <DataFreshness
                    updatedAt={formatDateTime(queriedAt, "full", "passthrough")}
                    dateTime={queriedAt}
                    state="fresh"
                    label="库存记录更新时间"
                />
            }
            actions={
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
            }
        />
    )
}
