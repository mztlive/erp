"use client"

import * as React from "react"
import { XIcon, type LucideIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    LoadingButton,
    type LoadingButtonProps,
} from "@/components/ui/loading-button"
import { cn } from "@/lib/utils"

const ACTIVE_OVERLAY_SELECTOR = [
    '[role="dialog"]:not([data-slot="toast"])',
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

export type ListSelectionBarAction = {
    id: string
    label: React.ReactNode
    icon?: LucideIcon
    /** 当前可执行项数；为 0 时自动降级为中性 `variant="secondary"` 禁用态，避免主蓝半透明显脏。 */
    actionableCount?: number
    /** 有可执行项时是否在按钮末尾附带 `.num` 可执行计数，默认 true。 */
    showCount?: boolean
    disabled?: boolean
    loading?: boolean
    /** 已选非空但可执行数为 0 时的禁用原因提示（写入 `title`）。 */
    zeroActionableTitle?: string
    title?: string
    onClick: () => void
    className?: string
}

export type ListSelectionBarActionButtonProps = Omit<
    LoadingButtonProps,
    "children"
> & {
    label: React.ReactNode
    icon?: LucideIcon
    /** 当前可执行项数；为 0 时自动降级为 `variant="secondary"` 并禁用，杜绝淡蓝半透明显脏。 */
    actionableCount: number
    /** 是否展示尾部 `.num` 可执行计数，默认 true。 */
    showCount?: boolean
    /** 可执行数为 0 时的原因说明（写入 `title`）。 */
    zeroActionableTitle?: string
}

/**
 * 批量操作条标准主动作按钮：
 * - `actionableCount > 0`：实心飞书蓝 `variant="default"` + 可选尾部 `.num` 计数；
 * - `actionableCount === 0`：自动降级为中性 `variant="secondary"` 禁用态并展示原因提示。
 */
export function ListSelectionBarActionButton({
    label,
    icon: Icon,
    actionableCount,
    showCount = true,
    zeroActionableTitle,
    variant,
    size = "sm",
    disabled,
    title,
    type = "button",
    onClick,
    ...props
}: ListSelectionBarActionButtonProps) {
    const hasActionable = actionableCount > 0
    const resolvedVariant = variant ?? (hasActionable ? "default" : "secondary")
    const resolvedDisabled = Boolean(disabled || !hasActionable)
    const resolvedTitle = !hasActionable
        ? (zeroActionableTitle ?? title)
        : title

    return (
        <LoadingButton
            type={type}
            variant={resolvedVariant}
            size={size}
            disabled={resolvedDisabled}
            title={resolvedTitle}
            onClick={(event) => {
                if (!hasActionable) return
                onClick?.(event)
            }}
            {...props}
        >
            {Icon ? <Icon data-icon="inline-start" aria-hidden="true" /> : null}
            {label}
            {showCount && hasActionable ? (
                <span className="num">{actionableCount}</span>
            ) : null}
        </LoadingButton>
    )
}

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
    /** 结构化批量主动作；可执行数为 0 时自动降级为 `variant="secondary"` 禁用态并在退场期间冻结计数。 */
    primaryAction?: ListSelectionBarAction
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
 * - 统一提供实蓝计数徽标、附加分流摘要、全选本页、批量主动作插槽（支持零可执行自动降级 secondary）与清空关闭钮；
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
    primaryAction,
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
    const rawPrimaryActionableCount =
        primaryAction?.actionableCount ?? selectedCount
    const frozen = useFrozenSelectionSnapshot(selectedCount, {
        resultCount,
        selectedCount,
        primaryActionableCount: rawPrimaryActionableCount,
    })
    const display: ListSelectionBarDisplay = {
        resultCount: frozen.resultCount,
        selectedCount: frozen.selectedCount,
    }

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
                    className="h-control-sm px-2 text-body-compact font-normal text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                    disabled={busy || pageRowCount === 0 || allPageSelected}
                    onClick={onSelectPage}
                >
                    {selectPageLabel}
                </Button>
                {primaryAction ? (
                    <ListSelectionBarActionButton
                        id={primaryAction.id}
                        label={primaryAction.label}
                        icon={primaryAction.icon}
                        actionableCount={frozen.primaryActionableCount}
                        showCount={primaryAction.showCount}
                        loading={primaryAction.loading}
                        disabled={
                            busy ||
                            primaryAction.disabled ||
                            display.selectedCount === 0
                        }
                        zeroActionableTitle={
                            display.selectedCount > 0
                                ? primaryAction.zeroActionableTitle
                                : undefined
                        }
                        title={primaryAction.title}
                        className={primaryAction.className}
                        onClick={() => {
                            if (
                                selectedCount > 0 &&
                                rawPrimaryActionableCount > 0
                            ) {
                                primaryAction.onClick()
                            }
                        }}
                    />
                ) : null}
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
