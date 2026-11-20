"use client"

import * as React from "react"
import { PackageCheckIcon } from "lucide-react"

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
    // 取消最后一条勾选播放收起动画时冻结上一次非零计数，避免退场瞬间闪出「已选 0 件」或按钮置灰闪烁。
    const [snapshot, setSnapshot] = React.useState({
        resultCount,
        selectedCount,
        actionableCount,
        skippedCount,
    })
    if (
        selectedCount > 0 &&
        (snapshot.resultCount !== resultCount ||
            snapshot.selectedCount !== selectedCount ||
            snapshot.actionableCount !== actionableCount ||
            snapshot.skippedCount !== skippedCount)
    ) {
        setSnapshot({
            resultCount,
            selectedCount,
            actionableCount,
            skippedCount,
        })
    }
    const display =
        selectedCount > 0
            ? { resultCount, selectedCount, actionableCount, skippedCount }
            : snapshot

    const batchListDisabled =
        busy || display.selectedCount === 0 || display.actionableCount === 0
    const batchListTitle =
        display.selectedCount > 0 && display.actionableCount === 0
            ? "已选商品均已上架或不满足上架条件"
            : undefined

    return (
        <div
            data-slot="product-list-selection-bar"
            className="flex flex-wrap items-center gap-x-2.5 gap-y-2 text-body-compact"
        >
            <span className="text-muted-foreground">
                已选{" "}
                <span className="num font-medium text-foreground">
                    {display.selectedCount}
                </span>{" "}
                / <span className="num">{display.resultCount}</span> 件
                {display.selectedCount > 0 ? (
                    <>
                        {" · 可上架 "}
                        <span className="num font-medium text-foreground">
                            {display.actionableCount}
                        </span>
                        {" 件"}
                        {display.skippedCount > 0 ? (
                            <span className="num">
                                （{display.skippedCount} 件将跳过）
                            </span>
                        ) : null}
                    </>
                ) : null}
            </span>
            <Button
                id={`${idPrefix}-select-page`}
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy || pageRowCount === 0 || allPageSelected}
                onClick={onSelectPage}
            >
                全选本页
            </Button>
            <Button
                id={`${idPrefix}-clear`}
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy || display.selectedCount === 0}
                onClick={() => {
                    if (selectedCount > 0) onClear()
                }}
            >
                清空已选
            </Button>
            <Button
                id={`${idPrefix}-batch-list`}
                type="button"
                variant="default"
                size="sm"
                disabled={batchListDisabled}
                title={batchListTitle}
                onClick={() => {
                    if (selectedCount > 0 && actionableCount > 0) onBatchList()
                }}
            >
                <PackageCheckIcon data-icon="inline-start" aria-hidden="true" />
                {display.actionableCount > 0
                    ? `批量上架 ${display.actionableCount}`
                    : "批量上架"}
            </Button>
        </div>
    )
}
