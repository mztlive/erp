"use client"

import type * as React from "react"

import { ListFilterRangeField } from "@/components/business/list-workspace"

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
        <ListFilterRangeField
            label="含税售价"
            fieldClassName={fieldClassName}
            minId={minId}
            maxId={maxId}
            errorId={errorId}
            inputClassName={inputClassName}
            inputRef={inputRef}
            min={min}
            max={max}
            minAriaLabel="最低含税售价"
            maxAriaLabel="最高含税售价"
            unit="元"
            error={error}
            onMinChange={onMinChange}
            onMaxChange={onMaxChange}
        />
    )
}
