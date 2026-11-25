"use client"

import {
    DeliveryAllocationOverview as DeliveryAllocationOverviewPrimitive,
    DeliveryAllocationPrintAppendix as DeliveryAllocationPrintAppendixPrimitive,
    type DeliveryAllocationOverviewProps as DeliveryAllocationOverviewPrimitiveProps,
    type DeliveryAllocationPrintAppendixProps as DeliveryAllocationPrintAppendixPrimitiveProps,
    type DeliveryFulfillmentDescriptor,
} from "@/components/business/delivery-allocation-overview"
import { RegionName } from "./region-name"

export type { DeliveryFulfillmentDescriptor }

export type DeliveryAllocationOverviewProps =
    DeliveryAllocationOverviewPrimitiveProps

export type DeliveryAllocationPrintAppendixProps =
    DeliveryAllocationPrintAppendixPrimitiveProps

const renderRegionNameDefault = (regionCode: string) => (
    <RegionName code={regionCode} />
)

/** 绑定行政区划查询的「交付与收货地址」总览，缺失 `region_name` 时自动通过 `RegionName` 解析完整省市区。 */
export function DeliveryAllocationOverview({
    renderRegionName = renderRegionNameDefault,
    ...props
}: DeliveryAllocationOverviewProps) {
    return (
        <DeliveryAllocationOverviewPrimitive
            {...props}
            renderRegionName={renderRegionName}
        />
    )
}

/** 绑定行政区划查询的纸质单据收货地址附表。 */
export function DeliveryAllocationPrintAppendix({
    renderRegionName = renderRegionNameDefault,
    ...props
}: DeliveryAllocationPrintAppendixProps) {
    return (
        <DeliveryAllocationPrintAppendixPrimitive
            {...props}
            renderRegionName={renderRegionName}
        />
    )
}
