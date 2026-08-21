"use client"

import * as React from "react"

/**
 * 列表页搜索草稿（docs/ui-filter-design.md §5）：
 * 输入框受控于本地草稿，只有表单提交（applyFilters）会写 URL；
 * URL 回填时若搜索框正处于编辑状态，则不覆盖尚未提交的内容。
 */
export function useSupplierOrdersSearchDraft({
    q,
    searchInputRef,
}: {
    q: string | undefined
    searchInputRef: React.RefObject<HTMLInputElement | null>
}) {
    const [searchDraft, setSearchDraft] = React.useState(q ?? "")

    React.useEffect(() => {
        if (document.activeElement !== searchInputRef.current) {
            setSearchDraft(q ?? "")
        }
    }, [q, searchInputRef])

    return { searchDraft, setSearchDraft }
}
