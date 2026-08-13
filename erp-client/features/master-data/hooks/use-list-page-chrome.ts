"use client"

import * as React from "react"

import { useSlashSearchHotkey } from "@/features/master-data/hooks/use-slash-search-hotkey"

/** 列表页共用的搜索框、结果标题与行焦点。 */
export function useListPageChrome() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const resultsHeadingRef = React.useRef<HTMLHeadingElement | null>(null)
    const lastFocusedRowId = React.useRef<string | null>(null)
    useSlashSearchHotkey(searchInputRef)
    return { searchInputRef, resultsHeadingRef, lastFocusedRowId }
}
