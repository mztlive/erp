"use client"

import { BusinessEmptyState, BusinessStatusBadge } from "@/components/business"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
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
        <div className="flex flex-col gap-4 text-sm">
            <DescriptionList columns="two" aria-label="审计事件身份">
                <DescriptionItem>
                    <DescriptionTerm>审计事件号</DescriptionTerm>
                    <DescriptionDetails className="font-mono">
                        {query.data.auditEventId}
                    </DescriptionDetails>
                </DescriptionItem>
                <DescriptionItem>
                    <DescriptionTerm>发生时间</DescriptionTerm>
                    <DescriptionDetails className="num">
                        {formatDateTime(query.data.recordedAt, "full")}
                    </DescriptionDetails>
                </DescriptionItem>
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
                    <DescriptionTerm>动作</DescriptionTerm>
                    <DescriptionDetails>
                        {query.data.actionLabel}
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
                <DescriptionItem>
                    <DescriptionTerm>对象</DescriptionTerm>
                    <DescriptionDetails>
                        {query.data.objectLabel}
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
            </DescriptionList>
            <Separator />
            <div className="flex flex-col gap-1.5">
                <h3 className="text-sm font-medium">变更字段</h3>
                <p className="text-muted-foreground">
                    {query.data.changedFieldDisplay !== "—"
                        ? query.data.changedFieldDisplay
                        : "无字段变更记录"}
                </p>
                <p className="text-xs text-muted-foreground">
                    敏感字段不返回完整旧值或新值；安全摘要默认仅作引用。
                </p>
                {query.data.safeDigest ? (
                    <p className="font-mono text-xs">
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
