"use client"

import * as React from "react"

import { DetailWorkspaceSkeleton } from "@/components/business"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { SettlementList } from "@/features/supplier-settlements/components/settlement-list"
import {
    buildSettlementsSearchParams,
    parseSettlementsSearchParams,
    type SettlementsUrlState,
} from "@/features/supplier-settlements/lib/url-state"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { hasPermission } from "@/lib/permissions"

function buildDetailHref(
    statementId: string,
    state?: Partial<
        Pick<
            SettlementsUrlState,
            | "section"
            | "diff"
            | "returnTo"
            | "workItemId"
            | "queueContextId"
            | "from"
        >
    >,
): string {
    const base = `/supplier-api/settlements/${encodeURIComponent(statementId)}`
    const params = new URLSearchParams()
    if (state?.section && state.section !== "overview") {
        params.set("section", state.section)
    }
    if (state?.diff) params.set("diff", state.diff)
    if (state?.returnTo) params.set("returnTo", state.returnTo)
    if (state?.workItemId) params.set("workItemId", state.workItemId)
    if (state?.queueContextId) {
        params.set("queueContextId", state.queueContextId)
    }
    if (state?.from) params.set("from", state.from)
    const qs = params.toString()
    return qs ? `${base}?${qs}` : base
}

export function SupplierSettlementsPage() {
    const router = useRouter()
    const searchParams = useSearchParams()

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

    const parsed = React.useMemo(
        () => parseSettlementsSearchParams(searchParams),
        [searchParams],
    )
    const urlState = React.useMemo(() => sanitize(parsed), [parsed, sanitize])

    // 兼容旧链接 `/supplier-api/settlements?statementId=X`，重定向至独立详情页签路由
    React.useEffect(() => {
        if (!parsed.statementId) return
        router.replace(buildDetailHref(parsed.statementId, parsed), {
            scroll: false,
        })
    }, [parsed, router])

    const replaceUrl = React.useCallback(
        (next: SettlementsUrlState) => {
            const listPath = "/supplier-api/settlements"
            const qs = buildSettlementsSearchParams({
                ...next,
                statementId: undefined,
            })
            router.replace(`${listPath}${qs}`, { scroll: false })
        },
        [router],
    )

    const patchUrl = React.useCallback(
        (patch: Partial<SettlementsUrlState>) => {
            if (patch.statementId) {
                router.push(buildDetailHref(patch.statementId, patch))
                return
            }
            replaceUrl(sanitize({ ...urlState, ...patch }))
        },
        [replaceUrl, router, sanitize, urlState],
    )

    React.useEffect(() => {
        if (
            account.data &&
            !parsed.statementId &&
            buildSettlementsSearchParams(parsed) !==
                buildSettlementsSearchParams(urlState)
        ) {
            replaceUrl(urlState)
        }
    }, [account.data, parsed, replaceUrl, urlState])

    if (parsed.statementId) {
        return (
            <DetailWorkspaceSkeleton
                title="供应商结算单"
                description="正在打开结算单详情…"
                summary
            />
        )
    }

    return (
        <SettlementList
            urlState={urlState}
            patchUrl={patchUrl}
            returnTo={urlState.returnTo}
            onOpen={(id) => router.push(buildDetailHref(id))}
        />
    )
}
