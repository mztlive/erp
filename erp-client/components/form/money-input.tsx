"use client"

import type { ComponentProps, ReactNode } from "react"

import { Input } from "@/components/ui/input"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import { describeMoney } from "@/lib/money-display"
import { cn } from "@/lib/utils"

export type MoneyInputProps = Omit<
    ComponentProps<typeof Input>,
    "id" | "value" | "prefix"
> & {
    id: string
    value: string
    /** 可选货币前缀；传入 `true` 时默认显示 `¥`。 */
    prefix?: ReactNode | boolean
    /** 是否显示保留全精度的中文辅助读数；默认 true。 */
    showReadout?: boolean
    align?: "start" | "end"
}

/** 受控金额仍以元提交，辅助读数不改写输入或业务校验。 */
export function MoneyInput({
    id,
    value,
    className,
    inputMode = "decimal",
    prefix,
    showReadout = true,
    align = "start",
    size,
    "aria-describedby": describedBy,
    ...props
}: MoneyInputProps) {
    const resolvedPrefix = prefix === true ? "¥" : prefix || undefined
    const hint = showReadout ? describeMoney(value)?.readout : null
    const hintId = `${id}-money-hint`
    const mergedDescribedBy =
        [describedBy, hint ? hintId : undefined].filter(Boolean).join(" ") ||
        undefined
    const numericClassName = cn(
        "num",
        align === "end" && "text-right",
        className,
    )

    return (
        <span className="flex min-w-0 flex-1 flex-col gap-1">
            {resolvedPrefix ? (
                <InputGroup
                    className={cn(
                        size === "sm" && "h-control-sm",
                        size === "lg" && "h-control-lg",
                    )}
                >
                    <InputGroupAddon
                        align="inline-start"
                        className="pr-0 text-muted-foreground/80"
                    >
                        {resolvedPrefix}
                    </InputGroupAddon>
                    <InputGroupInput
                        {...props}
                        id={id}
                        value={value}
                        inputMode={inputMode}
                        className={numericClassName}
                        aria-describedby={mergedDescribedBy}
                    />
                </InputGroup>
            ) : (
                <Input
                    {...props}
                    id={id}
                    size={size}
                    value={value}
                    inputMode={inputMode}
                    className={numericClassName}
                    aria-describedby={mergedDescribedBy}
                />
            )}
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
