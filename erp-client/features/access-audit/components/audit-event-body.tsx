"use client"

import { BusinessEmptyState, BusinessStatusBadge } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import { useAuditEventQuery } from "@/features/access-audit/hooks/queries"
import { formatDateTime } from "@/lib/datetime"

type AuditEventBodyProps = {
    query: ReturnType<typeof useAuditEventQuery>
}

function AuditEventBody({ query }: AuditEventBodyProps) {
    if (query.isPending) {
        return <div className="h-32 animate-pulse rounded-lg bg-muted" />
    }
    if (!query.data) {
        return (
            <BusinessEmptyState
                kind="no-data"
                title="事件不存在或无权查看"
                description="仅展示你有权查看的审计记录。"
            />
        )
    }
    return (
        <div className="flex flex-col gap-5 text-sm">
            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">谁在何时操作</h3>
                <DescriptionList columns="two" aria-label="操作者与时间">
                    <DescriptionItem>
                        <DescriptionTerm>操作者</DescriptionTerm>
                        <DescriptionDetails>
                            {query.data.actorLabel}（{query.data.actorId}）
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>责任角色</DescriptionTerm>
                        <DescriptionDetails>
                            {query.data.actorRole}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>发生时间</DescriptionTerm>
                        <DescriptionDetails className="num">
                            {formatDateTime(query.data.recordedAt, "full")}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>结果</DescriptionTerm>
                        <DescriptionDetails>
                            <BusinessStatusBadge
                                label={query.data.resultLabel}
                                tone={query.data.resultTone}
                            />
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
            </section>
            <section className="flex flex-col gap-2">
                <h3 className="text-sm font-medium">做了什么</h3>
                <DescriptionList columns="two" aria-label="动作与对象">
                    <DescriptionItem>
                        <DescriptionTerm>动作</DescriptionTerm>
                        <DescriptionDetails>
                            {query.data.actionLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>对象</DescriptionTerm>
                        <DescriptionDetails>
                            {query.data.objectLabel}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>
                <div className="rounded-lg bg-muted/50 px-3 py-2.5">
                    <div className="text-xs text-muted-foreground">
                        变更字段
                    </div>
                    <p className="mt-0.5">
                        {query.data.changedFieldDisplay !== "—"
                            ? query.data.changedFieldDisplay
                            : "无字段变更记录"}
                    </p>
                    <p className="mt-1 text-xs text-muted-foreground">
                        敏感字段不返回完整旧值或新值；安全摘要默认仅作引用。
                    </p>
                </div>
            </section>
            <details className="group rounded-lg border border-border">
                <summary className="cursor-pointer px-3 py-2 text-xs text-muted-foreground hover:text-foreground">
                    技术信息：事件号、追踪号与安全摘要
                </summary>
                <div className="border-t border-border px-3 py-3">
                    <DescriptionList columns="two" aria-label="技术信息">
                        <DescriptionItem>
                            <DescriptionTerm>审计事件号</DescriptionTerm>
                            <DescriptionDetails className="font-mono text-xs">
                                {query.data.auditEventId}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>请求追踪号</DescriptionTerm>
                            <DescriptionDetails className="font-mono text-xs">
                                {query.data.traceId}
                                <div className="text-muted-foreground">
                                    req {query.data.requestId}
                                </div>
                            </DescriptionDetails>
                        </DescriptionItem>
                        {query.data.safeDigest ? (
                            <DescriptionItem>
                                <DescriptionTerm>安全摘要</DescriptionTerm>
                                <DescriptionDetails className="font-mono text-xs">
                                    {query.data.safeDigest}
                                </DescriptionDetails>
                            </DescriptionItem>
                        ) : null}
                    </DescriptionList>
                </div>
            </details>
            <p className="text-xs text-muted-foreground">
                审计记录不可编辑或删除。打开关联对象时将重新鉴权。
            </p>
        </div>
    )
}

export { AuditEventBody }
