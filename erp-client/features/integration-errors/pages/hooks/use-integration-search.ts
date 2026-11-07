import { useSearchDraft } from "@/lib/use-search-draft"
import { useSearchHotkey } from "@/lib/use-search-hotkey"

export function useIntegrationSearch({ q }: { q: string | undefined }) {
    const draft = useSearchDraft(q)
    useSearchHotkey(draft.searchInputRef)
    return draft
}
