"use client"

import { ChevronDownIcon } from "lucide-react"

import { detailWorkspaceStyles } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuGroup,
    DropdownMenuItem,
    DropdownMenuLabel,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import type { SalesProcurementPriceBasis } from "../types"

export function SalesOrderCreatePriceBasisBatchMenu({
    lineCount,
    onApply,
}: {
    lineCount: number
    onApply: (basis: SalesProcurementPriceBasis) => void
}) {
    return (
        <DropdownMenu>
            <DropdownMenuTrigger
                id="sales-orders-create-batch-price-basis-open"
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
                批量设置取价方式
                <ChevronDownIcon
                    data-icon="inline-end"
                    className="size-3.5"
                    aria-hidden="true"
                />
            </DropdownMenuTrigger>
            <DropdownMenuContent
                align="end"
                className="w-72 max-w-[calc(100vw-2rem)]"
            >
                <DropdownMenuGroup>
                    <DropdownMenuLabel>
                        应用到全部 {lineCount}{" "}
                        条明细，覆盖已填写的供给单价；缺价需补填。
                    </DropdownMenuLabel>
                    <DropdownMenuItem
                        id="sales-orders-create-batch-price-basis-bulk"
                        onClick={() => onApply("BULK")}
                    >
                        全部设为集采
                    </DropdownMenuItem>
                    <DropdownMenuItem
                        id="sales-orders-create-batch-price-basis-dropship"
                        onClick={() => onApply("DROPSHIP")}
                    >
                        全部设为一件代发
                    </DropdownMenuItem>
                </DropdownMenuGroup>
            </DropdownMenuContent>
        </DropdownMenu>
    )
}
