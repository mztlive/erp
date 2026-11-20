"use client"

import { PackageCheckIcon } from "lucide-react"

import {
    ListSelectionBar,
    useFrozenSelectionSnapshot,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"

export function ProductListSelectionBar({
    idPrefix = "master-data-products-list-selection",
    resultCount,
    pageRowCount,
    selectedCount,
    actionableCount,
    skippedCount,
    allPageSelected,
    busy,
    onSelectPage,
    onClear,
    onBatchList,
}: {
    idPrefix?: string
    resultCount: number
    pageRowCount: number
    selectedCount: number
    actionableCount: number
    skippedCount: number
    allPageSelected: boolean
    busy: boolean
    onSelectPage: () => void
    onClear: () => void
    onBatchList: () => void
}) {
    const counts = useFrozenSelectionSnapshot(selectedCount, {
        actionableCount,
        skippedCount,
    })

    return (
        <ListSelectionBar
            data-slot="product-list-selection-bar"
            idPrefix={idPrefix}
            unit="件"
            resultCount={resultCount}
            pageRowCount={pageRowCount}
            selectedCount={selectedCount}
            allPageSelected={allPageSelected}
            busy={busy}
            onSelectPage={onSelectPage}
            onClear={onClear}
            summaryExtra={(display) =>
                display.selectedCount > 0 ? (
                    <>
                        {"· 可上架 "}
                        <span
                            className={
                                counts.actionableCount > 0
                                    ? "num font-medium text-foreground"
                                    : "num"
                            }
                        >
                            {counts.actionableCount}
                        </span>
                        {" 件"}
                        {counts.skippedCount > 0 ? (
                            <span className="num text-muted-foreground/75">
                                （{counts.skippedCount} 件将跳过）
                            </span>
                        ) : null}
                    </>
                ) : null
            }
            actions={(display) => {
                const hasActionable = counts.actionableCount > 0
                const batchListDisabled =
                    busy || display.selectedCount === 0 || !hasActionable
                const batchListTitle =
                    display.selectedCount > 0 && !hasActionable
                        ? "已选商品均已上架或不满足上架条件"
                        : undefined
                return (
                    <Button
                        id={`${idPrefix}-batch-list`}
                        type="button"
                        variant={hasActionable ? "default" : "secondary"}
                        size="sm"
                        disabled={batchListDisabled}
                        title={batchListTitle}
                        onClick={() => {
                            if (selectedCount > 0 && actionableCount > 0) {
                                onBatchList()
                            }
                        }}
                    >
                        <PackageCheckIcon
                            data-icon="inline-start"
                            aria-hidden="true"
                        />
                        {hasActionable
                            ? `批量上架 ${counts.actionableCount}`
                            : "批量上架"}
                    </Button>
                )
            }}
        />
    )
}
