"use client"
import { useEffect, useState } from "react"
import { buildWorkspaceDetailHref } from "@/lib/workspace-detail-href"
import { useRouter, useSearchParams } from "@/lib/navigation"
import {
    BusinessEmptyState,
    BusinessFailureState,
    DataFreshness,
    PageHeader,
    PageScaffold,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    FileTextIcon,
    LayersIcon,
    MailIcon,
    RefreshCwIcon,
    SearchIcon,
    XIcon,
} from "lucide-react"
import { Badge } from "@/components/ui/badge"
import { OptionCombobox } from "@/components/business/option-combobox"
import {
    WorkspaceViewNav,
    WorkspaceViewNavItem,
} from "@/components/business/workspace-view-nav"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupButton,
    InputGroupInput,
} from "@/components/ui/input-group"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { useAccountProfileQuery } from "@/features/auth/queries"
import { COPY_TRIGGER_LABELS } from "@/features/approval-processes/copy-rules"
import { isDocumentType } from "@/features/approval-processes/parse"
import { documentTypeLabel as knownDocumentTypeLabel } from "@/features/approval-processes/labels"
import { hasPermission } from "@/lib/permissions"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { getErrorMessage } from "@/lib/api"
import { useCopies, useReadCopies } from "./queries"
import type { CopyFilters, CopyReceipt } from "./api"
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

export function CopyNavLink({ active = false }: { active?: boolean }) {
    const profile = useAccountProfileQuery().data
    const query = useCopies()
    if (profile?.account_kind !== "admin") return null
    return (
        <WorkspaceViewNavItem
            id="workspace-queue-scope-copied"
            active={active}
            href="/workspace?view=copied"
        >
            <span>抄送给我</span>
            {query.data && (
                <span
                    className="num ml-1 text-xs text-muted-foreground"
                    aria-label={`${query.data.unread_count} 条未读`}
                >
                    {query.data.unread_count}
                </span>
            )}
        </WorkspaceViewNavItem>
    )
}

