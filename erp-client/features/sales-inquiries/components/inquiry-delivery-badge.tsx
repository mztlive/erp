"use client"

import type { Ref } from "react"
import { MapPinIcon } from "lucide-react"
import { Button } from "@/components/ui/button"
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
            ? "0 城市"
            : regions.isError
              ? "城市统计失败"
              : regions.isPending
                ? "城市统计中"
                : cityCodes.some((code) => code === null)
                  ? "城市待确认"
                  : `${new Set(cityCodes).size} 城市`
    const label = `${line.allocations.length} 地址 · ${cityLabel}`
    return (
        <Button
            ref={ref}
            id={id}
            type="button"
            variant="ghost"
            size="xs"
            className="h-auto min-h-6 max-w-full gap-1 px-1 py-0.5 text-right font-normal whitespace-normal text-muted-foreground hover:text-primary"
            aria-label={`${line.product_name}配送地址：${label}`}
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
            <MapPinIcon className="size-3" aria-hidden="true" />
            <span>{label}</span>
        </Button>
    )
}
