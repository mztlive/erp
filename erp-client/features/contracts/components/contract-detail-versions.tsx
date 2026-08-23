"use client"

import {
    DocumentSection,
    surfaceInsetClassName,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { cn } from "@/lib/utils"
import { CONTRACT_AUDIT_ACTION_LABEL } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"

/** 版本与审计分区：修订时间线 + 审计时间线。 */
export function ContractDetailVersions({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <div className="grid gap-8 lg:grid-cols-2 lg:gap-10">
            <DocumentSection
                title="版本时间线"
                description="每个版本对应已上传的签署 PDF。"
            >
                {contract.revisionTimeline.length === 0 ? (
                    <p className="text-sm leading-relaxed text-muted-foreground">
                        尚无已确认修订。
                    </p>
                ) : (
                    <ol className="space-y-3" aria-label="合同修订时间线">
                        {contract.revisionTimeline.map((item) => (
                            <li
                                key={item.revisionId}
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-4 py-3",
                                )}
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="flex items-center gap-2">
                                        <span className="num font-medium">
                                            v{item.revisionNo}
                                        </span>
                                        {item.isCurrent ? (
                                            <Badge variant="info">当前</Badge>
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
                                <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">
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
                                                ：{diff.before} → {diff.after}
                                            </li>
                                        ))}
                                    </ul>
                                ) : null}
                            </li>
                        ))}
                    </ol>
                )}
            </DocumentSection>

            <DocumentSection
                title="审计时间线"
                description="PDF 上传、版本归档、终止与下载等处理动作。"
            >
                {contract.auditTimeline.length === 0 ? (
                    <p className="text-sm leading-relaxed text-muted-foreground">
                        暂无审计记录。
                    </p>
                ) : (
                    <ol className="space-y-3" aria-label="合同审计时间线">
                        {contract.auditTimeline.map((event) => (
                            <li
                                key={event.id}
                                className={cn(
                                    surfaceInsetClassName,
                                    "px-4 py-3",
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
                                <p className="mt-1.5 text-xs leading-relaxed text-muted-foreground">
                                    {event.actorLabel} · {event.summary}
                                </p>
                            </li>
                        ))}
                    </ol>
                )}
            </DocumentSection>
        </div>
    )
}
