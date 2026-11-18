"use client"

import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { useSearchParams } from "next/navigation"

import type { CurrentQualityQuery } from "../dual-types"
import type { PatchDual } from "../lib/dual-filter-state"
import {
    parseCsvIds,
    parseCurrentDimension,
    parseCurrentSort,
    parseDualBoolean,
    parseDualPage,
    parseDualPageSize,
} from "../lib/dual-url-state"

export function useCurrentQualityFilters({
    from,
    to,
    patchDual,
}: {
    from: string
    to: string
    patchDual: PatchDual
}) {
    const searchParams = useSearchParams()
    const account = useAccountProfileQuery()
    const canFilterOwners = hasPermission(
        account.data?.permissions,
        "sales_person:list",
    )
    const canFilterOrgs = hasPermission(
        account.data?.permissions,
        "org_unit:list",
    )
    const ownerIds = parseCsvIds(
        canFilterOwners ? searchParams.get("ownerUserIds") : null,
    )
    const orgIds = parseCsvIds(
        canFilterOrgs ? searchParams.get("orgUnitIds") : null,
    )
    const includeDescendants = parseDualBoolean(
        canFilterOrgs ? searchParams.get("includeDescendants") : null,
    )
    const ownerGroup = searchParams.get("ownerGroup") ?? undefined
    const hasUnavailableOwners =
        !canFilterOwners && searchParams.has("ownerUserIds")
    const hasUnavailableOrgs =
        !canFilterOrgs &&
        (searchParams.has("orgUnitIds") ||
            searchParams.has("includeDescendants"))
    const resetting = hasUnavailableOwners || hasUnavailableOrgs
    React.useEffect(() => {
        if (!account.data || !resetting) return
        patchDual({
            ...(hasUnavailableOwners ? { ownerUserIds: null } : {}),
            ...(hasUnavailableOrgs
                ? { orgUnitIds: null, includeDescendants: null }
                : {}),
            dualPage: null,
            scopeVersion: null,
        })
    }, [
        account.data,
        hasUnavailableOrgs,
        hasUnavailableOwners,
        patchDual,
        resetting,
    ])
    const customerId = searchParams.get("dualCustomerId") ?? undefined
    const qParam = searchParams.get("dualQ") ?? ""
    const dimension = parseCurrentDimension(searchParams.get("dualDimension"))
    const sort = parseCurrentSort(searchParams.get("dualSort"))
    const page = resetting ? 1 : parseDualPage(searchParams.get("dualPage"))
    const pageSize = parseDualPageSize(searchParams.get("dualPageSize"))
    const scopeVersion = resetting
        ? undefined
        : (searchParams.get("scopeVersion") ?? undefined)

    const [qDraft, setQDraft] = React.useState(qParam)
    const suppressDescendantsPatch = React.useRef(false)
    React.useEffect(() => {
        if (document.activeElement?.id !== "customers-quality-dual-search") {
            setQDraft(qParam)
        }
    }, [qParam])

    const query: CurrentQualityQuery = React.useMemo(
        () => ({
            from,
            to,
            ownerUserIds: ownerIds.length ? ownerIds : undefined,
            orgUnitIds: orgIds.length ? orgIds : undefined,
            includeDescendants:
                orgIds.length > 0 ? includeDescendants : undefined,
            customerId,
            ownerGroup,
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
            ownerIds,
            orgIds,
            includeDescendants,
            customerId,
            ownerGroup,
            qParam,
            dimension,
            sort,
            scopeVersion,
            page,
            pageSize,
        ],
    )
    const hasFilters =
        ownerIds.length > 0 ||
        orgIds.length > 0 ||
        qParam !== "" ||
        ownerGroup != null ||
        customerId != null

    function applySearch() {
        patchDual({
            dualQ: qDraft.trim() || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyOwners(value: string) {
        patchDual({
            ownerUserIds: value || null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyOrgs(value: string) {
        const cleared = value.trim() === ""
        if (cleared) suppressDescendantsPatch.current = true
        patchDual({
            orgUnitIds: cleared ? null : value,
            ...(cleared ? { includeDescendants: null } : {}),
            scopeVersion: null,
            dualPage: null,
        })
    }

    function applyDescendants(checked: boolean) {
        if (suppressDescendantsPatch.current) {
            suppressDescendantsPatch.current = false
            return
        }
        patchDual({
            includeDescendants: checked ? "true" : null,
            scopeVersion: null,
            dualPage: null,
        })
    }

    return {
        canFilterOwners,
        canFilterOrgs,
        searchParams,
        query,
        ownerIds,
        orgIds,
        includeDescendants,
        ownerGroup,
        dimension,
        sort,
        page,
        pageSize,
        qDraft,
        setQDraft,
        hasFilters,
        applySearch,
        applyOwners,
        applyOrgs,
        applyDescendants,
    }
}
