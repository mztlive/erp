"use client"

import { HistoryIcon } from "lucide-react"

import {
    surfaceInsetClassName,
    surfacePanelClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"
import {
    CONTRACT_AUDIT_ACTION_LABEL,
} from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"

/** 版本与审计分区：修订时间线 + 审计时间线。 */
export function ContractDetailVersions({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <div className="grid gap-4 lg:grid-cols-2">
            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <div className="flex flex-wrap items-center gap-2">
                        <HistoryIcon
                            className="size-4 text-muted-foreground"
                            aria-hidden="true"
                        />
                        <CardTitle>版本时间线</CardTitle>
                    </div>
                    <CardDescription>
                        每个版本对应已上传的签署 PDF。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    {contract.revisionTimeline.length === 0 ? (
                        <p className="text-sm text-muted-foreground">
                            尚无已确认修订。
                        </p>
                    ) : (
                        <ol
                            className="space-y-3"
                            aria-label="合同修订时间线"
                        >
                            {contract.revisionTimeline.map((item) => (
                                <li
                                    key={item.revisionId}
                                    className={cn(
                                        surfaceInsetClassName,
                                        "px-3 py-2.5",
                                    )}
                                >
                                    <div className="flex flex-wrap items-center justify-between gap-2">
                                        <div className="flex items-center gap-2">
                                            <span className="num font-medium">
                                                v{item.revisionNo}
                                            </span>
                                            {item.isCurrent ? (
                                                <Badge variant="info">
                                                    当前
                                                </Badge>
                                            ) : (
                                                <Badge variant="outline">
                                                    历史
                                                </Badge>
                                            )}
                                        </div>
                                        <span className="num text-xs text-muted-foreground">
                                            {item.effectiveAt ?? "—"}
                                        </span>
                                    </div>
                                    <p className="mt-1 text-xs text-muted-foreground">
                                        {item.validFrom} 至 {item.validTo}
                                        {item.changeReason
                                            ? ` · ${item.changeReason}`
                                            : null}
                                    </p>
                                    {item.diffSummary &&
                                    item.diffSummary.length > 0 ? (
                                        <ul className="mt-2 space-y-1 text-xs">
                                            {item.diffSummary.map((diff) => (
                                                <li key={diff.field}>
                                                    <span className="font-medium">
                                                        {diff.field}
                                                    </span>
                                                    ：{diff.before} →{" "}
                                                    {diff.after}
                                                </li>
                                            ))}
                                        </ul>
                                    ) : null}
                                </li>
                            ))}
                        </ol>
                    )}
                </CardContent>
            </Card>

            <Card size="sm" className={surfacePanelClassName}>
                <CardHeader className="border-b border-border/30">
                    <CardTitle>审计时间线</CardTitle>
                    <CardDescription>
                        PDF 上传、版本归档、终止与下载等处理动作。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    <ol className="space-y-3" aria-label="合同审计时间线">
                        {contract.auditTimeline.map((event) => (
                            <li
                                key={event.id}
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-3 py-2.5",
                                )}
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span className="text-sm font-medium">
                                        {CONTRACT_AUDIT_ACTION_LABEL[
                                            event.action
                                        ] ?? event.action}
                                    </span>
                                    <span className="num text-xs text-muted-foreground">
                                        {event.at}
                                    </span>
                                </div>
                                <p className="mt-1 text-xs text-muted-foreground">
                                    {event.actorLabel} · {event.summary}
                                </p>
                            </li>
                        ))}
                    </ol>
                </CardContent>
            </Card>
        </div>
    )
}
