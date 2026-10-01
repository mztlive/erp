"use client"

import { NativeCheckbox } from "@/components/ui/checkbox"
import { toAutomationIdSegment } from "@/lib/automation-id"
import type { QualityFilterOption } from "../../dual-types"

export function CandidatePicker({
    idPrefix,
    title,
    options,
    selected,
    emptyLabel,
    onToggle,
}: {
    idPrefix: string
    title: string
    options: readonly QualityFilterOption[]
    selected: readonly string[]
    emptyLabel: string
    onToggle: (id: string) => void
}) {
    return (
        <fieldset className="min-w-0 rounded-xl border border-border p-3">
            <legend className="px-1 text-xs font-medium text-muted-foreground">
                {title}（{options.length}）
            </legend>
            {options.length === 0 ? (
                <p className="text-xs text-muted-foreground">{emptyLabel}</p>
            ) : (
                <ul className="flex max-h-40 min-w-0 flex-col gap-1 overflow-y-auto">
                    {options.map((option) => {
                        const checked = selected.includes(option.value)
                        const inputId = `${idPrefix}-option-${toAutomationIdSegment(option.value)}`
                        return (
                            <li key={option.value} className="min-w-0">
                                <label
                                    htmlFor={inputId}
                                    className="flex min-w-0 cursor-pointer items-center gap-2 text-body-compact"
                                >
                                    <NativeCheckbox
                                        id={inputId}
                                        checked={checked}
                                        onCheckedChange={() =>
                                            onToggle(option.value)
                                        }
                                    />
                                    <span className="min-w-0 truncate">
                                        {option.label}
                                    </span>
                                </label>
                            </li>
                        )
                    })}
                </ul>
            )}
        </fieldset>
    )
}
