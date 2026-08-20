"use client"

import { surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import type { MallSyncPageView } from "@/features/mall-sync/types"
import { formatDateTime } from "@/lib/datetime"

type MallSyncHistoryViewProps = {
    data: MallSyncPageView | undefined
    sealed: boolean
}

export function MallSyncHistoryView({
    data,
    sealed,
}: MallSyncHistoryViewProps) {
    return (
        <div className="space-y-3">
            {sealed ? (
                <Alert>
                    <AlertTitle>历史只读</AlertTitle>
                    <AlertDescription>
                        第一期同步已完成归档。请前往执行信息与对账工作区查看后续内容。
                    </AlertDescription>
                </Alert>
            ) : null}
            {(data?.history ?? []).map((h) => (
                <Card key={h.id} size="sm" className={surfacePanelClassName}>
                    <CardHeader className="border-b border-grid">
                        <CardTitle className="text-base">{h.title}</CardTitle>
                        <CardDescription>
                            {formatDateTime(h.recordedAt, "default")}
                            {h.watermark
                                ? ` · ${formatDateTime(h.watermark, "default")}`
                                : ""}
                            {h.reference ? ` · ${h.reference}` : ""}
                        </CardDescription>
                    </CardHeader>
                    <CardContent className="text-sm text-muted-foreground">
                        {h.summary}
                    </CardContent>
                </Card>
            ))}
        </div>
    )
}
