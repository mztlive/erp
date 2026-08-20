import { AuditTimeline, surfacePanelClassName } from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { formatDateTime } from "@/lib/datetime"
import { INTEGRATION_ACTION_LABEL } from "../lib/presentation"
import type { IntegrationResolutionItemView } from "../types"
import { EVIDENCE_KIND_LABEL, REVIEWER_SEPARATION_LABEL } from "../types"

export function IntegrationEvidencePanel({
    item,
}: {
    item: IntegrationResolutionItemView
}) {
    return (
        <Card size="sm" className={surfacePanelClassName}>
            <CardHeader className="border-b border-grid">
                <CardTitle>证据与尝试（历史保留）</CardTitle>
                <CardDescription>
                    消息、尝试与处理记录只保留，不提供覆盖控件
                </CardDescription>
            </CardHeader>
            <CardContent className="space-y-4 pt-4">
                {item.attempts.length > 0 ? (
                    <div className="space-y-2">
                        <h4 className="text-sm font-medium">尝试历史</h4>
                        <ul className="space-y-2">
                            {item.attempts.map((attempt) => (
                                <li
                                    key={`${attempt.attemptNumber}-${attempt.attemptedAt}`}
                                    className="rounded-lg border bg-muted/30 px-3 py-2 text-sm"
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

                <div>
                    <h4 className="mb-2 text-sm font-medium">证据时间线</h4>
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
                        emptyMessage="暂无证据记录"
                    />
                </div>

                <div>
                    <h4 className="mb-2 text-sm font-medium">处理审计</h4>
                    <AuditTimeline
                        entries={item.auditTrail.map((entry) => ({
                            id: entry.id,
                            action:
                                INTEGRATION_ACTION_LABEL[entry.action] ??
                                entry.action,
                            operator: entry.actor,
                            occurredAt: entry.at,
                            occurredAtLabel: formatDateTime(
                                entry.at,
                                "default",
                            ),
                            source: "处理",
                            note: entry.detail,
                        }))}
                        emptyMessage="暂无审计记录"
                    />
                </div>

                {item.linkedEvidence.length > 0 ? (
                    <div className="space-y-1">
                        <h4 className="text-sm font-medium">已关联核验记录</h4>
                        <ul className="text-sm">
                            {item.linkedEvidence.map((evidence) => (
                                <li key={evidence.recordId}>
                                    {EVIDENCE_KIND_LABEL[evidence.kind]} ·{" "}
                                    {evidence.label}
                                </li>
                            ))}
                        </ul>
                    </div>
                ) : null}

                {item.resolutionEvidencePolicy ? (
                    <Alert variant="info">
                        <AlertTitle>解决证据策略</AlertTitle>
                        <AlertDescription>
                            需要{" "}
                            {item.resolutionEvidencePolicy.requiredEvidenceKinds
                                .map((kind) => EVIDENCE_KIND_LABEL[kind])
                                .join("、")}
                            {" · 岗位分离 "}
                            {REVIEWER_SEPARATION_LABEL[
                                item.resolutionEvidencePolicy.reviewerSeparation
                            ] ??
                                item.resolutionEvidencePolicy
                                    .reviewerSeparation}
                        </AlertDescription>
                    </Alert>
                ) : item.hasWorkItem ? (
                    <Alert variant="warning">
                        <AlertTitle>解决证据规则尚未配置</AlertTitle>
                        <AlertDescription>
                            处理完成已从可操作范围排除；仅展示服务端当前开放的非终结动作与责任动作。
                        </AlertDescription>
                    </Alert>
                ) : null}
            </CardContent>
        </Card>
    )
}
