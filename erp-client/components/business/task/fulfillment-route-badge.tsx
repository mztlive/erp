"use client"

import type * as React from "react"
import {
    BoxesIcon,
    PackageCheckIcon,
    RouteIcon,
    TruckIcon,
    WarehouseIcon,
    type LucideIcon,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import {
    sourceRouteCategory,
    sourceRouteStyleForCategory,
    type SourceRouteCategory,
    type SourceRouteInput,
} from "./source-palette"

const ROUTE_ICONS: Readonly<Record<SourceRouteCategory, LucideIcon>> = {
    stock: PackageCheckIcon,
    warehouse: WarehouseIcon,
    direct_dropship: TruckIcon,
    direct_bulk: BoxesIcon,
    other: RouteIcon,
}

export interface FulfillmentRouteBadgeProps extends Omit<
    React.ComponentProps<"span">,
    "children"
> {
    /** 显式履约路径分类；传入时优先于 `option` 推导结果。 */
    route?: SourceRouteCategory
    /** 供给方案结构输入，自动映射至对应路径分类与语义色。 */
    option?: SourceRouteInput | null
    /** 覆盖默认中文标签（如「集采入仓」）；省略时按路径分类自动给出。 */
    label?: React.ReactNode
    /** 是否展示路径语义图标（默认 `true`，保证双编码可辨识度）。 */
    showIcon?: boolean
    /** 尺寸档位：`default` 为标准 12px 徽章，`compact` 为紧凑 11px 微胶囊。 */
    size?: "default" | "compact"
}

/**
 * 履约路径语义徽章（docs/task-page-design-system.md §3.3.2）：
 * - `stock`（现有库存）→ `teal`
 * - `warehouse`（采购入仓）→ `orange`
 * - `direct_dropship`（代发直发）→ `info`
 * - `direct_bulk`（集采直发）→ `violet`
 * - `other`（其他履约）→ `neutral`
 */
export function FulfillmentRouteBadge({
    route,
    option,
    label,
    showIcon = true,
    size = "default",
    className,
    ...props
}: FulfillmentRouteBadgeProps) {
    const resolvedCategory = route ?? sourceRouteCategory(option)
    const tokens = sourceRouteStyleForCategory(resolvedCategory)
    const Icon = ROUTE_ICONS[resolvedCategory]

    return (
        <Badge
            data-slot="fulfillment-route-badge"
            data-route={resolvedCategory}
            variant={tokens.variant}
            className={cn(
                tokens.badgeClass,
                size === "compact" && "h-4.5 px-1.5 text-tiny",
                className,
            )}
            {...props}
        >
            {showIcon ? <Icon aria-hidden="true" /> : null}
            {label ?? tokens.label}
        </Badge>
    )
}
