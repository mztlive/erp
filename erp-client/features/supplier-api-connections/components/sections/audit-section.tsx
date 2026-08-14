"use client"

import * as React from "react"
import Link from "next/link"

import {
    BusinessEmptyState,
    surfaceInsetClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { AUDIT_ACTION_LABEL } from "@/features/supplier-api-connections/types"
import { formatDateTime } from "@/lib/datetime"
import { cn } from "@/lib/utils"

export function AuditSection({ conn }: { conn: ConnectionCenterView }) {
    const [expanded, setExpanded] = React.useState(false)
    const events = expanded ? conn.auditEvents : conn.auditEvents.slice(0, 10)
    return (
        <div className="space-y-3">
            <p className="text-sm text-muted-foreground">
                配置变更与业务确认均保留审计记录 ·{" "}
                <Link
                    href={`/system/access-audit?objectId=${conn.connectionId}`}
                    className="text-primary underline-offset-2 hover:underline"
                >
                    打开权限与审计
                </Link>
            </p>
            <ul className="space-y-2">
                {events.map((e) => (
                    <li
                        key={e.eventId}
                        className={cn(
                            surfaceInsetClassName,
                            "px-3 py-2 text-sm",
                        )}
                    >
                        <div className="flex flex-wrap items-center justify-between gap-2">
                            <span className="font-medium">
                                {AUDIT_ACTION_LABEL[e.action] ??
                                    e.summary.split("·")[0]}
                            </span>
                            <span className="text-xs text-muted-foreground">
                                {formatDateTime(e.at, "default")}
                            </span>
                        </div>
                        <p className="text-muted-foreground">{e.summary}</p>
                        <p className="text-xs text-muted-foreground">
                            {e.actor}
                            {e.auditNo ? ` · 审计号 ${e.auditNo}` : ""}
                        </p>
                    </li>
                ))}
                {conn.auditEvents.length === 0 ? (
                    <BusinessEmptyState
                        kind="no-data"
                        className="rounded-lg border-0 bg-transparent p-6 shadow-none ring-0"
                        title="暂无审计事件"
                        description="配置与确认动作会追加审计记录。"
                    />
                ) : null}
            </ul>
            {conn.auditEvents.length > 10 ? (
                <Button
                    type="button"
                    size="sm"
                    variant="ghost"
                    className="text-muted-foreground hover:text-foreground"
                    onClick={() => setExpanded((v) => !v)}
                >
                    {expanded
                        ? "收起"
                        : `查看更多（共 ${conn.auditEvents.length} 条）`}
                </Button>
            ) : null}
        </div>
    )
}
