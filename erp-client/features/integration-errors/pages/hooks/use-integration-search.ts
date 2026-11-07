import * as React from "react"

import { useSearchHotkey } from "@/lib/use-search-hotkey"

export function useIntegrationSearch({ q }: { q: string | undefined }) {
    const [searchDraft, setSearchDraft] = React.useState(q ?? "")
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    React.useEffect(() => {
        setSearchDraft(q ?? "")
    }, [q])

    useSearchHotkey(searchInputRef)

    return { searchDraft, setSearchDraft, searchInputRef }
}
