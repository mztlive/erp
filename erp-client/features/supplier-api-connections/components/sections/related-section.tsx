"use client"

import Link from "next/link"

import { MetricItem, MetricStrip } from "@/components/business"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function RelatedSection({ conn }: { conn: ConnectionCenterView }) {
    return (
        <MetricStrip variant="plain" columns={3} aria-label="关联业务统计">
            {[
                {
                    label: "活跃供给",
                    value: conn.relatedImpact.activeOfferings,
                    href: "/procurement/supplier-offerings",
                },
                {
                    label: "待处理订单",
                    value: conn.relatedImpact.openSupplierOrders,
                    href: "/supplier-api/orders",
                },
                {
                    label: "同步任务",
                    value: conn.relatedImpact.activeSyncJobs,
                    href: "/procurement/supplier-offerings",
                },
            ].map((item) => (
                <div key={item.label} className="min-w-0 space-y-1">
                    <MetricItem label={item.label} value={item.value} />
                    <Link
                        id={`supplier-api-connections-related-${toAutomationIdSegment(item.label)}`}
                        href={item.href}
                        className="text-xs text-primary underline-offset-2 hover:underline"
                    >
                        打开关联页面
                    </Link>
                </div>
            ))}
            <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-3">
                进入相关页面时将重新获取最新状态。
            </p>
        </MetricStrip>
    )
}
