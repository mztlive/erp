"use client"

import {
    Select,
    SelectContent,
    SelectItem,
    SelectTrigger,
    SelectValue,
} from "@/components/ui/select"
import { toAutomationIdSegment } from "@/lib/automation-id"

/** 少量固定选项直接选择，不提供搜索输入；业务候选检索使用远程 Combobox。 */
export function FilterSelect({
    id,
    label,
    value,
    options,
    onValueChange,
    className,
}: {
    id: string
    label: string
    value: string
    options: readonly { value: string; label: string }[]
    onValueChange: (value: string) => void
    className?: string
}) {
    return (
        <Select
            value={value}
            onValueChange={(next) => {
                if (next !== null) onValueChange(next)
            }}
            items={options}
        >
            <SelectTrigger id={id} aria-label={label} className={className}>
                <SelectValue />
            </SelectTrigger>
            <SelectContent>
                {options.map((option) => (
                    <SelectItem
                        id={`${id}-option-${toAutomationIdSegment(option.value)}`}
                        key={option.value}
                        value={option.value}
                    >
                        {option.label}
                    </SelectItem>
                ))}
            </SelectContent>
        </Select>
    )
}
