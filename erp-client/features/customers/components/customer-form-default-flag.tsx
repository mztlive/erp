"use client"

import { Checkbox } from "@/components/ui/checkbox"
import { Label } from "@/components/ui/label"

/** 客户表单行内的默认标记。勾选值只接受 true。 */
export function CustomerFormDefaultFlag({
    id,
    label,
    checked,
    onCheckedChange,
}: {
    id: string
    label: string
    checked: boolean
    onCheckedChange: (checked: boolean) => void
}) {
    return (
        <Label htmlFor={id} className="flex items-center gap-2 text-sm">
            <Checkbox
                id={id}
                checked={checked}
                onCheckedChange={(next) => onCheckedChange(next === true)}
            />
            {label}
        </Label>
    )
}
