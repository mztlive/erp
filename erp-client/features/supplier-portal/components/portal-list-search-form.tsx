"use client"

import * as React from "react"

import {
    ListSearchField,
    ListWorkspaceFilterBar,
} from "@/components/business/list-workspace"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

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
    hasPendingChanges,
    resultStatus,
}: {
    searchId: string
    searchLabel: string
    placeholder: string
    value: string
    onChange: (value: string) => void
    submitId: string
    onSubmit: () => void
    children?: React.ReactNode
    hasPendingChanges?: boolean
    resultStatus?: React.ReactNode
}) {
    const searchInputRef = React.useRef<HTMLInputElement>(null)
    useSearchHotkey(searchInputRef)

    return (
        <ListWorkspaceFilterBar
            idPrefix={searchId}
            morePresentation="popover"
            formAriaLabel={searchLabel}
            onSubmit={onSubmit}
            queryButtonId={submitId}
            hasPendingChanges={hasPendingChanges}
            resultStatus={resultStatus}
            search={
                <ListSearchField
                    id={searchId}
                    searchInputRef={searchInputRef}
                    aria-label={searchLabel}
                    placeholder={placeholder}
                    value={value}
                    onChange={onChange}
                />
            }
            primaryFilters={children}
        />
    )
}
