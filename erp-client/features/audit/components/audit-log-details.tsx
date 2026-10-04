"use client"

import { BusinessStatusBadge } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    auditActionLabel,
    auditActorLabel,
    auditChangesLabel,
    auditObjectLabel,
    auditResultView,
    auditTimeLabel,
    auditValueLabel,
} from "../lib/display"
import type { AuditLogItem } from "../types"

export function AuditLogDetails({ row }: { row: AuditLogItem }) {
    const event = row.structured_event
    const result = auditResultView(row)
    const technicalFields = [
        ["审计记录号", row.id],
        ["动作代码", event?.action_code ?? row.action],
        ["动作版本", event ? String(event.action_version) : "未记录"],
        ["事件格式版本", event ? String(event.schema_version) : "未记录"],
        ["操作人编号", event?.actor_id ?? row.actor_id],
        ["对象类型代码", event?.resource_type ?? row.resource_type],
        ["对象内部编号", event?.resource_id ?? row.resource_id],
        ["命令关联号", event?.command_id],
        ["请求关联号", event?.request_id],
    ]

    return (
        <div className="flex flex-col gap-5 text-sm">
            <DescriptionList columns="two" aria-label="业务操作记录">
                <DescriptionItem>
                    <DescriptionTerm>操作人</DescriptionTerm>
                    <DescriptionDetails>
                        {auditActorLabel(row)}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>操作账号</DescriptionTerm>
                    <DescriptionDetails>
                        {event?.actor_account ?? row.actor_account}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>发生时间</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {auditTimeLabel(row)}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>执行结果</DescriptionTerm>
                    <DescriptionDetails>
                        <BusinessStatusBadge
                            label={result.label}
                            tone={result.tone}
                        />
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>业务动作</DescriptionTerm>
                    <DescriptionDetails>
                        {auditActionLabel(row)}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>业务编号</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {auditObjectLabel(row)}
                    </DescriptionDetails>
                </DescriptionItem>
            </DescriptionList>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">业务结果</h3>
                {event && event.facts.length > 0 ? (
                    <DescriptionList columns="two" aria-label="业务结果">
                        {event.facts.map((fact) => (
                            <DescriptionItem key={fact.field}>
                                <DescriptionTerm>
                                    {fact.field_label}
                                </DescriptionTerm>
                                <DescriptionDetails>
                                    {auditValueLabel(fact.value)}
                                </DescriptionDetails>
                            </DescriptionItem>
                        ))}
                    </DescriptionList>
                ) : (
                    <p className="text-muted-foreground">业务结果未记录</p>
                )}
            </section>

            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">字段变化</h3>
                {event && event.field_changes.length > 0 ? (
                    <DescriptionList columns="one" aria-label="字段变化">
                        {event.field_changes.map((change) => (
                            <DescriptionItem key={change.field}>
                                <DescriptionTerm>
                                    {change.field_label}
                                </DescriptionTerm>
                                <DescriptionDetails>
                                    {change.before.kind === "changed" ||
                                    change.after.kind === "changed"
                                        ? "已变更"
                                        : `${auditValueLabel(change.before)} → ${auditValueLabel(change.after)}`}
                                </DescriptionDetails>
                            </DescriptionItem>
                        ))}
                    </DescriptionList>
                ) : (
                    <p className="text-muted-foreground">
                        {auditChangesLabel(row)}
                    </p>
                )}
                <p className="text-xs text-muted-foreground">
                    仅展示允许记录的业务字段。敏感内容仅标记已变更。
                </p>
            </section>

            <details className="rounded-lg border border-border">
                <summary
                    id="business-audit-details-technical-trigger"
                    className="cursor-pointer px-3 py-2 text-xs text-muted-foreground hover:text-foreground"
                >
                    技术信息与关联编号
                </summary>
                <div className="border-t border-border px-3 py-3">
                    <DescriptionList columns="two" aria-label="技术信息">
                        {technicalFields.map(([label, value]) => (
                            <DescriptionItem key={label}>
                                <DescriptionTerm>{label}</DescriptionTerm>
                                <DescriptionDetails className="break-all font-mono text-xs">
                                    {value || "未记录"}
                                </DescriptionDetails>
                            </DescriptionItem>
                        ))}
                    </DescriptionList>
                </div>
            </details>
            <p className="text-xs text-muted-foreground">
                人物名称与业务编号使用操作发生时保存的记录。历史缺失信息不补用当前资料。
            </p>
        </div>
    )
}
