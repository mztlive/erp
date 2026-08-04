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
  ComboboxList,
  ComboboxValue,
  useComboboxAnchor,
} from "@/components/ui/combobox"
import type { ComboboxOption } from "@/components/business/option-combobox"
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

export type MultiOptionComboboxProps = {
  options: readonly ComboboxOption[]
  /** 已选值；多选用逗号分隔值数组承载。 */
  value: readonly string[]
  onValueChange: (value: string[]) => void
  placeholder?: string
  emptyLabel?: string
  disabled?: boolean
  id?: string
  "aria-label"?: string
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
  emptyLabel = "没有符合条件的选项",
  disabled = false,
  id,
  "aria-label": ariaLabel,
  className,
  size = "default",
}: MultiOptionComboboxProps) {
  const items = React.useMemo(() => toInternal(options), [options])
  const selected = React.useMemo(
    () => items.filter((item) => value.includes(item.value)),
    [items, value]
  )
  const anchorRef = useComboboxAnchor()
  const inputRef = React.useRef<HTMLInputElement | null>(null)

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
        onClick={() => inputRef.current?.focus()}
      >
        <ComboboxChips
          className={cn(
            size === "sm" && "min-h-7 py-0.5 *:data-[slot=combobox-chip]:h-5"
          )}
        >
          <ComboboxValue>
            {(valueItems: InternalOption[]) =>
              valueItems.map((item) => (
                <ComboboxChip key={item.value} aria-label={item.label}>
                  {item.label}
                </ComboboxChip>
              ))
            }
          </ComboboxValue>
          <ComboboxChipsInput
            ref={inputRef}
            id={id}
            aria-label={ariaLabel}
            placeholder={selected.length > 0 ? "" : placeholder}
            disabled={disabled}
            className={cn(size === "sm" && "text-xs")}
          />
        </ComboboxChips>
      </div>
      <ComboboxContent anchor={anchorRef}>
        <ComboboxEmpty>{emptyLabel}</ComboboxEmpty>
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
    </Combobox>
  )
}
