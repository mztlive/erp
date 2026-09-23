"use client"

import * as React from "react"

import {
    Combobox,
    ComboboxChip,
    ComboboxChips,
    ComboboxChipsInput,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxItem,
    ComboboxInput,
    ComboboxList,
    ComboboxTrigger,
    ComboboxValue,
    useComboboxAnchor,
} from "@/components/ui/combobox"
import type { ComboboxOption } from "@/components/business/option-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { Button } from "@/components/ui/button"

type InternalOption = ComboboxOption & { __search: string }

function toInternal(options: readonly ComboboxOption[]): InternalOption[] {
    return options.map((option) => ({
        ...option,
        __search: [option.label, option.value, option.keywords]
            .filter(Boolean)
            .join(" "),
    }))
}

export type MultiOptionComboboxProps = {
    options: readonly ComboboxOption[]
    /** 已选值；多选用逗号分隔值数组承载。 */
    value: readonly string[]
    onValueChange: (value: string[]) => void
    placeholder?: string
    /** 查询栏使用固定高度的名称与选择摘要，避免多选标签撑高控件。 */
    filterLabel?: string
    emptyLabel?: string
    disabled?: boolean
    id?: string
    "aria-label"?: string
    "aria-describedby"?: string
    className?: string
    size?: "sm" | "default"
}

/**
 * 基于 `components/ui/combobox` 的可搜索多选（chips 展示）。
 * 与 `OptionCombobox` 同源、同尺寸样式，用于筛选条的枚举多选。
 */
export function MultiOptionCombobox({
    options,
    value,
    onValueChange,
    placeholder = "请选择",
    filterLabel,
    emptyLabel = "没有符合条件的选项",
    disabled = false,
    id,
    "aria-label": ariaLabel,
    "aria-describedby": ariaDescribedBy,
    className,
    size = "default",
}: MultiOptionComboboxProps) {
    const items = React.useMemo(() => toInternal(options), [options])
    const selected = React.useMemo(
        () => items.filter((item) => value.includes(item.value)),
        [items, value],
    )
    const anchorRef = useComboboxAnchor()

    return (
        <Combobox
            items={items}
            multiple
            value={selected}
            onValueChange={(next) => {
                onValueChange(next.map((item) => item.value))
            }}
            itemToStringLabel={(item) => item.label}
            itemToStringValue={(item) => item.value}
            isItemEqualToValue={(item, current) => item.value === current.value}
            filter={(item, query) => {
                const q = query.trim().toLowerCase()
                if (!q) return true
                return item.__search.toLowerCase().includes(q)
            }}
            disabled={disabled}
        >
            <div
                ref={anchorRef}
                data-slot="multi-option-combobox"
                data-size={size}
                className={cn("min-w-0", className)}
            >
                {filterLabel ? (
                    <ComboboxTrigger
                        id={id}
                        aria-label={ariaLabel}
                        aria-describedby={ariaDescribedBy}
                        disabled={disabled}
                        render={
                            <Button
                                type="button"
                                variant="outline"
                                className="h-control w-full min-w-0 justify-start gap-1 rounded-lg bg-surface-control px-2.5 font-normal shadow-none"
                            />
                        }
                    >
                        <span className="shrink-0 text-muted-foreground">
                            {filterLabel}：
                        </span>
                        <span className="min-w-0 flex-1 truncate text-left">
                            {selected.length === 0
                                ? "全部"
                                : selected.length === 1
                                  ? selected[0].label
                                  : `已选 ${selected.length} 项`}
                        </span>
                    </ComboboxTrigger>
                ) : (
                    <ComboboxChips
                        className={cn(
                            size === "sm" &&
                                "min-h-7 py-0.5 *:data-[slot=combobox-chip]:h-5",
                        )}
                    >
                        <ComboboxValue>
                            {(valueItems: InternalOption[]) =>
                                valueItems.map((item) => (
                                    <ComboboxChip
                                        key={item.value}
                                        removeId={
                                            id
                                                ? `${id}-chip-${toAutomationIdSegment(item.value)}-remove`
                                                : undefined
                                        }
                                        aria-label={item.label}
                                    >
                                        <span className="min-w-0 truncate">
                                            {item.label}
                                        </span>
                                    </ComboboxChip>
                                ))
                            }
                        </ComboboxValue>
                        <ComboboxChipsInput
                            id={id}
                            aria-label={ariaLabel}
                            aria-describedby={ariaDescribedBy}
                            placeholder={selected.length > 0 ? "" : placeholder}
                            disabled={disabled}
                            className={cn(size === "sm" && "text-xs")}
                        />
                    </ComboboxChips>
                )}
            </div>
            <ComboboxContent anchor={anchorRef}>
                {filterLabel && (
                    <ComboboxInput
                        id={id ? `${id}-search` : undefined}
                        aria-label={`搜索${filterLabel}`}
                        placeholder={`搜索${filterLabel}`}
                        showTrigger={false}
                    />
                )}
                <ComboboxEmpty>{emptyLabel}</ComboboxEmpty>
                <ComboboxList>
                    {items.map((item) => (
                        <ComboboxItem
                            key={item.value}
                            id={
                                id
                                    ? `${id}-option-${toAutomationIdSegment(item.value)}`
                                    : undefined
                            }
                            value={item}
                            disabled={item.disabled}
                        >
                            <span className="truncate">{item.label}</span>
                        </ComboboxItem>
                    ))}
                </ComboboxList>
            </ComboboxContent>
        </Combobox>
    )
}
