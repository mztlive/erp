"use client"

import * as React from "react"

import {
    remoteSearchFromInputChange,
    useStickySelected,
} from "@/components/business/combobox-input-search"
import {
    Combobox,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
} from "@/components/ui/combobox"
import { cn } from "@/lib/utils"

/** 通用可搜索选项；用于枚举、筛选与轻量业务列表。 */
export type ComboboxOption = Readonly<{
    value: string
    label: string
    disabled?: boolean
    /** 额外参与搜索的文本（编号、别名等） */
    keywords?: string
}>

export type OptionComboboxProps = {
    options: readonly ComboboxOption[]
    value?: string | null
    onValueChange: (value: string | null) => void
    placeholder?: string
    emptyLabel?: string
    searchPlaceholder?: string
    /** 服务端已完成搜索时关闭本地二次过滤。 */
    filterMode?: "local" | "remote"
    /** 输入变化通知；远程模式下用于触发查询。 */
    onSearchChange?: (query: string) => void
    disabled?: boolean
    loading?: boolean
    /** 是否显示清除按钮；默认 true */
    allowClear?: boolean
    required?: boolean
    id?: string
    "aria-label"?: string
    "aria-invalid"?: boolean
    className?: string
    /** 输入框外层 InputGroup 的 className（宽度等） */
    inputClassName?: string
    size?: "sm" | "default"
    onBlur?: () => void
}

type InternalOption = ComboboxOption & { __search: string }

function toInternal(options: readonly ComboboxOption[]): InternalOption[] {
    return options.map((option) => ({
        ...option,
        __search: [option.label, option.value, option.keywords]
            .filter(Boolean)
            .join(" "),
    }))
}

/**
 * 基于 `components/ui/combobox` 的可搜索单选。
 * 用于替代 Select / NativeSelect：筛选条、表单枚举等。
 */
export function OptionCombobox({
    options,
    value,
    onValueChange,
    placeholder = "请选择",
    searchPlaceholder,
    emptyLabel = "没有符合条件的选项",
    disabled = false,
    loading = false,
    filterMode = "local",
    onSearchChange,
    allowClear = true,
    required = false,
    id,
    "aria-label": ariaLabel,
    "aria-invalid": ariaInvalid,
    className,
    inputClassName,
    size = "default",
    onBlur,
}: OptionComboboxProps) {
    const items = React.useMemo(() => toInternal(options), [options])
    const selected = useStickySelected(items, value ?? undefined, (item) => item.value)

    return (
        <Combobox
            items={items}
            value={selected}
            onValueChange={(next) => {
                onValueChange(next?.value ?? null)
            }}
            onInputValueChange={(query, details) => {
                const nextQuery = remoteSearchFromInputChange(
                    query,
                    details.reason,
                )
                if (nextQuery !== undefined) onSearchChange?.(nextQuery)
            }}
            itemToStringLabel={(item) => item.label}
            itemToStringValue={(item) => item.value}
            isItemEqualToValue={(item, current) => item.value === current.value}
            filter={(item, query) => {
                if (filterMode === "remote") return true
                const q = query.trim().toLowerCase()
                if (!q) return true
                return item.__search.toLowerCase().includes(q)
            }}
            disabled={disabled}
            required={required}
        >
            <div
                data-slot="option-combobox"
                data-size={size}
                className={cn("min-w-0", className)}
            >
                <ComboboxInput
                    id={id}
                    aria-label={ariaLabel}
                    aria-invalid={ariaInvalid || undefined}
                    aria-busy={loading || undefined}
                    placeholder={searchPlaceholder ?? placeholder}
                    showClear={allowClear && !disabled}
                    disabled={disabled}
                    onBlur={onBlur}
                    className={cn(
                        "w-full",
                        size === "sm" &&
                            "h-7 min-h-7 *:data-[slot=input-group-control]:h-7 *:data-[slot=input-group-control]:text-xs",
                        inputClassName,
                    )}
                />
                <ComboboxContent>
                    <ComboboxEmpty>
                        {loading ? "正在加载…" : emptyLabel}
                    </ComboboxEmpty>
                    <ComboboxList>
                        {items.map((item) => (
                            <ComboboxItem
                                key={item.value}
                                value={item}
                                disabled={item.disabled}
                            >
                                <span className="truncate">{item.label}</span>
                            </ComboboxItem>
                        ))}
                    </ComboboxList>
                </ComboboxContent>
            </div>
        </Combobox>
    )
}
