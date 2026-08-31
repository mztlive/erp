"use client"

import * as React from "react"

import { RadioGroup, RadioGroupItem } from "@/components/ui/radio-group"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"

export type FixedOptionRadioFilterOption<Value extends string = string> =
    Readonly<{
        value: Value
        label: string
        disabled?: boolean
    }>

export type FixedOptionRadioFilterProps<Value extends string = string> = {
    label: string
    value: Value
    options: readonly FixedOptionRadioFilterOption<Value>[]
    onValueChange: (value: Value) => void
    disabled?: boolean
    "aria-label"?: string
    className?: string
    id?: string
    idPrefix?: string
}

/**
 * 固定枚举筛选。
 *
 * 选项横向铺开，未选项使用虚线边框，选中项切换为实线边框。
 * 组件自身占据完整筛选行，适用于选项数量固定且无需搜索的业务筛选。
 */
export function FixedOptionRadioFilter<Value extends string>({
    label,
    value,
    options,
    onValueChange,
    disabled = false,
    "aria-label": ariaLabel,
    className,
    id,
    idPrefix,
}: FixedOptionRadioFilterProps<Value>) {
    const labelId = React.useId()
    const baseId = idPrefix ?? id

    return (
        <div
            data-slot="fixed-option-radio-filter"
            className={cn(
                "grid min-w-0 gap-2 sm:grid-cols-[4.5rem_minmax(0,1fr)] sm:items-center",
                className,
            )}
        >
            <span id={labelId} className="text-sm text-muted-foreground">
                {label}
            </span>
            <RadioGroup
                value={value}
                onValueChange={(nextValue) => onValueChange(nextValue as Value)}
                disabled={disabled}
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
                        <RadioGroupItem
                            id={
                                baseId
                                    ? `${baseId}-option-${toAutomationIdSegment(option.value)}`
                                    : undefined
                            }
                            value={option.value}
                            disabled={option.disabled}
                            aria-label={option.label}
                            className="absolute inset-0 z-10 h-full w-full aspect-auto cursor-pointer rounded-md border-0 bg-transparent opacity-0"
                        />
                        <span aria-hidden="true">{option.label}</span>
                    </div>
                ))}
            </RadioGroup>
        </div>
    )
}
