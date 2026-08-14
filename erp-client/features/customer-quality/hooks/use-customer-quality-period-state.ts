"use client"

import * as React from "react"
import type { PaginationState } from "@tanstack/react-table"
import { usePathname, useRouter, useSearchParams } from "next/navigation"

import type {
    BusinessTypeFilter,
    CustomerQualityQuery,
    CustomerQualityScenario,
    FundsReviewFilter,
    PeriodSelectionSource,
} from "../types"
import { useCustomerQualityPeriodPolicyQuery } from "./queries"
import type { CustomerQualityPatch } from "./use-customer-quality-navigation-state"

export function useCustomerQualityPeriodState({
    scenario,
    fromParam,
    toParam,
    periodPreset,
    fundsReview,
    businessType,
    scaleTag,
    profitTag,
    riskTag,
    qParam,
    sort,
    chartDimension,
    chartCode,
    customerId,
    scopeId,
    pagination,
    patchUrl,
}: {
    scenario?: CustomerQualityScenario
    fromParam: string | null
    toParam: string | null
    periodPreset?: string
    fundsReview: FundsReviewFilter
    businessType?: BusinessTypeFilter
    scaleTag?: string
    profitTag?: string
    riskTag?: string
    qParam: string
    sort: string
    chartDimension?: string
    chartCode?: string
    customerId?: string
    scopeId: string
    pagination: PaginationState
    patchUrl: CustomerQualityPatch
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()

    const periodPolicyQuery = useCustomerQualityPeriodPolicyQuery(scenario)
    const periodPolicy = periodPolicyQuery.data

    const [explicitFrom, setExplicitFrom] = React.useState(fromParam ?? "")
    const [explicitTo, setExplicitTo] = React.useState(toParam ?? "")
    const [periodWriteDone, setPeriodWriteDone] = React.useState(false)

    // Apply server default period into URL when missing (never silent calendar-year client fallback).
    React.useEffect(() => {
        if (periodWriteDone) return
        if (!periodPolicy) return
        if (fromParam && toParam) {
            setPeriodWriteDone(true)
            return
        }
        if (periodPolicy.hasDefault && periodPolicy.from && periodPolicy.to) {
            const next = new URLSearchParams(searchParams.toString())
            next.set("from", periodPolicy.from)
            next.set("to", periodPolicy.to)
            if (periodPolicy.customerQualityPeriodPolicyId) {
                next.set(
                    "customerQualityPeriodPolicyId",
                    periodPolicy.customerQualityPeriodPolicyId,
                )
            }
            if (periodPolicy.customerQualityPeriodPolicyVersion != null) {
                next.set(
                    "customerQualityPeriodPolicyVersion",
                    String(periodPolicy.customerQualityPeriodPolicyVersion),
                )
            }
            if (
                !next.get("periodPreset") &&
                periodPolicy.selectionSource === "SERVER_DEFAULT"
            ) {
                next.set("periodPreset", "ytd")
            }
            router.replace(`${pathname}?${next.toString()}`)
            setPeriodWriteDone(true)
        } else {
            setPeriodWriteDone(true)
        }
    }, [
        periodPolicy,
        fromParam,
        toParam,
        periodWriteDone,
        pathname,
        router,
        searchParams,
    ])

    const resolvedFrom = fromParam ?? undefined
    const resolvedTo = toParam ?? undefined
    const hasPeriod = Boolean(resolvedFrom && resolvedTo)
    const needsPeriodBlocker =
        periodWriteDone &&
        !hasPeriod &&
        periodPolicy != null &&
        !periodPolicy.hasDefault

    const periodSelectionSource: PeriodSelectionSource =
        searchParams.get("periodSelectionSource") === "EXPLICIT" ||
        (hasPeriod && !periodPolicy?.hasDefault)
            ? "EXPLICIT"
            : periodPreset
              ? "CONFIGURED_PRESET"
              : "SERVER_DEFAULT"

    const analysisQuery: CustomerQualityQuery | null = React.useMemo(() => {
        if (!resolvedFrom || !resolvedTo) return null
        return {
            from: resolvedFrom,
            to: resolvedTo,
            periodBasis:
                periodSelectionSource === "EXPLICIT"
                    ? "EXPLICIT"
                    : (periodPolicy?.periodBasis ?? "BUSINESS_DATE"),
            periodSelectionSource,
            customerQualityPeriodPolicyId:
                searchParams.get("customerQualityPeriodPolicyId") ??
                periodPolicy?.customerQualityPeriodPolicyId,
            customerQualityPeriodPolicyVersion: Number(
                searchParams.get("customerQualityPeriodPolicyVersion") ??
                    periodPolicy?.customerQualityPeriodPolicyVersion ??
                    NaN,
            )
                ? Number(
                      searchParams.get("customerQualityPeriodPolicyVersion") ??
                          periodPolicy?.customerQualityPeriodPolicyVersion,
                  )
                : undefined,
            scopeId,
            fundsReview,
            businessType,
            scaleTag,
            profitTag,
            riskTag,
            q: qParam || undefined,
            sort,
            page: pagination.pageIndex + 1,
            pageSize: pagination.pageSize,
            chartDimension,
            chartCode,
            customerId,
            scenario,
        }
    }, [
        resolvedFrom,
        resolvedTo,
        periodSelectionSource,
        periodPolicy,
        searchParams,
        scopeId,
        fundsReview,
        businessType,
        scaleTag,
        profitTag,
        riskTag,
        qParam,
        sort,
        pagination.pageIndex,
        pagination.pageSize,
        chartDimension,
        chartCode,
        customerId,
        scenario,
    ])

    const periodInvalid = Boolean(
        resolvedFrom && resolvedTo && resolvedFrom > resolvedTo,
    )

    function applyExplicitPeriod() {
        if (!explicitFrom || !explicitTo) return
        if (explicitFrom > explicitTo) return
        patchUrl({
            from: explicitFrom,
            to: explicitTo,
            periodSelectionSource: "EXPLICIT",
            periodPreset: null,
            customerQualityPeriodPolicyId: null,
            customerQualityPeriodPolicyVersion: null,
        })
    }

    function applyPreset(presetId: string, from: string, to: string) {
        patchUrl({
            from,
            to,
            periodPreset: presetId,
            periodSelectionSource: "CONFIGURED_PRESET",
            customerQualityPeriodPolicyId:
                periodPolicy?.customerQualityPeriodPolicyId ?? null,
            customerQualityPeriodPolicyVersion:
                periodPolicy?.customerQualityPeriodPolicyVersion != null
                    ? String(periodPolicy.customerQualityPeriodPolicyVersion)
                    : null,
        })
    }

    return {
        periodPolicyQuery,
        periodPolicy,
        periodWriteDone,
        explicitFrom,
        setExplicitFrom,
        explicitTo,
        setExplicitTo,
        resolvedFrom,
        resolvedTo,
        hasPeriod,
        needsPeriodBlocker,
        periodSelectionSource,
        analysisQuery,
        periodInvalid,
        applyExplicitPeriod,
        applyPreset,
    }
}
