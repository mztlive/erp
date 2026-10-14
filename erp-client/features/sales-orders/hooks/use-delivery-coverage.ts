"use client"

import * as React from "react"
import { useQuery } from "@tanstack/react-query"

import {
    deliveryCoverageKey,
    fetchDeliveryCoverage,
    type DeliveryCoverageHint,
    type DeliveryCoverageLine,
} from "../api/delivery-coverage"
import { salesOrderKeys } from "./queries"

type CoverageSource = Readonly<{
    sku: string
    deliveryAllocations?: readonly { region_code: string }[]
}>

/** 按商品与已选收货地区查询配送提示；未选地区或未选商品时不请求。 */
export function useDeliveryCoverage(lines: readonly CoverageSource[]) {
    const request = React.useMemo<DeliveryCoverageLine[]>(
        () =>
            lines
                .map((line) => ({
                    skuId: line.sku.trim(),
                    regionCodes: [
                        ...new Set(
                            (line.deliveryAllocations ?? [])
                                .map((row) => row.region_code)
                                .filter(Boolean),
                        ),
                    ].sort(),
                }))
                .filter((line) => line.skuId && line.regionCodes.length),
        [lines],
    )
    const query = useQuery({
        queryKey: [...salesOrderKeys.all, "delivery-coverage", request],
        queryFn: () => fetchDeliveryCoverage(request),
        enabled: request.length > 0,
    })
    return React.useCallback(
        (sku: string, regionCode: string): DeliveryCoverageHint =>
            query.data?.get(deliveryCoverageKey(sku.trim(), regionCode)) ??
            "NONE",
        [query.data],
    )
}
