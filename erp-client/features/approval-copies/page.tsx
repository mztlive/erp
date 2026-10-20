"use client"
import Link from "next/link"
import { useEffect, useRef, useState } from "react"
import { useRouter, useSearchParams } from "next/navigation"
import {
    BusinessFailureState,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { ApprovalSubmittedMaterialsContent } from "@/features/approval-workflow/components/approval-submitted-materials"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { isDocumentType } from "@/features/approval-processes/parse"
import { documentTypeLabel as knownDocumentTypeLabel } from "@/features/approval-processes/labels"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api"
import { useCopies, useCopy, useReadCopies } from "./queries"
import { CopyFiles } from "./copy-files"
import { CopyPurchaseLines } from "./purchase-lines"
import type { CopyFilters } from "./api"
const documentTypeLabel = (kind: string) =>
    isDocumentType(kind) ? knownDocumentTypeLabel(kind) : "审批单据"
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

export function CopyNavLink() {
    const profile = useAccountProfileQuery().data
    const query = useCopies()
    if (profile?.account_kind !== "admin") return null
    return (
        <Link
            id="workspace-queue-scope-copied"
            className="px-1.5 py-2 text-sm text-muted-foreground hover:text-foreground"
            href="/workspace?view=copied"
        >
            抄送给我{query.data ? `（${query.data.unread_count}）` : ""}
        </Link>
    )
}

export function ApprovalCopiesPage() {
    const params = useSearchParams()
    const router = useRouter()
    const profile = useAccountProfileQuery()
    const id = params.get("copyReceiptId")
    const page = Math.max(1, parseInt(params.get("page") ?? "1", 10) || 1)
    const read = params.get("read")
    const filters: CopyFilters = {
        page,
        read: read === "true" ? true : read === "false" ? false : undefined,
        document_type: params.get("documentType") || undefined,
        query: params.get("q") || undefined,
    }
    const list = useCopies(filters)
    const [search, setSearch] = useState(filters.query ?? "")
    const [selected, setSelected] = useState<string[]>([])
    const mark = useReadCopies()
    const update = (changes: Record<string, string | undefined>) => {
        const next = new URLSearchParams(params)
        next.set("view", "copied")
        for (const [key, value] of Object.entries(changes)) {
            if (value) next.set(key, value)
            else next.delete(key)
        }
        router.push(`/workspace?${next}`)
        setSelected([])
    }
    useEffect(() => setSearch(filters.query ?? ""), [filters.query])
    if (profile.isPending) return <p role="status">正在读取账号…</p>
    if (profile.isError)
        return (
            <BusinessFailureState
                error={profile.error}
                onRetry={() => void profile.refetch()}
            />
        )
    if (profile.data?.account_kind !== "admin")
        return (
            <BusinessFailureState
                kind="permission"
                title="请使用内部账号查看抄送"
            />
        )
    return (
        <PageScaffold>
            <PageHeader
                title="抄送给我"
                description="查看与我相关的审批资料，阅读不影响审批进度。"
            />
            <nav
                className="flex flex-wrap items-center gap-6 border-b pb-3"
                aria-label="工作视图"
            >
                {hasPermission(profile.data.permissions, "work_item:list") && (
                    <Link id="copy-nav-inbox" href="/workspace">
                        待我处理
                    </Link>
                )}
                <span className="font-medium">
                    抄送给我
                    {list.data ? `（${list.data.unread_count} 未读）` : ""}
                </span>
                {hasPermission(profile.data.permissions, "work_item:list") &&
                    hasPermission(
                        profile.data.permissions,
                        "approval_instance:read",
                    ) && (
                        <Link
                            id="copy-nav-started"
                            href="/workspace?view=started"
                        >
                            我发起的审批
                        </Link>
                    )}
                {hasPermission(profile.data.permissions, "work_item:list") &&
                    hasPermission(
                        profile.data.permissions,
                        "work_item:manage",
                    ) && (
                        <Link
                            id="copy-nav-managed"
                            href="/workspace?view=managed"
                        >
                            范围内待办
                        </Link>
                    )}
            </nav>
            {id ? (
                <>
                    <Button
                        id="copy-back-to-list"
                        className="self-start"
                        variant="outline"
                        onClick={() => update({ copyReceiptId: undefined })}
                    >
                        返回抄送列表
                    </Button>
                    <CopyDetailPane key={id} id={id} />
                </>
            ) : (
                <>
                    <div className="flex flex-wrap gap-2">
                        {([undefined, "false", "true"] as const).map(
                            (value, i) => (
                                <Button
                                    key={value ?? "all"}
                                    id={`copy-filter-${value ?? "all"}`}
                                    variant={
                                        (read ?? undefined) === value
                                            ? "default"
                                            : "outline"
                                    }
                                    onClick={() =>
                                        update({ read: value, page: undefined })
                                    }
                                >
                                    {["全部", "未读", "已读"][i]}
                                </Button>
                            ),
                        )}
                        <Input
                            id="copy-search"
                            className="max-w-64"
                            aria-label="搜索单号"
                            placeholder="搜索单号"
                            value={search}
                            onChange={(e) => setSearch(e.target.value)}
                            onKeyDown={(e) => {
                                if (e.key === "Enter")
                                    update({
                                        q: search.trim() || undefined,
                                        page: undefined,
                                    })
                            }}
                        />
                        <Button
                            id="copy-search-apply"
                            variant="outline"
                            onClick={() =>
                                update({
                                    q: search.trim() || undefined,
                                    page: undefined,
                                })
                            }
                        >
                            查询
                        </Button>
                        <Button
                            id="copy-filter-reset"
                            variant="ghost"
                            onClick={() =>
                                update({
                                    q: undefined,
                                    read: undefined,
                                    documentType: undefined,
                                    page: undefined,
                                })
                            }
                        >
                            重置
                        </Button>
                    </div>
                    <div className="flex flex-wrap gap-2">
                        <Button
                            id="copy-type-all"
                            variant={
                                !filters.document_type ? "secondary" : "ghost"
                            }
                            onClick={() =>
                                update({
                                    documentType: undefined,
                                    page: undefined,
                                })
                            }
                        >
                            全部类型
                        </Button>
                        {Object.entries(
                            list.data?.document_type_counts ?? {},
                        ).map(([kind, count]) => (
                            <Button
                                key={kind}
                                id={`copy-type-${toAutomationIdSegment(kind)}`}
                                variant={
                                    filters.document_type === kind
                                        ? "secondary"
                                        : "ghost"
                                }
                                onClick={() =>
                                    update({
                                        documentType: kind,
                                        page: undefined,
                                    })
                                }
                            >
                                {documentTypeLabel(kind)}（{count}）
                            </Button>
                        ))}
                    </div>
                    {list.isError ? (
                        <BusinessFailureState
                            error={list.error}
                            onRetry={() => void list.refetch()}
                        />
                    ) : list.isPending ? (
                        <p role="status">正在读取抄送…</p>
                    ) : (
                        <>
                            <div className="flex items-center justify-between">
                                <span className="text-sm text-muted-foreground">
                                    共 {list.data.total} 条
                                </span>
                                <Button
                                    id="copy-mark-selected"
                                    disabled={
                                        !selected.length || mark.isPending
                                    }
                                    onClick={() =>
                                        mark.mutate(selected, {
                                            onSuccess: () => setSelected([]),
                                        })
                                    }
                                >
                                    标记选中已读
                                </Button>
                            </div>
                            {mark.isError && (
                                <p role="alert" className="text-destructive">
                                    {getErrorMessage(mark.error)}
                                </p>
                            )}
                            <div className="divide-y rounded-lg border">
                                {!list.data.items.length && (
                                    <p className="p-8 text-center text-muted-foreground">
                                        当前条件下没有抄送。
                                    </p>
                                )}
                                {list.data.items.map((row) => (
                                    <div
                                        key={row.id}
                                        className="flex items-center gap-3 p-4"
                                    >
                                        <NativeCheckbox
                                            id={`copy-select-${toAutomationIdSegment(row.id)}`}
                                            aria-label={`选择${row.document_no}`}
                                            checked={selected.includes(row.id)}
                                            onChange={(e) =>
                                                setSelected((ids) =>
                                                    e.target.checked
                                                        ? [...ids, row.id]
                                                        : ids.filter(
                                                              (item) =>
                                                                  item !==
                                                                  row.id,
                                                          ),
                                                )
                                            }
                                        />
                                        <button
                                            id={`copy-open-${toAutomationIdSegment(row.id)}`}
                                            className="min-w-0 flex-1 space-y-1 text-left"
                                            onClick={() =>
                                                update({
                                                    copyReceiptId: row.id,
                                                })
                                            }
                                        >
                                            <p className="font-medium">
                                                {!row.read_at && (
                                                    <span className="mr-2 text-xs text-primary">
                                                        未读
                                                    </span>
                                                )}
                                                {documentTypeLabel(
                                                    row.document_type,
                                                )}{" "}
                                                · {row.document_no}{" "}
                                                <span className="text-xs text-muted-foreground">
                                                    {STATUS[
                                                        row.instance_status ??
                                                            ""
                                                    ] ?? "状态待核对"}
                                                </span>
                                            </p>
                                            <p className="text-sm text-muted-foreground">
                                                {row.submitter_name ||
                                                    "申请人未记录"}{" "}
                                                ·{" "}
                                                {
                                                    COPY_TRIGGER_LABELS[
                                                        row.trigger
                                                    ]
                                                }
                                                {row.node_name
                                                    ? ` · ${row.node_name}`
                                                    : ""}{" "}
                                                · 第 {row.round_no} 轮
                                            </p>
                                            <p className="text-xs text-muted-foreground">
                                                {time(row.copied_at)}
                                            </p>
                                        </button>
                                    </div>
                                ))}
                            </div>
                            <div className="flex items-center justify-end gap-3">
                                <Button
                                    id="copy-page-prev"
                                    variant="outline"
                                    disabled={page <= 1}
                                    onClick={() =>
                                        update({ page: String(page - 1) })
                                    }
                                >
                                    上一页
                                </Button>
                                <span className="text-sm">第 {page} 页</span>
                                <Button
                                    id="copy-page-next"
                                    variant="outline"
                                    disabled={page * 20 >= list.data.total}
                                    onClick={() =>
                                        update({ page: String(page + 1) })
                                    }
                                >
                                    下一页
                                </Button>
                            </div>
                        </>
                    )}
                </>
            )}
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
