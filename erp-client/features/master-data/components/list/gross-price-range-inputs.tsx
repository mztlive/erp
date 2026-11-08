"use client"

import type * as React from "react"

import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"

/** 商品列表与公司商品池共用的一件代发含税售价区间。 */
export function GrossPriceRangeInputs({
    fieldClassName,
    minId,
    maxId,
    errorId,
    inputClassName,
    inputRef,
    min,
    max,
    error,
    onMinChange,
    onMaxChange,
}: {
    fieldClassName?: string
    minId: string
    maxId: string
    errorId: string
    inputClassName: string
    inputRef?: React.Ref<HTMLInputElement>
    min: string
    max: string
    error: string | null
    onMinChange: (value: string) => void
    onMaxChange: (value: string) => void
}) {
    return (
        <ListWorkspaceFilterField
            className={fieldClassName}
            label="含税售价 · 元"
        >
            <div className="flex min-w-0 items-center gap-2">
                <Input
                    id={minId}
                    ref={inputRef}
                    className={inputClassName}
                    value={min}
                    onChange={(event) => onMinChange(event.target.value)}
                    inputMode="decimal"
                    autoComplete="off"
                    placeholder="最低价"
                    aria-label="最低一件代发价"
                    aria-invalid={Boolean(error)}
                    aria-describedby={error ? errorId : undefined}
                />
                <span className="text-xs text-muted-foreground">至</span>
                <Input
                    id={maxId}
                    className={inputClassName}
                    value={max}
                    onChange={(event) => onMaxChange(event.target.value)}
                    inputMode="decimal"
                    autoComplete="off"
                    placeholder="最高价"
                    aria-label="最高一件代发价"
                    aria-invalid={Boolean(error)}
                    aria-describedby={error ? errorId : undefined}
                />
            </div>
            {error ? (
                <p
                    id={errorId}
                    className="text-xs text-destructive"
                    role="alert"
                >
                    {error}
                </p>
            ) : null}
        </ListWorkspaceFilterField>
    )
}
