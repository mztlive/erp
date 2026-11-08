"use client"

import type * as React from "react"

import { ListSearchField } from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"

/** 供应商门户列表共用的搜索条。附加筛选取 children，插在搜索框和提交按钮之间。 */
export function PortalListSearchForm({
    searchId,
    searchLabel,
    placeholder,
    value,
    onChange,
    submitId,
    onSubmit,
    children,
}: {
    searchId: string
    searchLabel: string
    placeholder: string
    value: string
    onChange: (value: string) => void
    submitId: string
    onSubmit: () => void
    children?: React.ReactNode
}) {
    return (
        <form
            className="flex w-full flex-wrap items-center gap-2 [&>[data-slot=input-group]]:w-full [&>[data-slot=input-group]]:sm:w-96"
            onSubmit={(event) => {
                event.preventDefault()
                onSubmit()
            }}
        >
            <ListSearchField
                id={searchId}
                aria-label={searchLabel}
                placeholder={placeholder}
                value={value}
                onChange={onChange}
            />
            {children}
            <Button id={submitId} type="submit" variant="outline">
                搜索
            </Button>
        </form>
    )
}
