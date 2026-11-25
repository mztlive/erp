"use client"

import * as React from "react"
import { ChevronDownIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import {
    Combobox,
    ComboboxChip,
    ComboboxChips,
    ComboboxChipsInput,
    ComboboxContent,
    ComboboxEmpty,
    ComboboxInput,
    ComboboxItem,
    ComboboxList,
    ComboboxTrigger,
    ComboboxValue,
    useComboboxAnchor,
} from "@/components/ui/combobox"
import {
    MISSING_SELECTION_CHECKING_LABEL,
    MISSING_SELECTION_UNAVAILABLE_LABEL,
    remoteSearchFromInputChange,
} from "@/components/ui/combobox-input-search"
import {
    ComboboxRefresh,
    type ComboboxRefreshProps,
} from "@/components/ui/combobox-refresh"
import type { ComboboxOption } from "@/components/ui/option-combobox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

type InternalOption = ComboboxOption & { __search: string }

function toInternal(options: readonly ComboboxOption[]): InternalOption[] {
    return options.map((option) => ({
        ...option,
        __search: [option.label, option.value, option.keywords]
            .filter(Boolean)
            .join(" "),
    }))
}

export type MultiOptionComboboxProps = ComboboxRefreshProps & {
    options: readonly ComboboxOption[]
    /** 已选值；多选用逗号分隔值数组承载。 */
    value: readonly string[]
    onValueChange: (value: string[]) => void
    placeholder?: string
    /** 查询栏使用固定高度的名称与选择摘要，避免多选标签撑高控件。 */
    filterLabel?: string
    /** 外层已显示字段标签时，触发器只显示选择摘要，保留弹层搜索。 */
    hideFilterLabel?: boolean
    emptyLabel?: string
    disabled?: boolean
    required?: boolean
    id?: string
    /** 透传给可见输入或触发器，支持服务端字段错误自动聚焦。 */
    name?: string
    "aria-label"?: string
    "aria-invalid"?: boolean
    "aria-describedby"?: string
    onBlur?: () => void
    /** 已选标签的短文案；选项列表仍使用完整 label。 */
    getChipLabel?: (option: ComboboxOption) => string
    className?: string
    /** 输入框/触发器外层容器的 className（宽度等） */
    inputClassName?: string
    size?: "sm" | "default"
    filterMode?: "local" | "remote"
    onSearchChange?: (query: string) => void
    loading?: boolean
    hasMore?: boolean
    onLoadMore?: () => void
    loadMoreId?: string
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
    hideFilterLabel = false,
    emptyLabel = "没有符合条件的选项",
    disabled = false,
    required = false,
    id,
    name,
    "aria-label": ariaLabel,
    "aria-invalid": ariaInvalid,
    "aria-describedby": ariaDescribedBy,
    onBlur,
    getChipLabel,
    className,
    inputClassName,
    size = "default",
    filterMode = "local",
    onSearchChange,
    loading = false,
    onRefresh,
    hasMore = false,
    onLoadMore,
    loadMoreId,
}: MultiOptionComboboxProps) {
    const items = React.useMemo(() => toInternal(options), [options])
    const selected = React.useMemo(
        () =>
            value.map(
                (key) =>
                    items.find((item) => item.value === key) ?? {
                        value: key,
                        label: loading
                            ? MISSING_SELECTION_CHECKING_LABEL
                            : MISSING_SELECTION_UNAVAILABLE_LABEL,
                        __search: "",
                    },
            ),
        [items, value, loading],
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
            onInputValueChange={(query, details) => {
                if (filterMode !== "remote") return
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
        >
            <div
                ref={anchorRef}
                data-slot="multi-option-combobox"
                data-size={size}
                data-has-value={selected.length > 0 ? "true" : undefined}
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
                                name={name}
                                variant="control"
                                size={size}
                                className={cn(
                                    "w-full min-w-0 justify-start gap-1 px-2.5",
                                    inputClassName,
                                )}
                            />
                        }
                    >
                        {!hideFilterLabel && (
                            <span
                                data-filter-label=""
                                className="shrink-0 text-muted-foreground/80"
                            >
                                {filterLabel}：
                            </span>
                        )}
                        <span
                            className={cn(
                                "min-w-0 flex-1 truncate text-left",
                                selected.length === 0 &&
                                    "text-muted-foreground/75",
                            )}
                            data-slot="filter-selection-summary"
                        >
                            {selected.length === 0
                                ? "全部"
                                : selected.length === 1
                                  ? selected[0].label
                                  : `已选 ${selected.length} 项`}
                        </span>
                    </ComboboxTrigger>
                ) : (
                    <ComboboxChips
                        aria-invalid={ariaInvalid || undefined}
                        className={cn(
                            "min-w-0",
                            size === "sm" &&
                                "min-h-control-sm py-0.5 *:data-[slot=combobox-chip]:h-5",
                            inputClassName,
                        )}
                    >
                        <ComboboxValue>
                            {(valueItems: InternalOption[]) =>
                                valueItems.map((item) => (
                                    <ComboboxChip
                                        key={item.value}
                                        className="max-w-full min-w-0"
                                        removeId={
                                            id
                                                ? `${id}-chip-${toAutomationIdSegment(item.value)}-remove`
                                                : undefined
                                        }
                                        aria-label={item.label}
                                    >
                                        <span className="min-w-0 truncate">
                                            {getChipLabel?.(item) ?? item.label}
                                        </span>
                                    </ComboboxChip>
                                ))
                            }
                        </ComboboxValue>
                        <ComboboxChipsInput
                            id={id}
                            name={name}
                            aria-label={ariaLabel}
                            aria-invalid={ariaInvalid || undefined}
                            aria-describedby={ariaDescribedBy}
                            placeholder={selected.length > 0 ? "" : placeholder}
                            disabled={disabled}
                            aria-required={required || undefined}
                            onBlur={onBlur}
                            className={cn(
                                "min-w-0 truncate",
                                size === "sm" && "text-xs",
                            )}
                        />
                        <ChevronDownIcon
                            aria-hidden="true"
                            className="pointer-events-none ml-auto mr-0.5 size-3.5 shrink-0 text-muted-foreground/75"
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
                <ComboboxEmpty>
                    {loading ? "正在加载" : emptyLabel}
                </ComboboxEmpty>
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
                            <span className="min-w-0 truncate">
                                {item.label}
                            </span>
                        </ComboboxItem>
                    ))}
                </ComboboxList>
                {hasMore && onLoadMore ? (
                    <Button
                        id={loadMoreId}
                        type="button"
                        variant="ghost"
                        size="sm"
                        className="w-full"
                        disabled={loading}
                        onClick={onLoadMore}
                    >
                        加载更多
                    </Button>
                ) : null}
                <ComboboxRefresh
                    id={id}
                    onRefresh={onRefresh}
                    loading={loading}
                    disabled={disabled}
                />
            </ComboboxContent>
        </Combobox>
    )
}
