"use client"

import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { MultiOptionCombobox } from "@/components/business/multi-option-combobox"

export type ResponsibleUserOption = { value: string; label: string }

/** 只读查询条件：值保存稳定人员 ID，未知候选仍可查看和清除。 */
export const ResponsibleUserFilter = ({
    id,
    value,
    onChange,
    options,
    label = "负责人",
    hideLabel = false,
    compact = false,
}: {
    id: string
    value: string
    onChange: (value: string) => void
    options: readonly ResponsibleUserOption[]
    label?: string
    hideLabel?: boolean
    compact?: boolean
}) => {
    const selected = value.split(",").filter(Boolean)
    const available = new Set(options.map((option) => option.value))
    const items = [
        ...options,
        ...selected
            .filter((key) => !available.has(key))
            .map((key) => ({ value: key, label: "已选人员（当前不可用）" })),
    ]
    const control = (
        <MultiOptionCombobox
            id={id}
            aria-label={label}
            filterLabel={hideLabel || compact ? label : undefined}
            hideFilterLabel={compact}
            value={selected}
            options={items}
            onValueChange={(ids) =>
                onChange([...new Set(ids)].sort().join(","))
            }
            placeholder={`全部${label}`}
        />
    )
    if (compact) {
        return (
            <ListWorkspaceFilterField htmlFor={id} label={label}>
                {control}
            </ListWorkspaceFilterField>
        )
    }
    return (
        <div className={hideLabel ? "min-w-0" : "min-w-0 space-y-1.5"}>
            <label
                className={
                    hideLabel ? "sr-only" : "text-xs text-muted-foreground"
                }
                htmlFor={id}
            >
                {label}
            </label>
            {control}
        </div>
    )
}
