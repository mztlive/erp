"use client"

import * as React from "react"
import { useSearchParams } from "next/navigation"

import type { HistoryQualityQuery } from "../dual-types"
import type { PatchDual } from "../lib/dual-filter-state"
import {
    parseCsvIds,
    parseHistoryDimension,
    parseHistorySort,
    parseDualPage,
    parseDualPageSize,
    serializeCsvIds,
} from "../lib/dual-url-state"

export function useHistoryQualityFilters({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const searchParams = useSearchParams()
    const userIds = parseCsvIds(searchParams.get("attributionUserIds"))
    const orgIds = parseCsvIds(searchParams.get("attributionOrgUnitIds"))
    const attributionGroup = searchParams.get("attributionGroup") ?? undefined
    const customerId = searchParams.get("dualCustomerId") ?? undefined
    const qParam = searchParams.get("dualQ") ?? ""
    const dimension = parseHistoryDimension(searchParams.get("dualDimension"))
    const sort = parseHistorySort(searchParams.get("dualSort"))
    const page = parseDualPage(searchParams.get("dualPage"))
    const pageSize = parseDualPageSize(searchParams.get("dualPageSize"))
    const scopeVersion = searchParams.get("scopeVersion") ?? undefined

    const [qDraft, setQDraft] = React.useState(qParam)
    const [idDraft, setIdDraft] = React.useState("")
    React.useEffect(() => {
        if (document.activeElement?.id !== "customers-quality-dual-search") {
            setQDraft(qParam)
        }
    }, [qParam])

    const query: HistoryQualityQuery = React.useMemo(
        () => ({
            from,
            to,
            attributionUserIds: userIds.length ? userIds : undefined,
            attributionOrgUnitIds: orgIds.length ? orgIds : undefined,
            attributionGroup,
            customerId,
            q: qParam || undefined,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        }),
        [
            from,
            to,
            userIds,
            orgIds,
            attributionGroup,
            customerId,
            qParam,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        ],
    )
    const hasFilters =
        userIds.length > 0 ||
        orgIds.length > 0 ||
        qParam !== "" ||
        attributionGroup != null ||
        customerId != null

    function applySearch() {
        patchDual({
            dualQ: qDraft.trim() || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyIdDraft() {
        const ids = parseCsvIds(idDraft)
        if (ids.length === 0) return
        const merged = serializeCsvIds([...userIds, ...ids])
        setIdDraft("")
        patchDual({
            attributionUserIds: merged || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    return {
        searchParams,
        query,
        userIds,
        orgIds,
        attributionGroup,
        dimension,
        sort,
        page,
        pageSize,
        qDraft,
        setQDraft,
        idDraft,
        setIdDraft,
        hasFilters,
        applySearch,
        applyIdDraft,
    }
}
