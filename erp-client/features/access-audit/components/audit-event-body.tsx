"use client"

import {
    BusinessEmptyState,
    BusinessStatusBadge,
} from "@/components/business"
import { Separator } from "@/components/ui/separator"
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
        <div className="flex flex-col gap-3 text-sm">
            <dl className="grid gap-2 sm:grid-cols-2">
                <div>
                    <dt className="text-xs text-muted-foreground">
                        审计事件号
                    </dt>
                    <dd className="font-mono">
                        {query.data.auditEventId}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        发生时间
                    </dt>
                    <dd className="num">
                        {formatDateTime(query.data.recordedAt, "full")}
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">操作者</dt>
                    <dd>
                        {query.data.actorLabel}（{query.data.actorId}）
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        责任角色
                    </dt>
                    <dd>{query.data.actorRole}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">动作</dt>
                    <dd>{query.data.actionLabel}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">结果</dt>
                    <dd>
                        <BusinessStatusBadge
                            label={query.data.resultLabel}
                            tone={query.data.resultTone}
                        />
                    </dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">对象</dt>
                    <dd>{query.data.objectLabel}</dd>
                </div>
                <div>
                    <dt className="text-xs text-muted-foreground">
                        请求追踪号
                    </dt>
                    <dd className="font-mono text-xs">
                        {query.data.traceId}
                        <div className="text-muted-foreground">
                            req {query.data.requestId}
                        </div>
                    </dd>
                </div>
            </dl>
            <Separator />
            <div>
                <h3 className="text-sm font-semibold">变更字段</h3>
                <p className="mt-1 text-muted-foreground">
                    {query.data.changedFieldDisplay !== "—"
                        ? query.data.changedFieldDisplay
                        : "无字段变更记录"}
                </p>
                <p className="mt-2 text-xs text-muted-foreground">
                    敏感字段不返回完整旧值或新值；安全摘要默认仅作引用。
                </p>
                {query.data.safeDigest ? (
                    <p className="mt-1 font-mono text-xs">
                        安全摘要 {query.data.safeDigest}
                    </p>
                ) : null}
            </div>
            <p className="text-xs text-muted-foreground">
                审计记录不可编辑或删除。打开关联对象时将重新鉴权。
            </p>
        </div>
    )
}

export { AuditEventBody }
