"use client"

import { useEffect, type ReactNode } from "react"
import { XIcon } from "lucide-react"

import { useFrozenSelectionSnapshot } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import { Checkbox } from "@/components/ui/checkbox"
import { masterDataCopy } from "@/features/master-data/lib/copy"

export function SellableSelectAll({
    idPrefix,
    label,
    disabled,
    allSelected,
    someSelected,
    onSelectAll,
    onDeselectAll,
}: {
    idPrefix: string
    label: string
    disabled: boolean
    allSelected: boolean
    someSelected: boolean
    onSelectAll: () => void
    onDeselectAll: () => void
}) {
    return (
        <Checkbox
            id={`${idPrefix}-select-all`}
            checked={allSelected}
            indeterminate={someSelected}
            disabled={disabled}
            aria-label={label}
            title={label}
            onCheckedChange={(checked) => {
                if (checked === true) onSelectAll()
                else onDeselectAll()
            }}
        />
    )
}

export function SellableGallerySelectionBar({
    idPrefix,
    selectAllLabel,
    leadingControl,
    actions,
    resultCount,
    selectedCount,
    allSelected,
    canSelectRows,
    busy,
    onSelectAll,
    onClear,
}: {
    idPrefix: string
    selectAllLabel: string
    leadingControl?: ReactNode
    actions?: ReactNode
    resultCount: number
    selectedCount: number
    allSelected: boolean
    canSelectRows: boolean
    busy: boolean
    onSelectAll: () => void
    onClear: () => void
}) {
    const display = useFrozenSelectionSnapshot(selectedCount, {
        resultCount,
        selectedCount,
    })

    useEffect(() => {
        if (!selectedCount || busy) return
        const handleEscape = (event: KeyboardEvent) => {
            if (
                event.key !== "Escape" ||
                event.defaultPrevented ||
                event.isComposing ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            )
                return
            if (
                event.target instanceof HTMLElement &&
                (event.target.isContentEditable ||
                    event.target.closest("input, textarea, select"))
            )
                return
            if (
                document.querySelector(
                    '[role="dialog"], [role="alertdialog"], [role="menu"], [role="listbox"], [data-slot="list-filter-tray"][data-open], [data-slot="popover-content"][data-open]',
                )
            )
                return
            event.preventDefault()
            onClear()
        }
        window.addEventListener("keydown", handleEscape)
        return () => window.removeEventListener("keydown", handleEscape)
    }, [busy, onClear, selectedCount])

    return (
        <div
            role="region"
            aria-label="批量操作"
            className="flex max-w-full flex-wrap items-center gap-2 text-body-compact"
            data-slot="sellable-gallery-selection-bar"
        >
            {leadingControl}
            <span className="inline-flex items-center gap-1.5 pl-1 text-foreground/85">
                <span className="font-medium">已选</span>
                <span className="num inline-flex h-5 min-w-5 items-center justify-center rounded-full bg-primary px-1.5 text-xs leading-none font-semibold text-primary-foreground">
                    {display.selectedCount}
                </span>
                <span className="num text-muted-foreground">
                    / {display.resultCount} 件
                </span>
            </span>
            <span
                aria-hidden="true"
                className="hidden h-4 w-px bg-border/85 sm:inline-block"
            />
            <Button
                id={`${idPrefix}-select-all-action`}
                type="button"
                variant="ghost"
                size="sm"
                className="font-normal text-muted-foreground"
                disabled={busy || !canSelectRows || allSelected}
                onClick={onSelectAll}
            >
                {selectAllLabel}
            </Button>
            {actions}
            <Button
                id={`${idPrefix}-clear-selection`}
                type="button"
                variant="ghost"
                size="icon-sm"
                className="text-muted-foreground"
                disabled={busy || display.selectedCount === 0}
                aria-label={masterDataCopy.sellableClearSelection}
                title={`${masterDataCopy.sellableClearSelection}（Esc）`}
                onClick={onClear}
            >
                <XIcon aria-hidden="true" />
            </Button>
        </div>
    )
}
