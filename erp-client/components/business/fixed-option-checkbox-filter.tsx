"use client"

import * as React from "react"

import { Checkbox } from "@/components/ui/checkbox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type FixedOptionCheckboxFilterOption<Value extends string = string> =
    Readonly<{
        value: Value
        label: string
        disabled?: boolean
    }>

export type FixedOptionCheckboxFilterProps<Value extends string = string> = {
    label: string
    value: readonly Value[]
    options: readonly FixedOptionCheckboxFilterOption<Value>[]
    onValueChange: (value: Value[]) => void
    disabled?: boolean
    "aria-label"?: string
    className?: string
    id?: string
    idPrefix?: string
}

/**
 * 固定枚举多选筛选。
 *
 * 沿用 `FixedOptionRadioFilter` 的整行布局和选项视觉；每个选项改为独立复选框，
 * 适用于需要同时命中多个固定枚举值的筛选条件。
 */
export function FixedOptionCheckboxFilter<Value extends string>({
    label,
    value,
    options,
    onValueChange,
    disabled = false,
    "aria-label": ariaLabel,
    className,
    id,
    idPrefix,
}: FixedOptionCheckboxFilterProps<Value>) {
    const labelId = React.useId()
    const baseId = idPrefix ?? id

    /** 将单个复选操作转换为按声明顺序排列的已选值。 */
    const changeOption = (optionValue: Value, checked: boolean) => {
        const selected = new Set(value)
        if (checked) selected.add(optionValue)
        else selected.delete(optionValue)
        onValueChange(
            options
                .map((option) => option.value)
                .filter((candidate) => selected.has(candidate)),
        )
    }

    return (
        <div
            data-slot="fixed-option-checkbox-filter"
            className={cn(
                "grid min-w-0 gap-2 sm:grid-cols-[4.5rem_minmax(0,1fr)] sm:items-center",
                className,
            )}
        >
            <span id={labelId} className="text-sm text-muted-foreground">
                {label}
            </span>
            <div
                role="group"
                aria-label={ariaLabel}
                aria-labelledby={ariaLabel ? undefined : labelId}
                className="flex w-full flex-wrap gap-1.5"
            >
                {options.map((option) => (
                    <div
                        key={option.value}
                        className={cn(
                            "relative flex h-8 min-w-14 items-center justify-center rounded-md border border-dashed border-border bg-transparent px-2.5 text-xs font-normal text-muted-foreground transition-colors",
                            "hover:border-foreground/60 hover:bg-muted/30 hover:text-foreground",
                            "has-data-checked:border-solid has-data-checked:border-primary has-data-checked:font-medium has-data-checked:text-foreground",
                            "has-[:focus-visible]:border-ring has-[:focus-visible]:ring-3 has-[:focus-visible]:ring-ring/30",
                            "has-data-disabled:cursor-not-allowed has-data-disabled:opacity-50",
                        )}
                    >
                        <Checkbox
                            id={
                                baseId
                                    ? `${baseId}-option-${toAutomationIdSegment(option.value)}`
                                    : undefined
                            }
                            checked={value.includes(option.value)}
                            disabled={disabled || option.disabled}
                            onCheckedChange={(checked) =>
                                changeOption(option.value, checked === true)
                            }
                            aria-label={option.label}
                            className="absolute inset-0 z-10 h-full w-full cursor-pointer rounded-md border-0 bg-transparent opacity-0"
                        />
                        <span aria-hidden="true">{option.label}</span>
                    </div>
                ))}
            </div>
        </div>
    )
}
