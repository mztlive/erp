"use client"
import { useEffect, useRef, useState } from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { workspaceQueueHref } from "@/lib/workspace-detail-href"
import {
    BusinessFailureState,
    DetailWorkspaceBody,
    DetailWorkspaceChrome,
    DetailWorkspaceMeta,
    DetailWorkspacePage,
    DetailWorkspaceSection,
    detailWorkspaceStyles,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import type { StatusTone } from "@/components/ui/status-badge"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { ApprovalSubmittedMaterialsContent } from "@/features/approval-workflow/components/approval-submitted-materials"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { useCopy, useReadCopies } from "./queries"
import { CopyFiles } from "./copy-files"
import { CopyPurchaseLines } from "./purchase-lines"
const time = (value: number) => new Date(value * 1000).toLocaleString("zh-CN")
const STATUS: Record<string, string> = {
    RUNNING: "审批中",
    APPROVED: "已通过",
    REJECTED: "已驳回",
    CANCELLED: "已撤回",
    BLOCKED: "审批受阻",
    ACTIVE: "待审批",
    SUPERSEDED: "往轮记录",
}

const submittedMaterialsClassName =
    "[&>article]:rounded-none [&>article]:bg-transparent [&>article]:p-0 [&>article]:shadow-none [&>article>header]:pr-0 [&>article>section>article]:rounded-none [&>article>section>article]:border-x-0 [&>article>section>article]:border-b-0 [&>article>section>article]:px-0"

const STATUS_TONE: Record<string, StatusTone> = {
    RUNNING: "info",
    APPROVED: "success",
    REJECTED: "destructive",
    CANCELLED: "void",
    BLOCKED: "warning",
    ACTIVE: "warning",
}

export function ApprovalCopyDetailPage({ id }: { id: string }) {
    const router = useRouter()
    const params = useSearchParams()
    const returnTo = workspaceQueueHref(
        params.get("returnTo"),
        "/workspace/copies",
    )
    return (
        <CopyDetailPane
            key={id}
            id={id}
            onBack={() => router.replace(returnTo)}
        />
    )
}

function CopyDetailPane({ id, onBack }: { id: string; onBack: () => void }) {
    const [after, setAfter] = useState<string | undefined>()
    const query = useCopy(id, after)
    const mark = useReadCopies()
    const attempted = useRef(false)
    useEffect(() => {
        if (
            query.data &&
            !query.isFetching &&
            !query.isError &&
            !query.data.receipt.read_at &&
            !attempted.current
        ) {
            attempted.current = true
            mark.mutate([id])
        }
    }, [query.data, query.isFetching, query.isError, id, mark])
    const data = query.isError ? undefined : query.data
    return (
        <DetailWorkspacePage>
            <DetailWorkspaceChrome
                title="抄送详情"
                primaryStatus={
                    data && {
                        label: STATUS[data.instance_status] ?? "状态待核对",
                        tone: STATUS_TONE[data.instance_status] ?? "neutral",
                    }
                }
                meta={
                    data && (
                        <DetailWorkspaceMeta
                            items={[
                                <span key="number" className="num">
                                    {data.receipt.document_no}
                                </span>,
                                COPY_TRIGGER_LABELS[data.receipt.trigger],
                                time(data.receipt.copied_at),
                            ]}
                        />
                    )
                }
                secondaryActions={
                    <Button
                        id="copy-back-to-list"
                        size="sm"
                        variant="ghost"
                        className={detailWorkspaceStyles.quietButton}
                        onClick={onBack}
                    >
                        返回抄送列表
                    </Button>
                }
            />
            <DetailWorkspaceBody
                alerts={
                    mark.isError && (
                        <div
                            role="alert"
                            className="flex flex-wrap items-center gap-2 text-body-compact"
                        >
                            已读状态保存失败。
                            <Button
                                id="copy-read-retry"
                                size="sm"
                                variant="ghost"
                                className={detailWorkspaceStyles.subtleButton}
                                onClick={() => mark.mutate([id])}
                            >
                                重试标记已读
                            </Button>
                        </div>
                    )
                }
            >
                {query.isError ? (
                    <DetailWorkspaceSection>
                        <BusinessFailureState
                            error={query.error}
                            onRetry={() => void query.refetch()}
                        />
                    </DetailWorkspaceSection>
                ) : !data || query.isFetching ? (
                    <DetailWorkspaceSection>
                        <p
                            role="status"
                            className="text-body-compact text-muted-foreground"
                        >
                            正在读取抄送资料…
                        </p>
                    </DetailWorkspaceSection>
                ) : (
                    <>
                        <DetailWorkspaceSection
                            contentClassName={submittedMaterialsClassName}
                        >
                            <ApprovalSubmittedMaterialsContent
                                instanceId={data.receipt.instance_id}
                                data={data.materials}
                                attachmentContent={
                                    <CopyFiles
                                        id={id}
                                        files={data.materials.attachments}
                                    />
                                }
                            />
                        </DetailWorkspaceSection>
                        <CopyPurchaseLines
                            lines={data.materials.purchase_lines ?? []}
                        />
                        {Boolean(
                            data.materials.source_sales_orders?.length,
                        ) && (
                            <DetailWorkspaceSection contentClassName="divide-y divide-border/60 space-y-0">
                                {data.materials.source_sales_orders?.map(
                                    (order) => (
                                        <details
                                            key={order.revision.id}
                                            className="py-3 first:pt-0 last:pb-0"
                                        >
                                            <summary
                                                id={`copy-source-sales-${toAutomationIdSegment(order.revision.id)}`}
                                                className="cursor-pointer text-body-compact font-medium"
                                            >
                                                查看关联销售单完整资料 ·{" "}
                                                {order.document_no}
                                            </summary>
                                            <div
                                                className={`${submittedMaterialsClassName} pt-4`}
                                            >
                                                <ApprovalSubmittedMaterialsContent
                                                    instanceId={
                                                        data.receipt.instance_id
                                                    }
                                                    data={data.materials}
                                                    view="source-sales"
                                                    sourceRevisionId={
                                                        order.revision.id
                                                    }
                                                    attachmentContent={
                                                        <p className="text-xs text-muted-foreground">
                                                            附件请在上方本次提交资料中查看。
                                                        </p>
                                                    }
                                                />
                                            </div>
                                        </details>
                                    ),
                                )}
                            </DetailWorkspaceSection>
                        )}
                        <DetailWorkspaceSection title="审批进度" width="form">
                            <div className="divide-y divide-border/60">
                                {data.history.items.map((item) => (
                                    <div
                                        key={item.execution_id}
                                        className="py-3 text-body-compact first:pt-0"
                                    >
                                        <p>
                                            第 {item.round_no} 轮 ·{" "}
                                            {item.node_name} ·{" "}
                                            {STATUS[item.result] ?? "已结束"}
                                        </p>
                                        <p className="text-muted-foreground">
                                            {item.assignee_name}
                                            {item.decision_reason
                                                ? ` · ${item.decision_reason}`
                                                : ""}
                                        </p>
                                    </div>
                                ))}
                            </div>
                            <div className="flex flex-wrap gap-2 empty:hidden">
                                {after && (
                                    <Button
                                        id="copy-history-first"
                                        variant="ghost"
                                        size="sm"
                                        className={
                                            detailWorkspaceStyles.subtleButton
                                        }
                                        onClick={() => setAfter(undefined)}
                                    >
                                        回到第一页
                                    </Button>
                                )}
                                {data.history.has_more && (
                                    <Button
                                        id="copy-history-next"
                                        variant="ghost"
                                        size="sm"
                                        className={
                                            detailWorkspaceStyles.subtleButton
                                        }
                                        onClick={() =>
                                            setAfter(
                                                data.history.next_cursor ??
                                                    undefined,
                                            )
                                        }
                                    >
                                        加载更多记录
                                    </Button>
                                )}
                            </div>
                        </DetailWorkspaceSection>
                    </>
                )}
            </DetailWorkspaceBody>
        </DetailWorkspacePage>
    )
}
