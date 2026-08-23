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

/** 列表行预览半屏：单栏摘要 + 打开详情 / 新建销售单 / 纸质预览。 */
export function ContractPreviewSheet({
    row,
    detail,
    detailLoading,
    onOpenChange,
    onShowPaper,
}: ContractPreviewSheetProps) {
    const canCreateSo =
        row?.allowedActions.includes("CREATE_SALES_ORDER") ?? false
    const soBlocker = row?.actionBlockers.find(
        (b) => b.action === "CREATE_SALES_ORDER",
    )
    const createSalesOrderHref = row
        ? `/sales/orders?mode=create&customerId=${encodeURIComponent(
              row.customer.customerId,
          )}&contractId=${encodeURIComponent(row.contractId)}`
        : null

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
                            variant="outline"
                            render={
                                <Link
                                    href={`/sales/contracts/${row.contractId}`}
                                />
                            }
                        >
                            查看详情
                        </Button>
                        {canCreateSo && createSalesOrderHref ? (
                            <Button
                                type="button"
                                render={<Link href={createSalesOrderHref} />}
                            >
                                新建销售单
                            </Button>
                        ) : (
                            <Button
                                type="button"
                                disabled
                                title={
                                    soBlocker?.message ?? "当前不可新建销售单"
                                }
                            >
                                新建销售单
                            </Button>
                        )}
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
