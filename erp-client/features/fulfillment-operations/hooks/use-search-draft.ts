"use client"

import * as React from "react"

/**
 * 搜索框的本地草稿：输入过程不提交，回车/失焦才把差异回给上层。
 * 外部值（URL）变化时草稿重新同步，保证「清除筛选」后输入框内容一致。
 */
export function useSearchDraft(
    committed: string | undefined,
    onCommit: (next: string | null) => void,
): {
    searchDraft: string
    setSearchDraft: React.Dispatch<React.SetStateAction<string>>
    commitSearch: () => void
} {
    const [searchDraft, setSearchDraft] = React.useState(committed ?? "")
    React.useEffect(() => {
        setSearchDraft(committed ?? "")
    }, [committed])

    const commitSearch = React.useCallback(() => {
        const next = searchDraft.trim()
        if (next === (committed ?? "")) return
        onCommit(next || null)
    }, [searchDraft, committed, onCommit])

    return { searchDraft, setSearchDraft, commitSearch }
}
