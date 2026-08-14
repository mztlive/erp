"use client"

import Link from "next/link"

import { surfaceInsetClassName } from "@/components/business"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { ConnectionCenterView } from "@/features/supplier-api-connections/types"
import { cn } from "@/lib/utils"

export function RelatedSection({ conn }: { conn: ConnectionCenterView }) {
    return (
        <div className="grid gap-3 sm:grid-cols-2 lg:grid-cols-4">
            {[
                {
                    label: "活跃供给",
                    value: conn.relatedImpact.activeOfferings,
                    href: "/procurement/supplier-offerings",
                },
                {
                    label: "生效发布",
                    value: conn.relatedImpact.activePublications,
                    href: "/commerce/publications",
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
                <Card
                    key={item.label}
                    size="sm"
                    className={cn(surfaceInsetClassName, "shadow-none ring-0")}
                >
                    <CardHeader className="pb-1">
                        <CardDescription>{item.label}</CardDescription>
                        <CardTitle className="num text-2xl">
                            {item.value}
                        </CardTitle>
                    </CardHeader>
                    <CardContent>
                        <Link
                            href={item.href}
                            className="text-xs text-primary underline-offset-2 hover:underline"
                        >
                            打开关联页面
                        </Link>
                    </CardContent>
                </Card>
            ))}
            <p className="text-xs text-muted-foreground sm:col-span-2 lg:col-span-4">
                进入相关页面时将重新获取最新状态。
            </p>
        </div>
    )
}
