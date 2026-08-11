"use client"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    QuickPreviewSheet,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Separator } from "@/components/ui/separator"
import {
    useAuditEventQuery,
    useEffectiveAccessQuery,
} from "@/features/access-audit/queries"
import { formatDateTime } from "@/lib/datetime"

type AccessPreviewSheetsProps = {
    explainSubject: { type: "ROLE" | "USER"; id: string } | null
    eventOpenId: string | null
    effectiveQuery: ReturnType<typeof useEffectiveAccessQuery>
    eventQuery: ReturnType<typeof useAuditEventQuery>
    closeExplain: () => void
    closeEvent: () => void
    restoreRowFocus: () => void
}

function AccessPreviewSheets({
    explainSubject,
    eventOpenId,
    effectiveQuery,
    eventQuery,
    closeExplain,
    closeEvent,
    restoreRowFocus,
}: AccessPreviewSheetsProps) {
    return (
        <>
            {/* 有效权限解释 Sheet — 服务端投影，前端不合并 */}
            <QuickPreviewSheet
                open={Boolean(explainSubject)}
                onOpenChange={(open) => {
                    if (!open) closeExplain()
                }}
                size="detail"
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                title="有效权限解释"
                description="此处展示的权限结果为系统统一计算，可能与页面其它位置显示略有差异。"
            >
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
                    {effectiveQuery.isPending ? (
                        <div className="h-40 animate-pulse rounded-lg bg-muted" />
                    ) : effectiveQuery.isError ? (
                        <BusinessFailureState
                            error={effectiveQuery.error}
                            action={
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={() =>
                                        void effectiveQuery.refetch()
                                    }
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : !effectiveQuery.data ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="主体不存在或无权解释"
                            description="仅解释当前用户有权管理的主体。"
                        />
                    ) : (
                        <div className="flex flex-col gap-4">
                            <div className="flex flex-wrap items-center gap-2">
                                <Badge variant="secondary">
                                    {effectiveQuery.data.subject.type === "ROLE"
                                        ? "角色"
                                        : "用户"}
                                </Badge>
                                <span className="font-medium">
                                    {effectiveQuery.data.subject.label}
                                </span>
                                <Badge variant="outline">
                                    版本 v
                                    {effectiveQuery.data.permissionVersion
                                        .split("-")
                                        .at(-1)}
                                </Badge>
                                <span className="text-xs text-muted-foreground">
                                    计算于{" "}
                                    {formatDateTime(
                                        effectiveQuery.data.calculatedAt,
                                        "full",
                                    )}
                                </span>
                            </div>

                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    模块与动作权限来源
                                </h3>
                                {effectiveQuery.data.moduleAndActionGrants
                                    .length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        无有效模块授权
                                    </p>
                                ) : (
                                    effectiveQuery.data.moduleAndActionGrants.map(
                                        (g) => (
                                            <div
                                                key={g.id}
                                                className="rounded-lg border border-border p-3 text-sm"
                                            >
                                                <div className="font-medium">
                                                    {g.targetLabel}
                                                </div>
                                                <div className="text-muted-foreground">
                                                    {g.capability} · 来源{" "}
                                                    {g.sourceLabel}（
                                                    {g.sourceType}）
                                                </div>
                                            </div>
                                        ),
                                    )
                                )}
                            </section>

                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    数据范围来源
                                </h3>
                                {effectiveQuery.data.dataScopes.map((g) => (
                                    <div
                                        key={g.id}
                                        className="rounded-lg border border-border p-3 text-sm"
                                    >
                                        <div className="font-medium">
                                            {g.targetLabel}
                                        </div>
                                        <div className="text-muted-foreground">
                                            来源 {g.sourceLabel}
                                        </div>
                                    </div>
                                ))}
                            </section>

                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    字段策略来源
                                </h3>
                                {effectiveQuery.data.fieldPolicies.map((g) => (
                                    <div
                                        key={g.id}
                                        className="rounded-lg border border-border p-3 text-sm"
                                    >
                                        <div className="font-medium">
                                            {g.targetLabel}
                                        </div>
                                        <div className="text-muted-foreground">
                                            {g.capability} · {g.sourceLabel}
                                        </div>
                                    </div>
                                ))}
                            </section>

                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    历史参与者
                                </h3>
                                {effectiveQuery.data.historicalParticipantRules.map(
                                    (e) => (
                                        <div
                                            key={e.id}
                                            className="rounded-lg border border-border p-3 text-sm"
                                        >
                                            <div className="font-medium">
                                                {e.sourceLabel}
                                            </div>
                                            <div className="text-muted-foreground">
                                                {e.message}
                                            </div>
                                        </div>
                                    ),
                                )}
                            </section>

                            <section className="space-y-2">
                                <h3 className="text-sm font-semibold">
                                    拒绝 / 阻塞（含对象状态，不混淆为配置缺失）
                                </h3>
                                {effectiveQuery.data.deniedOrBlocked.map(
                                    (e) => (
                                        <div
                                            key={e.id}
                                            className="rounded-lg border border-warning/40 bg-warning/5 p-3 text-sm"
                                        >
                                            <div className="flex flex-wrap items-center gap-2">
                                                <Badge variant="warning">
                                                    {e.layerLabel}
                                                </Badge>
                                                <span className="font-mono text-xs">
                                                    {e.code}
                                                </span>
                                            </div>
                                            <p className="mt-1 text-muted-foreground">
                                                {e.message}
                                            </p>
                                            <p className="mt-1 text-xs text-muted-foreground">
                                                来源 {e.sourceLabel}（
                                                {e.sourceType}）
                                            </p>
                                        </div>
                                    ),
                                )}
                            </section>

                            {effectiveQuery.data.actionBlockers.length > 0 ? (
                                <section className="space-y-2">
                                    <h3 className="text-sm font-semibold">
                                        当前被阻断的操作
                                    </h3>
                                    {effectiveQuery.data.actionBlockers.map(
                                        (b) => (
                                            <Alert
                                                key={`${b.action}-${b.code}`}
                                                variant="warning"
                                            >
                                                <AlertTitle>
                                                    {b.message}
                                                </AlertTitle>
                                                <AlertDescription>
                                                    {b.message}
                                                </AlertDescription>
                                            </Alert>
                                        ),
                                    )}
                                </section>
                            ) : null}
                        </div>
                    )}
                </div>
            </QuickPreviewSheet>

            {/* 审计详情 — 敏感字段仅字段名 + 已变更 */}
            <QuickPreviewSheet
                open={Boolean(eventOpenId)}
                onOpenChange={(open) => {
                    if (!open) closeEvent()
                }}
                size="detail"
                onOpenChangeComplete={(open) => {
                    if (!open) restoreRowFocus()
                }}
                title="审计事件详情"
                description="追加式事件只读；不展示敏感旧值/新值或密钥。"
            >
                <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-6">
                    {eventQuery.isPending ? (
                        <div className="h-32 animate-pulse rounded-lg bg-muted" />
                    ) : !eventQuery.data ? (
                        <BusinessEmptyState
                            kind="no-data"
                            title="事件不存在或无权查看"
                            description="仅展示你有权查看的审计记录。"
                        />
                    ) : (
                        <div className="flex flex-col gap-3 text-sm">
                            <dl className="grid gap-2 sm:grid-cols-2">
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        审计事件号
                                    </dt>
                                    <dd className="font-mono">
                                        {eventQuery.data.auditEventId}
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        发生时间
                                    </dt>
                                    <dd className="num">
                                        {formatDateTime(
                                            eventQuery.data.recordedAt,
                                            "full",
                                        )}
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        操作者
                                    </dt>
                                    <dd>
                                        {eventQuery.data.actorLabel}（
                                        {eventQuery.data.actorId}）
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        责任角色
                                    </dt>
                                    <dd>{eventQuery.data.actorRole}</dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        动作
                                    </dt>
                                    <dd>{eventQuery.data.actionLabel}</dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        结果
                                    </dt>
                                    <dd>
                                        <BusinessStatusBadge
                                            label={eventQuery.data.resultLabel}
                                            tone={eventQuery.data.resultTone}
                                        />
                                    </dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        对象
                                    </dt>
                                    <dd>{eventQuery.data.objectLabel}</dd>
                                </div>
                                <div>
                                    <dt className="text-xs text-muted-foreground">
                                        请求追踪号
                                    </dt>
                                    <dd className="font-mono text-xs">
                                        {eventQuery.data.traceId}
                                        <div className="text-muted-foreground">
                                            req {eventQuery.data.requestId}
                                        </div>
                                    </dd>
                                </div>
                            </dl>
                            <Separator />
                            <div>
                                <h3 className="text-sm font-semibold">
                                    变更字段
                                </h3>
                                <p className="mt-1 text-muted-foreground">
                                    {eventQuery.data.changedFieldDisplay !== "—"
                                        ? eventQuery.data.changedFieldDisplay
                                        : "无字段变更记录"}
                                </p>
                                <p className="mt-2 text-xs text-muted-foreground">
                                    敏感字段不返回完整旧值或新值；安全摘要默认仅作引用。
                                </p>
                                {eventQuery.data.safeDigest ? (
                                    <p className="mt-1 font-mono text-xs">
                                        安全摘要 {eventQuery.data.safeDigest}
                                    </p>
                                ) : null}
                            </div>
                            <p className="text-xs text-muted-foreground">
                                审计记录不可编辑或删除。打开关联对象时将重新鉴权。
                            </p>
                        </div>
                    )}
                </div>
            </QuickPreviewSheet>
        </>
    )
}

export { AccessPreviewSheets }
