"use client"

import * as React from "react"
import { SlidersHorizontalIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Combobox,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
} from "@/components/ui/combobox"
import {
    remoteSearchFromInputChange,
    useStickySelected,
} from "@/components/business/combobox-input-search"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    Sheet,
    SheetContent,
    SheetDescription,
    SheetFooter,
    SheetHeader,
    SheetTitle,
} from "@/components/ui/sheet"
import { Spinner } from "@/components/ui/spinner"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

type BusinessObjectOption = {
    id: string
    code: string
    label: string
    status: {
        label: string
        tone: StatusTone
    }
    validUntil?: string
    description?: string
}

interface BusinessObjectComboboxProps {
    items: readonly BusinessObjectOption[]
    value?: string
    onValueChange: (id?: string) => void
    onSearchChange?: (query: string) => void
    /** 服务端已完成搜索时关闭本地二次过滤。 */
    filterMode?: "local" | "remote"
    label: string
    placeholder?: string
    emptyLabel?: string
    loading?: boolean
    disabled?: boolean
    required?: boolean
    id?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
    className?: string
}

function BusinessObjectCombobox({
    items,
    value,
    onValueChange,
    onSearchChange,
    filterMode = "local",
    label,
    placeholder = "搜索名称或编号",
    emptyLabel = "没有符合条件的对象",
    loading = false,
    disabled = false,
    required = false,
    id,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedBy,
    className,
}: BusinessObjectComboboxProps) {
    const selected = useStickySelected(items, value, (item) => item.id)

    return (
        <Combobox
            items={items}
            value={selected}
            onValueChange={(next) => onValueChange(next?.id)}
            onInputValueChange={(query, details) => {
                const nextQuery = remoteSearchFromInputChange(
                    query,
                    details.reason,
                )
                if (nextQuery !== undefined) onSearchChange?.(nextQuery)
            }}
            itemToStringLabel={(item) => item.label}
            itemToStringValue={(item) => item.id}
            isItemEqualToValue={(item, current) => item.id === current.id}
            filter={(item, query) => {
                if (filterMode === "remote") return true
                const q = query.trim().toLowerCase()
                if (!q) return true
                const haystack = [
                    item.label,
                    item.code,
                    item.description,
                    item.validUntil,
                    item.status.label,
                ]
                    .filter(Boolean)
                    .join(" ")
                    .toLowerCase()
                return haystack.includes(q)
            }}
            disabled={disabled}
            required={required}
        >
            <div
                data-slot="business-object-combobox"
                className={cn("min-w-0", className)}
            >
                <ComboboxInput
                    id={id}
                    triggerId={id ? `${id}-trigger` : undefined}
                    clearId={id ? `${id}-clear` : undefined}
                    aria-label={label}
                    aria-invalid={ariaInvalid || undefined}
                    aria-describedby={ariaDescribedBy}
                    aria-busy={loading}
                    placeholder={placeholder}
                    showClear
                    disabled={disabled}
                    className="w-full"
                />
                <ComboboxContent>
                    <ComboboxEmpty>
                        {loading ? "正在加载…" : emptyLabel}
                    </ComboboxEmpty>
                    <ComboboxList>
                        {items.map((item) => (
                            <ComboboxItem
                                key={item.id}
                                id={
                                    id
                                        ? `${id}-option-${toAutomationIdSegment(item.id)}`
                                        : undefined
                                }
                                value={item}
                            >
                                <div className="min-w-0 flex-1">
                                    <div className="flex min-w-0 items-center gap-2">
                                        <span className="truncate font-medium">
                                            {item.label}
                                        </span>
                                        <StatusBadge
                                            tone={item.status.tone}
                                            label={item.status.label}
                                        />
                                    </div>
                                    <div className="mt-1 flex flex-wrap gap-x-3 gap-y-1 text-xs text-muted-foreground">
                                        <span className="num">{item.code}</span>
                                        {item.validUntil ? (
                                            <span className="num">
                                                有效至 {item.validUntil}
                                            </span>
                                        ) : null}
                                        {item.description ? (
                                            <span>{item.description}</span>
                                        ) : null}
                                    </div>
                                </div>
                            </ComboboxItem>
                        ))}
                    </ComboboxList>
                </ComboboxContent>
            </div>
        </Combobox>
    )
}

