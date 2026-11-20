"use client"

import type * as React from "react"

import { ListWorkspaceFilterField } from "@/components/business/list-workspace"
import { Input } from "@/components/ui/input"
import { cn } from "@/lib/utils"

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
        <div className={cn("min-w-0", fieldClassName)}>
            <ListWorkspaceFilterField label="含税售价">
                <div className="flex min-w-0 items-center gap-1">
                    <Input
                        id={minId}
                        ref={inputRef}
                        className={inputClassName}
                        value={min}
                        onChange={(event) => onMinChange(event.target.value)}
                        inputMode="decimal"
                        autoComplete="off"
                        placeholder="最低"
                        aria-label="最低一件代发价"
                        aria-invalid={Boolean(error)}
                        aria-describedby={error ? errorId : undefined}
                    />
                    <span className="shrink-0 text-xs text-muted-foreground/65">
                        —
                    </span>
                    <Input
                        id={maxId}
                        className={inputClassName}
                        value={max}
                        onChange={(event) => onMaxChange(event.target.value)}
                        inputMode="decimal"
                        autoComplete="off"
                        placeholder="最高"
                        aria-label="最高一件代发价"
                        aria-invalid={Boolean(error)}
                        aria-describedby={error ? errorId : undefined}
                    />
                    <span className="shrink-0 pr-1.5 text-xs text-muted-foreground/75">
                        元
                    </span>
                </div>
            </ListWorkspaceFilterField>
            {error ? (
                <p
                    id={errorId}
                    className="mt-1 text-xs text-destructive"
                    role="alert"
                >
                    {error}
                </p>
            ) : null}
        </div>
    )
}
