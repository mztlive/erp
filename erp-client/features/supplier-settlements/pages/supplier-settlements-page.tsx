"use client"

import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import * as React from "react"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { hasPermission } from "@/lib/permissions"
import { usePathname, useRouter, useSearchParams } from "@/lib/navigation"

import { SettlementCenter } from "@/features/supplier-settlements/components/settlement-center"
import { SettlementList } from "@/features/supplier-settlements/components/settlement-list"
import {
    buildSettlementsSearchParams,
    parseSettlementsSearchParams,
    type SettlementsUrlState,
} from "@/features/supplier-settlements/lib/url-state"

export function SupplierSettlementsPage() {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const pathMatch = pathname.match(/\/supplier-api\/settlements\/([^/]+)$/)
    const pathStatementId = pathMatch?.[1]

    const account = useAccountProfileQuery()
    const canUsePeople = hasPermission(
        account.data?.permissions,
        "business_person:list",
    )
    const canUseOrg = hasPermission(account.data?.permissions, "org_unit:list")
    const canFilterSupplier = hasPermission(
        account.data?.permissions,
        "supplier:list",
    )
    const sanitize = React.useCallback(
        (state: SettlementsUrlState): SettlementsUrlState => {
            if (state.statementId) return state
            const next = { ...state }
            if (!canFilterSupplier) next.supplierId = undefined
            if (!canUsePeople) {
                next.ownerUserIds = undefined
                next.operatorUserIds = undefined
                next.handlerUserIds = undefined
            }
            if (!canUseOrg) {
                next.orgUnitIds = undefined
                next.includeDescendants = false
            }
            if (
                next.supplierId !== state.supplierId ||
                next.ownerUserIds !== state.ownerUserIds ||
                next.operatorUserIds !== state.operatorUserIds ||
                next.handlerUserIds !== state.handlerUserIds ||
                next.orgUnitIds !== state.orgUnitIds ||
                next.includeDescendants !== state.includeDescendants
            ) {
                next.page = 1
                next.scopeVersion = undefined
            }
            return next
        },
        [canFilterSupplier, canUseOrg, canUsePeople],
    )
    const parsed = React.useMemo(() => {
        const state = parseSettlementsSearchParams(searchParams)
        return pathStatementId && !state.statementId
            ? { ...state, statementId: pathStatementId }
            : state
    }, [searchParams, pathStatementId])
    const urlState = React.useMemo(() => sanitize(parsed), [parsed, sanitize])

    const replaceUrl = React.useCallback(
        (next: SettlementsUrlState) => {
            if (pathStatementId && next.statementId === pathStatementId) {
                const base = `/supplier-api/settlements/${pathStatementId}`
                const params = new URLSearchParams()
                if (next.section !== "overview")
                    params.set("section", next.section)
                if (next.returnTo) params.set("returnTo", next.returnTo)
                if (next.workItemId) params.set("workItemId", next.workItemId)
                if (next.queueContextId)
                    params.set("queueContextId", next.queueContextId)
                if (next.from) params.set("from", next.from)
                const qs = params.toString()
                router.replace(qs ? `${base}?${qs}` : base, { scroll: false })
                return
            }
            const listPath = "/supplier-api/settlements"
            const qs = buildSettlementsSearchParams(next)
            router.replace(`${listPath}${qs}`, { scroll: false })
        },
        [pathStatementId, router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) => {
            replaceUrl(sanitize({ ...urlState, ...patch }))
        },
        [replaceUrl, sanitize, urlState],
    )

    React.useEffect(() => {
        if (
            account.data &&
            !urlState.statementId &&
            buildSettlementsSearchParams(parsed) !==
                buildSettlementsSearchParams(urlState)
        ) {
            replaceUrl(urlState)
        }
    }, [account.data, parsed, replaceUrl, urlState])

    if (urlState.statementId) {
        const taskReturnTo = urlState.workItemId
            ? buildWorkspaceDetailHref("tasks", urlState.workItemId)
            : undefined
        return (
            <SettlementCenter
                statementId={urlState.statementId}
                workItemId={urlState.workItemId}
                urlState={urlState}
                patchUrl={patchUrl}
                returnTo={urlState.returnTo ?? taskReturnTo}
                onBack={() =>
                    patchUrl({
                        statementId: undefined,
                        section: "overview",
                        preview: undefined,
                        workItemId: undefined,
                        queueContextId: undefined,
                        from: undefined,
                    })
                }
            />
        )
    }

    return (
        <SettlementList
            urlState={urlState}
            patchUrl={patchUrl}
            returnTo={urlState.returnTo}
            onOpen={(id) =>
                patchUrl({
                    statementId: id,
                    section: "overview",
                    preview: undefined,
                    workItemId: undefined,
                    queueContextId: undefined,
                    from: undefined,
                })
            }
        />
    )
}