type SavedView = {
    id: string
    label: string
    scope: "personal" | "team"
    readOnly?: boolean
}

interface SavedViewPickerProps {
    views: readonly SavedView[]
    value?: string
    onValueChange: (id?: string) => void
    placeholder?: string
    disabled?: boolean
    id?: string
    actions?: React.ReactNode
    className?: string
}

function SavedViewPicker({
    views,
    value,
    onValueChange,
    placeholder = "选择保存视图",
    disabled,
    id,
    actions,
    className,
}: SavedViewPickerProps) {
    const options = React.useMemo(
        () =>
            views.map((view) => ({
                value: view.id,
                label: `${view.label}${view.scope === "team" ? " · 团队" : " · 个人"}${
                    view.readOnly ? " · 只读" : ""
                }`,
                keywords: view.label,
            })),
        [views],
    )

    return (
        <div
            data-slot="saved-view-picker"
            className={cn("flex items-center gap-2", className)}
        >
            <OptionCombobox
                id={id}
                options={options}
                value={value ?? null}
                onValueChange={(next) => onValueChange(next ?? undefined)}
                placeholder={placeholder}
                disabled={disabled}
                allowClear
                aria-label={placeholder}
                className="min-w-[12rem]"
            />
            {actions}
        </div>
    )
}

interface AdvancedFilterSheetProps {
    open: boolean
    onOpenChange: (open: boolean) => void
    idPrefix?: string
    title?: React.ReactNode
    description?: React.ReactNode
    summary?: React.ReactNode
    children: React.ReactNode
    onReset: () => void
    onApply: () => void
    applying?: boolean
}

function AdvancedFilterSheet({
    open,
    onOpenChange,
    idPrefix,
    title = "高级筛选",
    description,
    summary,
    children,
    onReset,
    onApply,
    applying = false,
}: AdvancedFilterSheetProps) {
    return (
        <Sheet open={open} onOpenChange={onOpenChange}>
            <SheetContent
                side="right"
                size="preview"
                closeButtonId={idPrefix ? `${idPrefix}-close` : undefined}
            >
                <SheetHeader className="border-b">
                    <SheetTitle className="flex items-center gap-2">
                        <SlidersHorizontalIcon
                            className="size-4"
                            aria-hidden="true"
                        />
                        {title}
                    </SheetTitle>
                    {description ? (
                        <SheetDescription>{description}</SheetDescription>
                    ) : null}
                    {summary ? (
                        <div className="pt-2 text-sm text-muted-foreground">
                            {summary}
                        </div>
                    ) : null}
                </SheetHeader>
                <div className="min-h-0 flex-1 space-y-5 overflow-y-auto p-6">
                    {children}
                </div>
                <SheetFooter className="border-t">
                    <Button
                        id={idPrefix ? `${idPrefix}-reset` : undefined}
                        type="button"
                        variant="outline"
                        onClick={onReset}
                    >
                        重置
                    </Button>
                    <Button
                        id={idPrefix ? `${idPrefix}-apply` : undefined}
                        type="button"
                        onClick={onApply}
                        disabled={applying}
                    >
                        {applying ? <Spinner /> : null}
                        应用筛选
                    </Button>
                </SheetFooter>
            </SheetContent>
        </Sheet>
    )
}

export {
    AdvancedFilterSheet,
    BusinessObjectCombobox,
    SavedViewPicker,
    type AdvancedFilterSheetProps,
    type BusinessObjectComboboxProps,
    type BusinessObjectOption,
    type SavedView,
    type SavedViewPickerProps,
}
