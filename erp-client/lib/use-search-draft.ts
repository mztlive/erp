"use client"

/**
 * 列表搜索草稿：输入只改本地 Draft，提交由各页面的 applyFilters 完成。
 *
 * URL 回填默认同步草稿；`skipWhileFocused` 用于保留用户正在编辑、尚未提交的
 * 关键词（列表页搜索框聚焦时不被 URL 覆盖）。
 */

import * as React from "react"

export function useSearchDraft(
    value: string | null | undefined,
    options: {
        skipWhileFocused?: boolean
        inputRef?: React.RefObject<HTMLInputElement | null>
    } = {},
) {
    const internalRef = React.useRef<HTMLInputElement | null>(null)
    const searchInputRef = options.inputRef ?? internalRef
    const [searchDraft, setSearchDraft] = React.useState(value ?? "")

    React.useEffect(() => {
        if (
            options.skipWhileFocused &&
            document.activeElement === searchInputRef.current
        )
            return
        setSearchDraft(value ?? "")
    }, [value, options.skipWhileFocused, searchInputRef])

    return { searchDraft, setSearchDraft, searchInputRef }
}
