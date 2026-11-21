"use client"

import * as React from "react"

import { usePurchaseOrdersListUrl } from "@/features/purchase-orders/hooks/use-purchase-orders-list-url"
import type { PurchaseType } from "@/features/purchase-orders/types"
import { useSearchDraft } from "@/lib/use-search-draft"

export function usePurchaseOrdersListFilters(
    searchInputRef: React.RefObject<HTMLInputElement | null>,
) {
    const listUrl = usePurchaseOrdersListUrl()
    const { canFilterOwner, url, pushUrl, search, statusFilter } = listUrl

    const [typeDraft, setTypeDraft] = React.useState<PurchaseType | "all">(
        url.purchaseType ?? "all",
    )
    React.useEffect(
        () => setTypeDraft(url.purchaseType ?? "all"),
        [url.purchaseType],
    )

    const [ownerDraft, setOwnerDraft] = React.useState(url.ownerUserIds ?? "")
    React.useEffect(
        () => setOwnerDraft(url.ownerUserIds ?? ""),
        [canFilterOwner, url.ownerUserIds],
    )

    const { searchDraft, setSearchDraft } = useSearchDraft(search, {
        skipWhileFocused: true,
        inputRef: searchInputRef,
    })

    const applyFilters = React.useCallback(() => {
        pushUrl({
            q: searchDraft.trim() || undefined,
            purchaseType: typeDraft === "all" ? undefined : typeDraft,
            ownerUserIds: canFilterOwner ? ownerDraft || undefined : undefined,
            page: 1,
        })
    }, [canFilterOwner, pushUrl, searchDraft, ownerDraft, typeDraft])

    const clearSourceOrder = React.useCallback(() => {
        pushUrl({ salesOrderId: undefined, page: 1 })
    }, [pushUrl])

    const clearAllFilters = React.useCallback(() => {
        setTypeDraft("all")
        setSearchDraft("")
        setOwnerDraft("")
        pushUrl({
            purchaseType: undefined,
            q: undefined,
            ownerUserIds: undefined,
            salesOrderId: undefined,
            status: "all",
            metric: "all",
            page: 1,
        })
    }, [pushUrl, setSearchDraft])

    return {
        ...listUrl,
        typeDraft,
        setTypeDraft,
        ownerDraft,
        setOwnerDraft,
        searchDraft,
        setSearchDraft,
        hasActiveFilters: Boolean(
            url.purchaseType ||
            search ||
            url.ownerUserIds ||
            url.salesOrderId ||
            statusFilter !== "all" ||
            url.metric !== "all",
        ),
        hasPendingChanges:
            typeDraft !== (url.purchaseType ?? "all") ||
            searchDraft.trim() !== search.trim() ||
            (canFilterOwner && ownerDraft !== (url.ownerUserIds ?? "")),
        applyFilters,
        clearAllFilters,
        clearSourceOrder,
    }
}
