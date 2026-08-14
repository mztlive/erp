"use client"

import Link from "next/link"
import { PrinterIcon } from "lucide-react"

import {
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { ContractPreviewPanel } from "@/features/contracts/components/contract-preview-panel"
import type {
    ContractCenterView,
    ContractListRow,
} from "@/features/contracts/types"

type ContractPreviewSheetProps = {
    row: ContractListRow | null
    detail: ContractCenterView | null | undefined
    detailLoading: boolean
    onOpenChange: (open: boolean) => void
    onShowPaper: (contractId: string) => void
}

/** 列表行预览半屏：身份摘要 + 详情面板 + 打开/打印动作。 */
export function ContractPreviewSheet({
    row,
    detail,
    detailLoading,
    onOpenChange,
    onShowPaper,
}: ContractPreviewSheetProps) {
    return (
        <QuickPreviewSheet
            open={row != null}
            onOpenChange={onOpenChange}
            size="detail"
            title={row?.customer.displayName ?? "合同预览"}
            identity={
                row ? (
                    <span className="num">
                        {row.contractNo} · v{row.revisionNo}
                    </span>
                ) : null
            }
            summary={
                row ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <BusinessStatusBadge
                            context="preview"
                            label={row.statusLabel}
                            tone={row.statusTone}
                        />
                        {row.expiringWithin30Days ? (
                            <Badge variant="warning">将到期</Badge>
                        ) : null}
                        <span className="text-xs text-muted-foreground">
                            关联销售 {row.salesOrderCount} 张
                        </span>
                    </div>
                ) : null
            }
            footer={
                row ? (
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            onClick={() => onOpenChange(false)}
                        >
                            关闭
                        </Button>
                        <Button
                            type="button"
                            variant="outline"
                            disabled={!row.allowedActions.includes("PRINT")}
                            title={
                                row.actionBlockers.find(
                                    (b) => b.action === "PRINT",
                                )?.message
                            }
                            onClick={() => onShowPaper(row.contractId)}
                        >
                            <PrinterIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            纸质预览
                        </Button>
                        <Button
                            type="button"
                            render={
                                <Link
                                    href={`/sales/contracts/${row.contractId}`}
                                />
                            }
                        >
                            查看详情
                        </Button>
                    </>
                ) : null
            }
        >
            {row ? (
                <ContractPreviewPanel
                    row={row}
                    detail={detail}
                    detailLoading={detailLoading}
                />
            ) : null}
        </QuickPreviewSheet>
    )
}
