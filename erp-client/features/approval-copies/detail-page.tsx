"use client"
import { useEffect, useRef, useState } from "react"
import { useRouter, useSearchParams } from "@/lib/navigation"
import { workspaceQueueHref } from "@/lib/workspace-detail-href"
import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
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
    SUPERSEDED: "已替换",
}

export function ApprovalCopyDetailPage({ id }: { id: string }) {
    const router = useRouter()
    const params = useSearchParams()
    const returnTo = workspaceQueueHref(
        params.get("returnTo"),
        "/workspace?view=copied",
    )
    return (
        <PageScaffold>
            <PageHeader title="抄送详情" />
            <CopyDetailPane key={id} id={id} />
            <div className="flex justify-end">
                <Button
                    id="copy-back-to-list"
                    variant="outline"
                    onClick={() => router.replace(returnTo)}
                >
                    返回抄送列表
                </Button>
            </div>
        </PageScaffold>
    )
}

function CopyDetailPane({ id }: { id: string }) {
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
    if (query.isError)
        return (
            <BusinessFailureState
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    if (!query.data || query.isFetching)
        return <p role="status">正在读取抄送资料…</p>
    const data = query.data
    return (
        <div className="space-y-5">
            <div className="flex flex-wrap gap-3">
                <span className="font-medium">{data.receipt.document_no}</span>
                <span>{STATUS[data.instance_status] ?? "状态待核对"}</span>
                <span className="text-muted-foreground">
                    {COPY_TRIGGER_LABELS[data.receipt.trigger]} ·{" "}
                    {time(data.receipt.copied_at)}
                </span>
            </div>
            {mark.isError && (
                <div role="alert" className="text-sm">
                    已读状态保存失败。
                    <Button
                        id="copy-read-retry"
                        variant="outline"
                        onClick={() => mark.mutate([id])}
                    >
                        重试标记已读
                    </Button>
                </div>
            )}
            <ApprovalSubmittedMaterialsContent
                instanceId={data.receipt.instance_id}
                data={data.materials}
                attachmentContent={
                    <CopyFiles id={id} files={data.materials.attachments} />
                }
            />
            <CopyPurchaseLines lines={data.materials.purchase_lines ?? []} />
            {data.materials.source_sales_orders?.map((order) => (
                <details
                    key={order.revision.id}
                    className="rounded-lg border p-4"
                >
                    <summary className="cursor-pointer text-sm font-medium">
                        查看关联销售单完整资料 · {order.document_no}
                    </summary>
                    <ApprovalSubmittedMaterialsContent
                        instanceId={data.receipt.instance_id}
                        data={data.materials}
                        view="source-sales"
                        sourceRevisionId={order.revision.id}
                        attachmentContent={
                            <p className="text-sm text-muted-foreground">
                                附件请在上方本次提交资料中查看。
                            </p>
                        }
                    />
                </details>
            ))}
            <section className="space-y-3 rounded-lg border p-4">
                <h2 className="font-medium">审批进度</h2>
                {data.history.items.map((item) => (
                    <div
                        key={item.execution_id}
                        className="border-t pt-3 text-sm"
                    >
                        <p>
                            第 {item.round_no} 轮 · {item.node_name} ·{" "}
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
                <div className="flex gap-2">
                    {after && (
                        <Button
                            id="copy-history-first"
                            variant="outline"
                            onClick={() => setAfter(undefined)}
                        >
                            返回首段记录
                        </Button>
                    )}
                    {data.history.has_more && (
                        <Button
                            id="copy-history-next"
                            variant="outline"
                            onClick={() =>
                                setAfter(data.history.next_cursor ?? undefined)
                            }
                        >
                            后续记录
                        </Button>
                    )}
                </div>
            </section>
        </div>
    )
}
