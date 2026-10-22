"use client"

import type { Ref } from "react"
import { Badge } from "@/components/ui/badge"
import { useRegionIndex } from "@/features/regions/hooks/queries"
import { regionCityCode } from "@/features/regions/lib/region-index"
import { getErrorMessage } from "@/lib/api/errors"
import type { InquiryLine } from "../types"

export function InquiryDeliveryBadge({
    line,
    id,
    onOpen,
    ref,
}: {
    line: InquiryLine
    id: string
    onOpen: () => void
    ref: Ref<HTMLButtonElement>
}) {
    const regions = useRegionIndex()
    const cityCodes = line.allocations.map((allocation) =>
        regions.data
            ? regionCityCode(regions.data, allocation.region_code)
            : null,
    )
    const cityLabel =
        line.allocations.length === 0
            ? "0 个城市"
            : regions.isError
              ? "城市统计失败"
              : regions.isPending
                ? "城市统计中"
                : cityCodes.some((code) => code === null)
                  ? "城市待确认"
                  : `${new Set(cityCodes).size} 个城市`
    const label = `${line.allocations.length} 个地址，${cityLabel}`
    return (
        <Badge
            id={id}
            variant="outline"
            className="h-auto min-h-5 max-w-full cursor-pointer whitespace-normal hover:bg-muted"
            render={
                <button
                    ref={ref}
                    type="button"
                    aria-label={`${line.product_name}配送地址：${label}`}
                />
            }
            aria-haspopup="dialog"
            title={
                regions.isError && line.allocations.length > 0
                    ? `${getErrorMessage(regions.error)}；点击重试并查看配送地址`
                    : "查看配送地址"
            }
            onClick={() => {
                if (regions.isError) void regions.refetch()
                onOpen()
            }}
        >
            {label}
        </Badge>
    )
}
