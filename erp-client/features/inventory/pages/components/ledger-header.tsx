"use client"

import { DownloadIcon, LoaderCircleIcon, RefreshCwIcon } from "lucide-react"

import { PageActions } from "@/components/business"
import { ListWorkspaceHeader } from "@/components/business/list-workspace"
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
        <ListWorkspaceHeader
            eyebrow="库存"
            title="库存台账"
            description={
                <>
                    查看账面现存、预占与可用数量。
                    <span className="ml-3 text-xs" role="status">
                        {queriedAt ? (
                            <time dateTime={queriedAt}>
                                更新于{" "}
                                {formatDateTime(
                                    queriedAt,
                                    "full",
                                    "passthrough",
                                )}
                            </time>
                        ) : (
                            "正在查询"
                        )}
                    </span>
                    {isPhoneNarrow ? (
                        <span className="block text-xs">
                            移动端只读：可查看余额与流水。库存调整、列设置与全量导出请在桌面完成。
                        </span>
                    ) : null}
                </>
            }
        >
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
        </ListWorkspaceHeader>
    )
}
