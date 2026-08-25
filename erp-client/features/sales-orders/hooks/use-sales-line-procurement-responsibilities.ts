"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import { resolveSalesLineProcurementResponsibilities } from "@/features/sales-orders/api/procurement-responsibility"
import type {
    SalesLineProcurementResponsibility,
    SalesOrderDraftLineInput,
    SalesOrderNature,
} from "@/features/sales-orders/types"

export const salesLineProcurementResponsibilityKeys = {
    all: ["sales-line-procurement-responsibility"] as const,
    resolve: (lines: readonly SalesOrderDraftLineInput[]) =>
        [
            ...salesLineProcurementResponsibilityKeys.all,
            lines.map((line) => ({
                rowKey: line.rowKey,
                sku: line.sku,
                skuRevisionId: line.skuRevisionId,
                serviceRegion: (line.serviceRegion ?? "").trim(),
            })),
        ] as const,
}

export function useSalesLineProcurementResponsibilities(input: {
    nature: SalesOrderNature
    lines: readonly SalesOrderDraftLineInput[]
}) {
    const requestLines = React.useMemo(
        () => input.lines.filter((line) => Boolean(line.sku.trim())),
        [input.lines],
    )
    const query = useQuery({
        queryKey: salesLineProcurementResponsibilityKeys.resolve(requestLines),
        queryFn: () =>
            resolveSalesLineProcurementResponsibilities(requestLines),
        enabled: input.nature === "physical_service" && requestLines.length > 0,
    })
    const byRowKey = React.useMemo(() => {
        return new Map<string, SalesLineProcurementResponsibility>(
            (query.data ?? []).map((line) => [line.rowKey, line]),
        )
    }, [query.data])
    const allResolved =
        input.nature !== "physical_service" ||
        (input.lines.length > 0 &&
            !query.isFetching &&
            !query.error &&
            input.lines.every((line) => {
                const responsibility = byRowKey.get(line.rowKey)
                return Boolean(
                    line.sku.trim() &&
                    responsibility?.resolved &&
                    responsibility.ownerUserId &&
                    responsibility.ownerName,
                )
            }))

    return { ...query, byRowKey, allResolved }
}
