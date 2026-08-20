"use client"

import { surfaceInsetClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { AUDIT_ACTION_LABEL } from "@/features/supplier-settlements/types"
import type { SettlementDetailView } from "@/features/supplier-settlements/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

function SettlementCenterAudit({
    events,
}: {
    events: SettlementDetailView["auditEvents"]
}) {
    return (
        <Card
            size="sm"
            className={cn(surfaceInsetClassName, "shadow-none ring-0")}
        >
            <CardHeader className="rounded-t-lg border-b border-grid py-3">
                <CardTitle className="text-base">审计</CardTitle>
            </CardHeader>
            <CardContent className="space-y-2 pt-4">
                {events.map((e) => (
                    <div
                        key={e.eventId}
                        className={cn(
                            surfaceInsetClassName,
                            "px-3 py-2 text-sm",
                        )}
                    >
                        <div className="flex flex-wrap gap-2">
                            <span className="font-medium">
                                {AUDIT_ACTION_LABEL[e.action] ??
                                    e.summary.split("·")[0]}
                            </span>
                            <span className="text-muted-foreground">
                                {e.actor}
                            </span>
                            {e.auditNo ? (
                                <span className="num text-xs">
                                    审计号 {e.auditNo}
                                </span>
                            ) : null}
                        </div>
                        <p className="text-muted-foreground">{e.summary}</p>
                        <p className="text-xs text-muted-foreground">
                            {formatDateTime(e.at, "default")}
                        </p>
                    </div>
                ))}
            </CardContent>
        </Card>
    )
}

export { SettlementCenterAudit }
