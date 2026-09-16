"use client"

import { ShieldAlertIcon } from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { scopeText } from "@/lib/ui-text"
import { cn } from "@/lib/utils"

type FundsScopeBannerProps = {
    scopeSummary: string | undefined
    asOf: string | undefined
    permissionLimited: boolean
    unassigned?: string | null
    className?: string
}

/** 范围行列表共用的授权摘要条：范围口径、时点与部分受限提示。 */
export function FundsScopeBanner({
    scopeSummary,
    asOf,
    permissionLimited,
    unassigned,
    className,
}: FundsScopeBannerProps) {
    if (!scopeSummary && !permissionLimited) return null
    return (
        <Alert
            variant={permissionLimited ? "warning" : "info"}
            className={cn("min-w-0", className)}
        >
            <ShieldAlertIcon aria-hidden="true" />
            <AlertTitle>
                {permissionLimited ? "仅显示获授权份额" : "按数据范围查询"}
            </AlertTitle>
            <AlertDescription className="min-w-0 break-words">
                {scopeSummary ? <span>{scopeSummary}</span> : null}
                {permissionLimited ? (
                    <span className="mt-1 block">
                        {scopeText.limitedOnlyVisibleShare}。
                        {unassigned != null && unassigned !== "" ? (
                            <>
                                {scopeText.unassignedShare} {unassigned}。
                            </>
                        ) : null}
                    </span>
                ) : null}
                {asOf ? (
                    <span className="mt-1 block text-xs opacity-80">
                        查询时点 <span className="num">{asOf}</span>
                    </span>
                ) : null}
            </AlertDescription>
        </Alert>
    )
}

type LimitedBadgeProps = {
    id: string
    className?: string
}

/** 部分受限行的统一标记；金额统一用 MoneyValue 空值展示，不写零。 */
export function LimitedBadge({ id, className }: LimitedBadgeProps) {
    return (
        <Badge
            id={id}
            variant="warning"
            title={scopeText.limitedOnlyVisibleShare}
            className={cn("shrink-0", className)}
        >
            部分受限
        </Badge>
    )
}
