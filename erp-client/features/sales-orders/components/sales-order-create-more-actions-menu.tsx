"use client"

import {
    ChevronDownIcon,
    DownloadIcon,
    ListChecksIcon,
    UploadIcon,
} from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { SalesProcurementPriceBasis } from "../types"

export function SalesOrderCreateMoreActionsMenu({
    lineCount,
    uniqueAddressCount,
    hasWorkbookProduct,
    exporting,
    exportDisabled,
    onApplyPriceBasis,
    onOpenOverview,
    onExportWorkbook,
    onOpenImportWorkbook,
}: {
    lineCount: number
    uniqueAddressCount: number
    hasWorkbookProduct: boolean
    exporting: boolean
    exportDisabled: boolean
    onApplyPriceBasis: (basis: SalesProcurementPriceBasis) => void
    onOpenOverview: () => void
    onExportWorkbook: () => void
    onOpenImportWorkbook: () => void
}) {
    return (
        <DropdownMenu>
            <DropdownMenuTrigger
                id="sales-orders-create-lines-more-actions"
                render={
                    <Button
                        type="button"
                        variant="ghost"
                        size="sm"
                        className={detailWorkspaceStyles.subtleButton}
                    />
                }
                disabled={lineCount === 0}
            >
                更多
                <ChevronDownIcon
                    data-icon="inline-end"
                    className="size-3.5"
                    aria-hidden="true"
                />
            </DropdownMenuTrigger>
            <DropdownMenuContent
                align="end"
                className="w-60 max-w-[calc(100vw-2rem)]"
            >
                <DropdownMenuGroup>
                    <DropdownMenuLabel>
                        批量取价（全部 {lineCount} 项）
                    </DropdownMenuLabel>
                    <DropdownMenuItem
                        id="sales-orders-create-batch-price-basis-dropship"
                        onClick={() => onApplyPriceBasis("DROPSHIP")}
                    >
                        全部设为一件代发
                    </DropdownMenuItem>
                    <DropdownMenuItem
                        id="sales-orders-create-batch-price-basis-bulk"
                        onClick={() => onApplyPriceBasis("BULK")}
                    >
                        全部设为集采
                    </DropdownMenuItem>
                </DropdownMenuGroup>
                <DropdownMenuSeparator />
                <DropdownMenuGroup>
                    <DropdownMenuLabel>收货安排</DropdownMenuLabel>
                    <DropdownMenuItem
                        id="sales-delivery-overview-open"
                        onClick={onOpenOverview}
                    >
                        <ListChecksIcon
                            className="size-3.5 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <span>地址总览</span>
                        {uniqueAddressCount > 0 ? (
                            <span className="num ml-auto text-xs text-muted-foreground">
                                {uniqueAddressCount}
                            </span>
                        ) : null}
                    </DropdownMenuItem>
                    {hasWorkbookProduct ? (
                        <>
                            <DropdownMenuItem
                                id="sales-delivery-workbook-export"
                                disabled={exportDisabled}
                                onClick={onExportWorkbook}
                            >
                                <DownloadIcon
                                    className="size-3.5 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                <span>
                                    {exporting
                                        ? "正在导出…"
                                        : "导出收货安排 (.xlsx)"}
                                </span>
                            </DropdownMenuItem>
                            <DropdownMenuItem
                                id="sales-delivery-workbook-import"
                                onClick={onOpenImportWorkbook}
                            >
                                <UploadIcon
                                    className="size-3.5 text-muted-foreground"
                                    aria-hidden="true"
                                />
                                <span>批量导入地址</span>
                            </DropdownMenuItem>
                        </>
                    ) : null}
                </DropdownMenuGroup>
            </DropdownMenuContent>
        </DropdownMenu>
    )
}
