import { AuditTimeline } from "@/components/business"
import { ChevronDownIcon, CheckCircle2Icon } from "lucide-react"
import { formatDateTime } from "@/lib/datetime"
import { INTEGRATION_ACTION_LABEL } from "../lib/presentation"
import type { IntegrationResolutionItemView } from "../types"
import { EVIDENCE_KIND_LABEL, REVIEWER_SEPARATION_LABEL } from "../types"

export function IntegrationEvidencePanel({
    item,
    flat,
}: {
    item: IntegrationResolutionItemView
    flat?: boolean
}) {
    return (
        <details className={flat ? "group" : "group border-t border-grid pt-5"}>
            <summary
                id="integration-evidence-history-toggle"
                className="flex cursor-pointer list-none items-center justify-between gap-3 rounded-sm outline-none focus-visible:ring-2 focus-visible:ring-ring"
            >
                <h3
                    className={
                        flat
                            ? "text-section-title font-semibold tracking-tight"
                            : "font-semibold"
                    }
                >
                    执行与处理记录
                </h3>
                <span className="flex items-center gap-3 text-xs text-muted-foreground">
                    {item.auditTrail.length} 条处理记录 ·{" "}
                    {item.linkedEvidence.length} 项核验
                    <ChevronDownIcon
                        className="size-4 transition-transform group-open:rotate-180"
                        aria-hidden
                    />
                </span>
            </summary>
            <div className="space-y-5 pt-5">
                {item.attempts.length > 0 ? (
                    <div className="space-y-2">
                        <h4 className="text-body-compact font-medium">
                            最近执行尝试
                        </h4>
                        <ul className="space-y-2">
                            {item.attempts.map((attempt) => (
                                <li
                                    key={`${attempt.attemptNumber}-${attempt.attemptedAt}`}
                                    className="border-l-2 border-border px-4 py-2 text-body-compact"
                                >
                                    <div className="font-medium">
                                        第 {attempt.attemptNumber} 次 ·{" "}
                                        {attempt.result}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        {formatDateTime(
                                            attempt.attemptedAt,
                                            "default",
                                        )}
                                        {attempt.requestSummary
                                            ? ` · 请求 ${attempt.requestSummary}`
                                            : ""}
                                        {attempt.responseSummary
                                            ? ` · 响应 ${attempt.responseSummary}`
                                            : ""}
                                    </div>
                                </li>
                            ))}
                        </ul>
                    </div>
                ) : null}

                {item.evidenceTimeline.length > 0 ? (
                    <div>
                        <h4 className="mb-2 text-body-compact font-medium">
                            证据时间线
                        </h4>
                        <AuditTimeline
                            entries={item.evidenceTimeline.map((entry) => ({
                                id: entry.id,
                                action: entry.action,
                                operator: entry.actor,
                                occurredAt: entry.at,
                                occurredAtLabel: formatDateTime(
                                    entry.at,
                                    "default",
                                ),
                                source: "证据",
                                note: entry.detail,
                            }))}
                            emptyMessage="尚未提供带时间的核验记录；已关联的核验结果见下方。"
                        />
                    </div>
                ) : null}

                <div>
                    <h4 className="mb-2 text-body-compact font-medium">
                        处理审计
                    </h4>
                    <AuditTimeline
                        entries={item.auditTrail.map((entry) => ({
                            id: entry.id,
                            action:
                                INTEGRATION_ACTION_LABEL[
                                    entry.action.toUpperCase()
                                ] ?? entry.action,
                            operator: entry.actor,
                            occurredAt: entry.at,
                            occurredAtLabel: formatDateTime(
                                entry.at,
                                "default",
                            ),
                            source: "处理",
                            note: entry.detail,
                        }))}
                        emptyMessage="暂无可展示的处理记录"
                    />
                </div>

                {item.linkedEvidence.length > 0 ? (
                    <div className="space-y-1">
                        <h4 className="text-body-compact font-medium">
                            已关联核验记录
                        </h4>
                        <ul className="text-body-compact">
                            {item.linkedEvidence.map((evidence) => (
                                <li
                                    key={evidence.recordId}
                                    className="flex items-start gap-2 py-1"
                                >
                                    <CheckCircle2Icon
                                        className="mt-0.5 size-4 shrink-0 text-muted-foreground"
                                        aria-hidden
                                    />
                                    <span>
                                        {evidence.label}
                                        <span className="ml-2 text-xs text-muted-foreground">
                                            {EVIDENCE_KIND_LABEL[evidence.kind]}
                                        </span>
                                    </span>
                                </li>
                            ))}
                        </ul>
                    </div>
                ) : null}

                {item.resolutionEvidencePolicy ? (
                    <p className="border-t border-grid pt-3 text-xs leading-relaxed text-muted-foreground">
                        完成依据：
                        {item.resolutionEvidencePolicy.requiredEvidenceKinds
                            .map((kind) => EVIDENCE_KIND_LABEL[kind])
                            .join("、")}
                        {" · 复核要求："}
                        {
                            REVIEWER_SEPARATION_LABEL[
                                item.resolutionEvidencePolicy.reviewerSeparation
                            ]
                        }
                    </p>
                ) : item.hasWorkItem && item.workItem?.status === "OPEN" ? (
                    <p className="text-xs text-muted-foreground">
                        完成条件暂未就绪，请先处理当前阻断事项。
                    </p>
                ) : null}
            </div>
        </details>
    )
}
