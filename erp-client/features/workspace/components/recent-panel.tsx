"use client"

import Link from "next/link"
import { ArrowRightIcon } from "lucide-react"

import { surfacePanelClassName } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { TodayWorkspaceView } from "@/features/workspace/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

export function RecentPanel({
    recent,
}: {
    recent: TodayWorkspaceView["recent"]
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="rounded-t-lg border-b border-grid">
                <CardTitle>最近打开</CardTitle>
                <CardDescription>快速回到上次处理的任务。</CardDescription>
            </CardHeader>
            <CardContent>
                {recent.length === 0 ? (
                    <p className="text-sm text-muted-foreground">
                        暂无最近记录。
                    </p>
                ) : (
                    <nav aria-label="最近打开的任务" className="space-y-1">
                        {recent.map((item) => (
                            <Button
                                key={item.id}
                                id={`workspace-recent-${toAutomationIdSegment(item.id)}`}
                                variant="ghost"
                                className="w-full justify-between"
                                render={
                                    <Link
                                        id={`workspace-recent-${toAutomationIdSegment(item.id)}`}
                                        href={item.href}
                                    />
                                }
                            >
                                <span className="truncate">{item.label}</span>
                                <ArrowRightIcon aria-hidden="true" />
                            </Button>
                        ))}
                    </nav>
                )}
            </CardContent>
        </Card>
    )
}
