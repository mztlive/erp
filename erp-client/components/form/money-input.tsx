"use client"

import type { ComponentProps } from "react"
import { Input } from "@/components/ui/input"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"

/** 受控金额仍以元提交，辅助读数不改写输入或业务校验。 */
export function MoneyInput({
    id,
    value,
    className,
    inputMode = "decimal",
    "aria-describedby": describedBy,
    ...props
}: Omit<ComponentProps<typeof Input>, "id" | "value"> & {
    id: string
    value: string
}) {
    const hint = describeMoney(value)?.readout
    const hintId = `${id}-money-hint`
    return (
        <span className="flex min-w-0 flex-1 flex-col gap-1">
            <Input
                {...props}
                id={id}
                value={value}
                inputMode={inputMode}
                className={cn("num", className)}
                aria-describedby={
                    [describedBy, hint ? hintId : undefined]
                        .filter(Boolean)
                        .join(" ") || undefined
                }
            />
            {hint ? (
                <span
                    id={hintId}
                    className="text-xs font-normal text-muted-foreground"
                >
                    {hint}
                </span>
            ) : null}
        </span>
    )
}
