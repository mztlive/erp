"use client"

import * as React from "react"
import {
    Combobox,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
} from "@/components/ui/combobox"
import { InputGroupAddon } from "@/components/ui/input-group"
import {
    remoteSearchFromInputChange,
    useStickySelected,
} from "@/components/business/combobox-input-search"
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
    onBlur?: () => void
    /** 服务端已完成搜索时关闭本地二次过滤。 */
    filterMode?: "local" | "remote"
    label: string
    /** 查询栏常驻名称。不传时选择器外观保持原样。 */
    filterLabel?: string
    placeholder?: string
    emptyLabel?: string
    loading?: boolean
    disabled?: boolean
    required?: boolean
    allowClear?: boolean
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
    onBlur,
    filterMode = "local",
    label,
    filterLabel,
    placeholder = "搜索名称或编号",
    emptyLabel = "没有符合条件的对象",
    loading = false,
    disabled = false,
    required = false,
    allowClear = true,
    id,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedBy,
    className,
}: BusinessObjectComboboxProps) {
    const resolved = useStickySelected(
        items,
        value,
        (item) => item.id,
        filterMode !== "remote",
    )
    // 远程授权未确认时仅保留身份，不能回退到组件内保存的旧名称。
    const selected: BusinessObjectOption | null =
        resolved ??
        (value
            ? {
                  id: value,
                  code: "",
                  label: loading
                      ? "已选对象（正在核对）"
                      : "已选对象（当前不可用）",
                  status: { label: "待核对", tone: "neutral" },
              }
            : null)

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
                    showClear={allowClear}
                    onBlur={onBlur}
                    disabled={disabled}
                    className="w-full"
                >
                    {filterLabel ? (
                        <InputGroupAddon className="shrink-0 whitespace-nowrap font-normal">
                            {filterLabel}：
                        </InputGroupAddon>
                    ) : null}
                </ComboboxInput>
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

export {
    BusinessObjectCombobox,
    type BusinessObjectComboboxProps,
    type BusinessObjectOption,
}
