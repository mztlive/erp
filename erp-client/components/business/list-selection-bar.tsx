"use client"

import * as React from "react"
import { XIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { cn } from "@/lib/utils"

const ACTIVE_OVERLAY_SELECTOR = [
    '[role="dialog"]',
    '[role="alertdialog"]',
    '[role="menu"]',
    '[role="listbox"]',
    '[data-slot="sheet"]',
    '[data-slot="popover-content"][data-open]',
    '[data-slot="list-filter-tray"][data-open]',
].join(", ")

function isEditableTarget(target: EventTarget | null): boolean {
    if (!(target instanceof HTMLElement)) return false
    return (
        target.tagName === "INPUT" ||
        target.tagName === "TEXTAREA" ||
        target.tagName === "SELECT" ||
        target.isContentEditable
    )
}

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
    /** 读屏区域名称，默认「批量操作」。 */
    ariaLabel?: string
    /** 无浮层打开且焦点不在输入框时，允许按 Esc 快速清空已选。默认开启。 */
    enableEscapeClear?: boolean
    onSelectPage: () => void
    onClear: () => void
    className?: string
    "data-slot"?: string
}

/**
 * 列表工作面统一批量勾选操作条（配合 ListWorkSurface 底部悬浮胶囊使用）：
 * - 内置退场动画期间的计数快照冻结，避免闪出 0；
 * - 统一提供实蓝计数徽标、附加分流摘要、全选本页、批量主动作插槽与清空关闭钮；
 * - 无浮层打开时支持按 Esc 快速清空已选。
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
    ariaLabel = "批量操作",
    enableEscapeClear = true,
    onSelectPage,
    onClear,
    className,
    "data-slot": dataSlot = "list-selection-bar",
}: ListSelectionBarProps) {
    const display = useFrozenSelectionSnapshot(selectedCount, {
        resultCount,
        selectedCount,
    })

    React.useEffect(() => {
        if (!enableEscapeClear || selectedCount === 0 || busy) return
        const onKeyDown = (event: KeyboardEvent) => {
            if (
                event.key !== "Escape" ||
                event.defaultPrevented ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            if (isEditableTarget(event.target)) return
            if (document.querySelector(ACTIVE_OVERLAY_SELECTOR)) return
            event.preventDefault()
            onClear()
        }
        window.addEventListener("keydown", onKeyDown)
        return () => window.removeEventListener("keydown", onKeyDown)
    }, [busy, enableEscapeClear, onClear, selectedCount])

    const resolvedExtra =
        typeof summaryExtra === "function"
            ? summaryExtra(display)
            : summaryExtra
    const resolvedActions =
        typeof actions === "function" ? actions(display) : actions

    return (
        <div
            role="region"
            aria-label={ariaLabel}
            data-slot={dataSlot}
            className={cn(
                "flex max-w-full flex-wrap items-center gap-x-2 gap-y-1.5 text-body-compact",
                className,
            )}
        >
            <div className="flex min-w-0 flex-wrap items-center gap-x-1.5 gap-y-1 pl-1">
                <span className="inline-flex items-center gap-1.5 text-foreground/85">
                    <span className="font-medium">已选</span>
                    <span className="num inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-primary px-1.5 text-xs leading-none font-semibold text-primary-foreground">
                        {display.selectedCount}
                    </span>
                    <span className="num text-muted-foreground">
                        / {display.resultCount} {unit}
                    </span>
                </span>
                {resolvedExtra ? (
                    <span className="text-xs text-muted-foreground">
                        {resolvedExtra}
                    </span>
                ) : null}
            </div>
            <span
                aria-hidden="true"
                className="hidden h-4 w-px shrink-0 bg-border/85 sm:inline-block"
            />
            <div className="flex flex-wrap items-center gap-1.5">
                <Button
                    id={`${idPrefix}-select-page`}
                    type="button"
                    variant="ghost"
                    size="sm"
                    className="h-control-sm px-2 text-xs font-normal text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                    disabled={busy || pageRowCount === 0 || allPageSelected}
                    onClick={onSelectPage}
                >
                    {selectPageLabel}
                </Button>
                {resolvedActions}
                <span
                    aria-hidden="true"
                    className="mx-0.5 h-4 w-px shrink-0 bg-border/85"
                />
                <Button
                    id={`${idPrefix}-clear`}
                    type="button"
                    variant="ghost"
                    size="icon-sm"
                    className="size-control-sm text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                    aria-label={clearLabel}
                    title={
                        enableEscapeClear ? `${clearLabel}（Esc）` : clearLabel
                    }
                    disabled={busy || display.selectedCount === 0}
                    onClick={() => {
                        if (selectedCount > 0) onClear()
                    }}
                >
                    <XIcon aria-hidden="true" />
                </Button>
            </div>
        </div>
    )
}
