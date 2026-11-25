"use client"

import { DetailWorkspaceSection } from "@/components/business"
import { StatusBadge } from "@/components/ui/status-badge"
import { CONTRACT_AUDIT_ACTION_LABEL } from "@/features/contracts/types"
import type { ContractCenterView } from "@/features/contracts/types"

/** 版本与审计分区：修订时间线 + 审计时间线。 */
export function ContractDetailVersions({
    contract,
}: {
    contract: ContractCenterView
}) {
    return (
        <>
            <DetailWorkspaceSection width="form" title="版本时间线">
                {contract.revisionTimeline.length === 0 ? (
                    <p className="text-body-compact text-muted-foreground">
                        尚无已确认修订。
                    </p>
                ) : (
                    <ol
                        className="divide-y divide-border/60"
                        aria-label="合同修订时间线"
                    >
                        {contract.revisionTimeline.map((item) => (
                            <li
                                key={item.revisionId}
                                className="py-3 first:pt-0"
                            >
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <div className="flex items-center gap-2">
                                        <span className="num font-medium">
                                            v{item.revisionNo}
                                        </span>
                                        {item.isCurrent ? (
                                            <StatusBadge
                                                tone="info"
                                                label="当前"
                                            />
                                        ) : (
                                            <StatusBadge
                                                tone="neutral"
                                                label="历史"
                                            />
                                        )}
                                    </div>
                                    <span className="num text-xs text-muted-foreground">
                                        {item.effectiveAt ?? "—"}
                                    </span>
                                </div>
                                <p className="mt-1.5 text-xs text-muted-foreground">
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
            </DetailWorkspaceSection>

            <DetailWorkspaceSection width="form" title="审计时间线">
                {contract.auditTimeline.length === 0 ? (
                    <p className="text-body-compact text-muted-foreground">
                        暂无审计记录。
                    </p>
                ) : (
                    <ol
                        className="divide-y divide-border/60"
                        aria-label="合同审计时间线"
                    >
                        {contract.auditTimeline.map((event) => (
                            <li key={event.id} className="py-3 first:pt-0">
                                <div className="flex flex-wrap items-center justify-between gap-2">
                                    <span className="text-body-compact font-medium">
                                        {CONTRACT_AUDIT_ACTION_LABEL[
                                            event.action
                                        ] ?? event.action}
                                    </span>
                                    <span className="num text-xs text-muted-foreground">
                                        {event.at}
                                    </span>
                                </div>
                                <p className="mt-1.5 text-xs text-muted-foreground">
                                    {event.actorLabel} · {event.summary}
                                </p>
                            </li>
                        ))}
                    </ol>
                )}
            </DetailWorkspaceSection>
        </>
    )
}