export function ApprovalCopiesPage() {
    const params = useSearchParams()
    const router = useRouter()
    const profile = useAccountProfileQuery()
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
    const hasActiveFilter = Boolean(
        filters.query || filters.document_type || filters.read !== undefined,
    )
    const types = Object.keys(list.data?.document_type_counts ?? {}).filter(
        (kind) => (list.data?.document_type_counts[kind] ?? 0) > 0,
    )
    if (filters.document_type && !types.includes(filters.document_type))
        types.push(filters.document_type)
    const selectedIds = selected.filter((selectedId) =>
        list.data?.items.some((row) => row.id === selectedId),
    )
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
        <PageScaffold density="compact" className="min-h-0 gap-4">
            <PageHeader
                title="抄送给我"
                actions={
                    <div className="flex flex-wrap items-center gap-3">
                        {list.dataUpdatedAt > 0 && (
                            <DataFreshness
                                updatedAt={new Date(
                                    list.dataUpdatedAt,
                                ).toLocaleTimeString("zh-CN", {
                                    hour: "2-digit",
                                    minute: "2-digit",
                                    hour12: false,
                                })}
                                dateTime={new Date(
                                    list.dataUpdatedAt,
                                ).toISOString()}
                                state={
                                    list.isFetching
                                        ? "syncing"
                                        : list.isError
                                          ? "failed"
                                          : "fresh"
                                }
                                statusLabel={
                                    list.isFetching
                                        ? "正在同步"
                                        : list.isError
                                          ? "更新失败"
                                          : "抄送已更新"
                                }
                            />
                        )}
                        <Button
                            id="copy-refresh"
                            variant="ghost"
                            size="sm"
                            disabled={list.isFetching}
                            onClick={() => void list.refetch()}
                        >
                            <RefreshCwIcon />
                            刷新
                        </Button>
                    </div>
                }
            />
            <WorkspaceViewNav>
                {hasPermission(profile.data.permissions, "work_item:list") && (
                    <WorkspaceViewNavItem id="copy-nav-inbox" href="/workspace">
                        待我处理
                    </WorkspaceViewNavItem>
                )}
                {hasPermission(profile.data.permissions, "work_item:list") &&
                    hasPermission(
                        profile.data.permissions,
                        "work_item:manage",
                    ) && (
                        <WorkspaceViewNavItem
                            id="copy-nav-managed"
                            href="/workspace?view=managed"
                        >
                            范围内待办
                        </WorkspaceViewNavItem>
                    )}
                {hasPermission(profile.data.permissions, "work_item:list") &&
                    hasPermission(
                        profile.data.permissions,
                        "approval_instance:read",
                    ) && (
                        <WorkspaceViewNavItem
                            id="copy-nav-started"
                            href="/workspace?view=started"
                        >
                            我发起的审批
                        </WorkspaceViewNavItem>
                    )}
                <CopyNavLink active />
            </WorkspaceViewNav>
            <section
                aria-label="抄送查询条件"
                className="space-y-3 rounded-xl border border-border/60 bg-muted/15 p-4"
            >
                <form
                    aria-label="抄送查询"
                    className="flex min-w-0 flex-col gap-2"
                    onSubmit={(event) => {
                        event.preventDefault()
                        update({
                            q: search.trim() || undefined,
                            page: undefined,
                        })
                    }}
                >
                    <div className="flex flex-wrap items-center gap-x-4 gap-y-3">
                        <div className="w-full sm:w-80 xl:w-96">
                            <InputGroup>
                                <InputGroupAddon>
                                    <InputGroupButton
                                        id="copy-search-apply"
                                        type="submit"
                                        loading={list.isFetching}
                                        size="icon-xs"
                                        aria-label="查询"
                                    >
                                        <SearchIcon aria-hidden="true" />
                                    </InputGroupButton>
                                </InputGroupAddon>
                                <InputGroupInput
                                    id="copy-search"
                                    aria-label="搜索单号"
                                    placeholder="搜索单号"
                                    value={search}
                                    onChange={(event) =>
                                        setSearch(event.target.value)
                                    }
                                />
                                {(search || filters.query) && (
                                    <InputGroupAddon align="inline-end">
                                        <InputGroupButton
                                            id="copy-search-clear"
                                            size="icon-xs"
                                            aria-label="清除关键词"
                                            onClick={() => {
                                                setSearch("")
                                                update({
                                                    q: undefined,
                                                    page: undefined,
                                                })
                                            }}
                                        >
                                            <XIcon aria-hidden="true" />
                                        </InputGroupButton>
                                    </InputGroupAddon>
                                )}
                            </InputGroup>
                        </div>
                        <OptionCombobox
                            id="copy-read-filter"
                            aria-label="阅读状态"
                            filterLabel="阅读状态"
                            className="w-44 max-w-full"
                            allowClear={false}
                            value={
                                filters.read === undefined
                                    ? "all"
                                    : String(filters.read)
                            }
                            options={[
                                { value: "all", label: "全部" },
                                { value: "false", label: "未读" },
                                { value: "true", label: "已读" },
                            ]}
                            onValueChange={(value) =>
                                update({
                                    read:
                                        value && value !== "all"
                                            ? value
                                            : undefined,
                                    page: undefined,
                                })
                            }
                        />
                    </div>
                    <div
                        role="status"
                        className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground"
                    >
                        <span>
                            查看与我相关的审批资料，阅读不影响审批进度。
                        </span>
                        {search.trim() !== (filters.query ?? "") && (
                            <span>关键词已修改，按回车查询</span>
                        )}
                    </div>
                </form>
                <div
                    role="group"
                    aria-label="审批类型"
                    className="flex flex-wrap items-center gap-2 border-t border-border/60 pt-3"
                >
                    <span className="mr-1 text-xs text-muted-foreground">
                        审批类型
                    </span>
                    {[undefined, ...types].map((kind) => {
                        const active = filters.document_type === kind
                        const Icon = kind ? FileTextIcon : LayersIcon
                        const count =
                            kind && list.data
                                ? (list.data.document_type_counts[kind] ?? 0)
                                : undefined
                        return (
                            <Button
                                key={kind ?? "all"}
                                id={`copy-type-${kind ? toAutomationIdSegment(kind) : "all"}`}
                                type="button"
                                size="sm"
                                variant={active ? "secondary" : "outline"}
                                className="h-auto min-h-8 max-w-full rounded-full px-3 py-1.5 whitespace-normal"
                                aria-pressed={active}
                                onClick={() =>
                                    update({
                                        documentType: kind,
                                        page: undefined,
                                    })
                                }
                            >
                                <Icon
                                    aria-hidden="true"
                                    className="size-4 shrink-0"
                                />
                                {kind ? documentTypeLabel(kind) : "全部"}
                                {count !== undefined && (
                                    <Badge
                                        variant={
                                            active ? "default" : "secondary"
                                        }
                                        className="num min-w-5 rounded-full shadow-none"
                                    >
                                        {count}
                                    </Badge>
                                )}
                            </Button>
                        )
                    })}
                </div>
                {hasActiveFilter && (
                    <div className="flex flex-wrap items-center gap-3 text-xs text-muted-foreground">
                        <span>
                            {[
                                filters.document_type &&
                                    documentTypeLabel(filters.document_type),
                                filters.read !== undefined &&
                                    (filters.read ? "已读" : "未读"),
                                filters.query && `关键词：${filters.query}`,
                            ]
                                .filter(Boolean)
                                .join(" · ")}
                        </span>
                        <Button
                            id="copy-filter-reset"
                            variant="ghost"
                            size="xs"
                            onClick={() => {
                                setSearch("")
                                update({
                                    q: undefined,
                                    read: undefined,
                                    documentType: undefined,
                                    page: undefined,
                                })
                            }}
                        >
                            清除筛选
                        </Button>
                    </div>
                )}
            </section>
            {mark.isError && (
                <p role="alert" className="text-sm text-destructive">
                    {getErrorMessage(mark.error)}
                </p>
            )}
            <section
                data-slot="workspace-queue"
                aria-label="抄送队列"
                className="flex min-h-0 flex-1 flex-col overflow-hidden rounded-xl border border-border/60 bg-card"
            >
                {list.isError ? (
                    <BusinessFailureState
                        error={list.error}
                        onRetry={() => void list.refetch()}
                    />
                ) : list.isPending ? (
                    <p
                        role="status"
                        className="p-8 text-sm text-muted-foreground"
                    >
                        正在读取抄送…
                    </p>
                ) : (
                    <>
                        {list.data.items.length ? (
                            <>
                                <div className="flex flex-wrap items-center justify-between gap-3 border-b border-grid bg-muted/35 px-4 py-3 text-xs sm:px-5">
                                    <span className="font-medium">
                                        抄送事项
                                    </span>
                                    <div className="flex flex-wrap items-center gap-3">
                                        {selectedIds.length > 0 && (
                                            <span className="text-muted-foreground">
                                                已选 {selectedIds.length} 条
                                            </span>
                                        )}
                                        <Button
                                            id="copy-mark-selected"
                                            size="xs"
                                            variant="outline"
                                            disabled={
                                                !selectedIds.length ||
                                                mark.isPending
                                            }
                                            onClick={() =>
                                                mark.mutate(selectedIds, {
                                                    onSuccess: () =>
                                                        setSelected([]),
                                                })
                                            }
                                        >
                                            标记选中已读
                                        </Button>
                                    </div>
                                </div>
                                <ul
                                    className="flex min-h-0 flex-1 flex-col divide-y divide-grid overflow-auto scroll-fade-b"
                                    aria-label="抄送列表"
                                >
                                    {list.data.items.map((row) => (
                                        <CopyListRow
                                            key={row.id}
                                            row={row}
                                            selected={selectedIds.includes(
                                                row.id,
                                            )}
                                            onSelectedChange={(checked) =>
                                                setSelected((ids) =>
                                                    checked
                                                        ? [...ids, row.id]
                                                        : ids.filter(
                                                              (item) =>
                                                                  item !==
                                                                  row.id,
                                                          ),
                                                )
                                            }
                                            onOpen={() =>
                                                router.push(
                                                    buildWorkspaceDetailHref(
                                                        "copies",
                                                        row.id,
                                                        {
                                                            returnTo: `/workspace?${params}`,
                                                        },
                                                    ),
                                                )
                                            }
                                        />
                                    ))}
                                </ul>
                            </>
                        ) : (
                            <BusinessEmptyState
                                kind="no-tasks"
                                icon={MailIcon}
                                title={
                                    hasActiveFilter
                                        ? "没有符合条件的抄送"
                                        : "当前没有抄送事项"
                                }
                                description={
                                    hasActiveFilter
                                        ? "可调整条件或清除筛选。"
                                        : "新的审批抄送到达后会显示在这里。"
                                }
                            />
                        )}
                        <footer className="flex shrink-0 items-center justify-between gap-3 border-t px-5 py-3 text-xs text-muted-foreground">
                            <span>共 {list.data.total} 条抄送</span>
                            {(list.data.total > 20 || page > 1) && (
                                <div className="flex items-center gap-2">
                                    <Button
                                        id="copy-page-prev"
                                        size="xs"
                                        variant="ghost"
                                        disabled={page <= 1 || list.isFetching}
                                        onClick={() =>
                                            update({
                                                page: String(page - 1),
                                            })
                                        }
                                    >
                                        上一页
                                    </Button>
                                    <span>第 {page} 页</span>
                                    <Button
                                        id="copy-page-next"
                                        size="xs"
                                        variant="ghost"
                                        disabled={
                                            page * 20 >= list.data.total ||
                                            list.isFetching
                                        }
                                        onClick={() =>
                                            update({
                                                page: String(page + 1),
                                            })
                                        }
                                    >
                                        下一页
                                    </Button>
                                </div>
                            )}
                        </footer>
                    </>
                )}
            </section>
        </PageScaffold>
    )
}

