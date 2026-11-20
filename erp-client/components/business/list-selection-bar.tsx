"use client"

import * as React from "react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

function isShallowEqual<T>(a: T, b: T): boolean {
    if (Object.is(a, b)) return true
    if (
        typeof a !== "object" ||
        a === null ||
        typeof b !== "object" ||
        b === null
    ) {
        return false
    }
    const keysA = Object.keys(a as Record<string, unknown>)
    const keysB = Object.keys(b as Record<string, unknown>)
    if (keysA.length !== keysB.length) return false
    for (const key of keysA) {
        if (
            !Object.prototype.hasOwnProperty.call(b, key) ||
            !Object.is(
                (a as Record<string, unknown>)[key],
                (b as Record<string, unknown>)[key],
            )
        ) {
            return false
        }
    }
    return true
}

/**
 * 取消最后一条勾选播放收起动画时冻结上一次非零快照，
 * 避免退场瞬间闪出「已选 0 条」或按钮置灰闪烁。
 */
export function useFrozenSelectionSnapshot<T>(
    selectedCount: number,
    value: T,
): T {
    const [snapshot, setSnapshot] = React.useState<T>(value)
    if (selectedCount > 0 && !isShallowEqual(snapshot, value)) {
        setSnapshot(value)
    }
    return selectedCount > 0 ? value : snapshot
}

export type ListSelectionBarDisplay = Readonly<{
    resultCount: number
    selectedCount: number
}>

export type ListSelectionBarProps = {
    idPrefix: string
    resultCount: number
    pageRowCount: number
    selectedCount: number
    allPageSelected: boolean
    busy?: boolean
    /** 计数单位，默认「条」；商品等场景可传「件」。 */
    unit?: string
    /** 附加在「已选 N / M 单位」之后的摘要（如可操作数 / 将跳过数）。 */
    summaryExtra?:
        | React.ReactNode
        | ((display: ListSelectionBarDisplay) => React.ReactNode)
    /** 右侧批量操作按钮（默认由调用方放置主蓝按钮）。 */
    actions?:
        | React.ReactNode
        | ((display: ListSelectionBarDisplay) => React.ReactNode)
    selectPageLabel?: string
    clearLabel?: string
    onSelectPage: () => void
    onClear: () => void
    className?: string
    "data-slot"?: string
}

/**
 * 列表工作面统一批量勾选操作条：
 * - 内置退场动画期间的计数快照冻结，避免闪出 0；
 * - 统一提供「已选 N / M 单位」、全选本页、清空已选与批量主动作插槽。
 */
export function ListSelectionBar({
    idPrefix,
    resultCount,
    pageRowCount,
    selectedCount,
    allPageSelected,
    busy = false,
    unit = "条",
    summaryExtra,
    actions,
    selectPageLabel = "全选本页",
    clearLabel = "清空已选",
    onSelectPage,
    onClear,
    className,
    "data-slot": dataSlot = "list-selection-bar",
}: ListSelectionBarProps) {
    const display = useFrozenSelectionSnapshot(selectedCount, {
        resultCount,
        selectedCount,
    })

    const resolvedExtra =
        typeof summaryExtra === "function"
            ? summaryExtra(display)
            : summaryExtra
    const resolvedActions =
        typeof actions === "function" ? actions(display) : actions

    return (
        <div
            data-slot={dataSlot}
            className={cn(
                "flex flex-wrap items-center gap-x-2.5 gap-y-2 text-body-compact",
                className,
            )}
        >
            <span className="text-muted-foreground">
                已选{" "}
                <span className="num font-medium text-foreground">
                    {display.selectedCount}
                </span>{" "}
                / <span className="num">{display.resultCount}</span> {unit}
                {resolvedExtra}
            </span>
            <Button
                id={`${idPrefix}-select-page`}
                type="button"
                variant="ghost"
                size="sm"
                disabled={busy || pageRowCount === 0 || allPageSelected}
                onClick={onSelectPage}
            >
                {selectPageLabel}
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
                {clearLabel}
            </Button>
            {resolvedActions}
        </div>
    )
}
