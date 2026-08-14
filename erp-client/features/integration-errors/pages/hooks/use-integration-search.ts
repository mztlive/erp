import * as React from "react"

export function useIntegrationSearch({
    q,
    onCommitSearch,
}: {
    q: string | undefined
    onCommitSearch: (q: string | null) => void
}) {
    const [searchDraft, setSearchDraft] = React.useState(q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // URL 搜索变化时回写输入框（浏览器前进/后退同步）
    React.useEffect(() => {
        setSearchDraft(q ?? "")
    }, [q])

    // P3 搜索：300ms 防抖写 URL，Enter 兜底，/ 聚焦
    React.useEffect(() => {
        const handle = globalThis.setTimeout(() => {
            if (searchDraft.trim() === (q ?? "")) return
            onCommitSearch(searchDraft.trim() || null)
        }, 300)
        return () => globalThis.clearTimeout(handle)
        // eslint-disable-next-line react-hooks/exhaustive-deps -- 以当前渲染快照为准
    }, [searchDraft])

    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key !== "/" ||
                event.metaKey ||
                event.ctrlKey ||
                event.altKey
            ) {
                return
            }
            const target = event.target as HTMLElement | null
            const tag = target?.tagName
            if (
                tag === "INPUT" ||
                tag === "TEXTAREA" ||
                tag === "SELECT" ||
                target?.isContentEditable
            ) {
                return
            }
            event.preventDefault()
            searchInputRef.current?.focus()
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    return { searchDraft, setSearchDraft, searchInputRef }
}