function CopyListRow({
    row,
    selected,
    onSelectedChange,
    onOpen,
}: {
    row: CopyReceipt
    selected: boolean
    onSelectedChange: (checked: boolean) => void
    onOpen: () => void
}) {
    return (
        <li className="flex items-center gap-3 px-4 py-4 transition-colors hover:bg-muted/30 sm:px-5">
            <NativeCheckbox
                id={`copy-select-${toAutomationIdSegment(row.id)}`}
                aria-label={`选择${row.document_no}`}
                checked={selected}
                onChange={(event) => onSelectedChange(event.target.checked)}
            />
            <button
                id={`copy-open-${toAutomationIdSegment(row.id)}`}
                type="button"
                aria-label={`查看${documentTypeLabel(row.document_type)} ${row.document_no}`}
                className="flex min-w-0 flex-1 flex-wrap items-center justify-between gap-x-6 gap-y-2 rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring/50"
                onClick={onOpen}
            >
                <span className="min-w-0 space-y-1">
                    <span className="flex flex-wrap items-center gap-2 text-sm font-medium">
                        {documentTypeLabel(row.document_type)}
                        <span className="num break-all text-muted-foreground">
                            {row.document_no}
                        </span>
                        {!row.read_at && (
                            <Badge variant="secondary">未读</Badge>
                        )}
                    </span>
                    <span className="block text-xs text-muted-foreground">
                        {row.submitter_name || "申请人未记录"} ·{" "}
                        {COPY_TRIGGER_LABELS[row.trigger]}
                        {row.node_name ? ` · ${row.node_name}` : ""} · 第{" "}
                        {row.round_no} 轮
                    </span>
                </span>
                <span className="space-y-1 text-xs sm:text-right">
                    <span className="block">
                        {STATUS[row.instance_status ?? ""] ?? "状态待核对"}
                    </span>
                    <span className="num block text-muted-foreground">
                        {time(row.copied_at)}
                    </span>
                </span>
            </button>
        </li>
    )
}
