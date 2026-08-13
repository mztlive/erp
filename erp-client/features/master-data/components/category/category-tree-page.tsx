"use client"

/**
 * W14 商品分类 · 树形维护页
 * 路由：/master-data/categories
 * 布局：左侧树 + 右侧摘要；新建根/子分类、更新资料、停用。
 */

import * as React from "react"
import Link from "next/link"
import {
    BanIcon,
    ChevronDownIcon,
    ChevronRightIcon,
    DownloadIcon,
    FolderTreeIcon,
    HistoryIcon,
    PlusIcon,
    SearchIcon,
} from "lucide-react"

import {
    BusinessFailureState,
    BusinessStatusBadge,
    DataFreshness,
    FormalActionResult,
    ListToolbar,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { cn } from "@/lib/utils"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    InputGroup,
    InputGroupAddon,
    InputGroupInput,
} from "@/components/ui/input-group"
import {
    buildCategoryForest,
    flattenCategoryForest,
    type CategoryTreeNode,
} from "@/features/master-data/lib/category-tree-model"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import {
    MasterDataCreateDialog,
    MasterDataDisableDialog,
    MasterDataReviseDialog,
} from "@/features/master-data/components/shared/master-data-action-dialog"
import { useMasterDataListQuery } from "@/features/master-data/hooks/queries"
import {
    buildMasterDataExportCsv,
    downloadCsv,
} from "@/features/master-data/lib/export-csv"
import type { MasterDataListItem } from "@/features/master-data/types"

function TreeRow({
    node,
    expanded,
    selectedId,
    onToggle,
    onSelect,
}: {
    node: CategoryTreeNode
    expanded: ReadonlySet<string>
    selectedId: string | null
    onToggle: (id: string) => void
    onSelect: (item: MasterDataListItem) => void
}) {
    const hasChildren = node.children.length > 0
    const isOpen = expanded.has(node.item.stableId)
    const selected = selectedId === node.item.stableId
    const code =
        node.item.dictionaryCode ??
        node.item.keyFacts.find((f) => f.label === "分类代码")?.value ??
        "—"

    return (
        <li>
            <div
                role="treeitem"
                aria-expanded={hasChildren ? isOpen : undefined}
                aria-selected={selected}
                tabIndex={0}
                className={cn(
                    "group flex cursor-pointer items-center gap-1 rounded-md border border-transparent px-1.5 py-1.5 text-sm outline-none",
                    "hover:bg-muted/60 focus-visible:ring-2 focus-visible:ring-ring",
                    selected && "border-border bg-muted",
                )}
                style={{ paddingLeft: `${0.375 + node.depth * 1.1}rem` }}
                onClick={() => onSelect(node.item)}
                onKeyDown={(e) => {
                    if (e.key === "Enter" || e.key === " ") {
                        e.preventDefault()
                        onSelect(node.item)
                    }
                    if (e.key === "ArrowRight" && hasChildren && !isOpen) {
                        e.preventDefault()
                        onToggle(node.item.stableId)
                    }
                    if (e.key === "ArrowLeft" && hasChildren && isOpen) {
                        e.preventDefault()
                        onToggle(node.item.stableId)
                    }
                }}
            >
                {hasChildren ? (
                    <button
                        type="button"
                        className="inline-flex size-6 shrink-0 items-center justify-center rounded-sm text-muted-foreground hover:bg-background"
                        aria-label={isOpen ? "收起" : "展开"}
                        onClick={(e) => {
                            e.stopPropagation()
                            onToggle(node.item.stableId)
                        }}
                    >
                        {isOpen ? (
                            <ChevronDownIcon className="size-3.5" />
                        ) : (
                            <ChevronRightIcon className="size-3.5" />
                        )}
                    </button>
                ) : (
                    <span className="inline-flex size-6 shrink-0" aria-hidden />
                )}
                <FolderTreeIcon
                    className="size-3.5 shrink-0 text-muted-foreground"
                    aria-hidden
                />
                <span className="min-w-0 flex-1 truncate font-medium">
                    {node.item.name}
                </span>
                <span className="num shrink-0 text-xs text-muted-foreground">
                    {code}
                </span>
                <span className="shrink-0 scale-90">
                    <BusinessStatusBadge
                        context="list"
                        label={node.item.lifecycleStatusLabel}
                        tone={node.item.lifecycleTone}
                    />
                </span>
            </div>
            {hasChildren && isOpen ? (
                <ul role="group" className="m-0 list-none p-0">
                    {node.children.map((child) => (
                        <TreeRow
                            key={child.item.stableId}
                            node={child}
                            expanded={expanded}
                            selectedId={selectedId}
                            onToggle={onToggle}
                            onSelect={onSelect}
                        />
                    ))}
                </ul>
            ) : null}
        </li>
    )
}

export function CategoryTreePage() {
    const [search, setSearch] = React.useState("")
    const [lifecycleStatus, setLifecycleStatus] = React.useState<
        "enabled" | "disabled" | "all"
    >("all")
    const [selectedId, setSelectedId] = React.useState<string | null>(null)
    const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set())
    const [createOpen, setCreateOpen] = React.useState(false)
    const [createParentId, setCreateParentId] = React.useState<
        string | undefined
    >()
    const [reviseTarget, setReviseTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [disableTarget, setDisableTarget] =
        React.useState<MasterDataListItem | null>(null)
    const [exportMeta, setExportMeta] = React.useState<{
        jobId: string
        rowCount: number
    } | null>(null)
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)

    // `/` 聚焦分类搜索（弹窗打开时不触发）
    React.useEffect(() => {
        const onKey = (event: KeyboardEvent) => {
            if (
                event.key === "/" &&
                !(event.target instanceof HTMLInputElement) &&
                !(event.target instanceof HTMLTextAreaElement)
            ) {
                if (
                    document.querySelector(
                        '[role="dialog"], [data-slot="sheet"]',
                    )
                ) {
                    return
                }
                event.preventDefault()
                searchInputRef.current?.focus()
            }
        }
        window.addEventListener("keydown", onKey)
        return () => window.removeEventListener("keydown", onKey)
    }, [])

    const listQuery = useMasterDataListQuery({
        resource: "categories",
        q: search,
        lifecycleStatus,
        revisionTiming: "all",
    })

    const rows = React.useMemo(
        () => listQuery.data?.rows ?? [],
        [listQuery.data?.rows],
    )
    const forest = React.useMemo(() => buildCategoryForest(rows), [rows])
    const flat = React.useMemo(() => flattenCategoryForest(forest), [forest])

    // 首次加载默认展开全部根
    React.useEffect(() => {
        if (expanded.size > 0 || forest.length === 0) return
        setExpanded(new Set(forest.map((n) => n.item.stableId)))
    }, [forest, expanded.size])

    const selected =
        rows.find((r) => r.stableId === selectedId) ??
        flat.find((n) => n.item.stableId === selectedId)?.item ??
        null

    const selectedPath =
        flat.find((n) => n.item.stableId === selectedId)?.pathLabel ??
        selected?.name

    /** 可见节点数：根节点 + 展开父级下的所有后代（与界面实际渲染一致）。 */
    const visibleCount = React.useMemo(() => {
        return forest.reduce((count, node) => {
            let total = 1
            const walk = (n: CategoryTreeNode): void => {
                if (!expanded.has(n.item.stableId)) return
                for (const child of n.children) {
                    total += 1
                    walk(child)
                }
            }
            walk(node)
            return count + total
        }, 0)
    }, [forest, expanded])

    /** 搜索/启停筛选是否生效：空态与「系统从未建分类」区分。 */
    const filterActive = search.trim() !== "" || lifecycleStatus !== "all"

    const toggle = React.useCallback((id: string) => {
        setExpanded((prev) => {
            const next = new Set(prev)
            if (next.has(id)) next.delete(id)
            else next.add(id)
            return next
        })
    }, [])

    const expandAll = () => {
        setExpanded(new Set(flat.map((n) => n.item.stableId)))
    }

    const collapseAll = () => {
        setExpanded(new Set())
    }

    const openCreateRoot = () => {
        setCreateParentId(undefined)
        setCreateOpen(true)
    }

    const openCreateChild = (parent: MasterDataListItem) => {
        setCreateParentId(parent.stableId)
        setCreateOpen(true)
    }

    if (listQuery.isPending) {
        return (
            <PageScaffold density="compact">
                <PageHeader title={masterDataCopy.pageTitle("商品分类")} />
                <div
                    className="h-40 animate-pulse rounded-lg bg-muted"
                    aria-busy
                />
            </PageScaffold>
        )
    }

    if (listQuery.isError || !listQuery.data) {
        return (
            <PageScaffold density="compact">
                <PageHeader title={masterDataCopy.pageTitle("商品分类")} />
                <BusinessFailureState
                    error={listQuery.error}
                    action={
                        <Button
                            type="button"
                            onClick={() => void listQuery.refetch()}
                        >
                            重试
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    return (
        <PageScaffold density="compact">
            <PageHeader
                title={masterDataCopy.pageTitle("商品分类")}
                breadcrumbs={[
                    { id: "md", label: "基础资料", href: "/master-data" },
                    { id: "resource", label: "商品分类", current: true },
                ]}
                metadata={
                    <DataFreshness
                        updatedAt="刚刚"
                        dateTime={listQuery.data.queriedAt}
                        state="fresh"
                        label="商品分类树"
                    />
                }
                actions={
                    <PageActions
                        actions={[
                            {
                                actionKey: "export",
                                label: masterDataCopy.actionExport,
                                icon: DownloadIcon,
                                variant: "outline",
                                mobileVisibility: "hide",
                                disabled: rows.length === 0,
                                onClick: () => {
                                    if (rows.length === 0) return
                                    const csv = buildMasterDataExportCsv(
                                        rows,
                                        `分类=${masterDataCopy.categoryTreeTitle}`,
                                    )
                                    downloadCsv(csv, `基础资料-商品分类`)
                                    const datePart = new Date()
                                        .toISOString()
                                        .slice(0, 10)
                                        .replace(/-/g, "")
                                    setExportMeta({
                                        jobId: `导出-${datePart}-${String(
                                            Date.now() % 100000,
                                        ).padStart(5, "0")}`,
                                        rowCount: rows.length,
                                    })
                                },
                            },
                            {
                                actionKey: "create-root",
                                label: masterDataCopy.categoryAddRoot,
                                icon: PlusIcon,
                                onClick: openCreateRoot,
                            },
                        ]}
                    />
                }
            />

            <p className="text-sm text-muted-foreground">
                {masterDataCopy.categoryTreeDesc(rows.length)}
            </p>

            {exportMeta ? (
                <p className="text-xs text-muted-foreground">
                    导出已完成：{exportMeta.rowCount} 条 · 任务号{" "}
                    <span className="num">{exportMeta.jobId}</span>
                    。文件已开始下载。
                </p>
            ) : null}

            <div className={`${surfacePanelClassName} px-3 py-2.5`}>
                <ListToolbar
                    aria-label="分类树筛选"
                    search={
                        <InputGroup>
                            <InputGroupAddon>
                                <SearchIcon aria-hidden />
                            </InputGroupAddon>
                            <InputGroupInput
                                ref={searchInputRef}
                                value={search}
                                onChange={(e) => setSearch(e.target.value)}
                                placeholder={masterDataCopy.categoryTreeSearch}
                                aria-label={masterDataCopy.categoryTreeSearch}
                            />
                        </InputGroup>
                    }
                    filters={
                        <div
                            role="group"
                            aria-label="生命周期"
                            className="inline-flex gap-1"
                        >
                            {(
                                [
                                    ["all", "全部"],
                                    ["enabled", "启用"],
                                    ["disabled", "停用"],
                                ] as const
                            ).map(([value, label]) => (
                                <Button
                                    key={value}
                                    type="button"
                                    size="sm"
                                    variant={
                                        lifecycleStatus === value
                                            ? "secondary"
                                            : "ghost"
                                    }
                                    onClick={() => setLifecycleStatus(value)}
                                >
                                    {label}
                                </Button>
                            ))}
                        </div>
                    }
                    actions={
                        <>
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={expandAll}
                            >
                                {masterDataCopy.categoryExpandAll}
                            </Button>
                            <Button
                                type="button"
                                size="sm"
                                variant="ghost"
                                onClick={collapseAll}
                            >
                                {masterDataCopy.categoryCollapseAll}
                            </Button>
                        </>
                    }
                />
            </div>

            <div className="grid min-h-[28rem] gap-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
                <section
                    className={cn(
                        surfacePanelClassName,
                        "flex min-h-0 flex-col",
                    )}
                    aria-label={masterDataCopy.categoryTreeTitle}
                >
                    <div className="flex items-center justify-between border-b border-border/30 px-3 py-2">
                        <h2 className="text-sm font-semibold">
                            {masterDataCopy.categoryTreeTitle}
                        </h2>
                        <span className="text-xs text-muted-foreground">
                            可见 {visibleCount} 项 · 共 {rows.length} 项
                        </span>
                    </div>
                    <div className="min-h-0 flex-1 overflow-y-auto p-2">
                        {forest.length === 0 ? (
                            filterActive ? (
                                <div className="flex flex-col items-center gap-3 py-12 text-center">
                                    <p className="text-sm text-muted-foreground">
                                        {masterDataCopy.categoryTreeNoMatch}
                                    </p>
                                    <p className="text-xs text-muted-foreground">
                                        {masterDataCopy.categoryTreeNoMatchDesc}
                                    </p>
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="secondary"
                                        className="rounded-lg shadow-none"
                                        onClick={() => {
                                            setSearch("")
                                            setLifecycleStatus("all")
                                        }}
                                    >
                                        清除筛选
                                    </Button>
                                </div>
                            ) : (
                                <div className="flex flex-col items-center gap-3 py-12 text-center">
                                    <p className="text-sm text-muted-foreground">
                                        {masterDataCopy.categoryTreeEmpty}
                                    </p>
                                    <Button
                                        type="button"
                                        size="sm"
                                        onClick={openCreateRoot}
                                    >
                                        <PlusIcon
                                            data-icon="inline-start"
                                            aria-hidden
                                        />
                                        {masterDataCopy.categoryAddRoot}
                                    </Button>
                                </div>
                            )
                        ) : (
                            <ul role="tree" className="m-0 list-none p-0">
                                {forest.map((node) => (
                                    <TreeRow
                                        key={node.item.stableId}
                                        node={node}
                                        expanded={expanded}
                                        selectedId={selectedId}
                                        onToggle={toggle}
                                        onSelect={(item) =>
                                            setSelectedId(item.stableId)
                                        }
                                    />
                                ))}
                            </ul>
                        )}
                    </div>
                </section>

                <section
                    className={cn(
                        surfacePanelClassName,
                        "flex min-h-0 flex-col",
                    )}
                    aria-label="分类详情"
                >
                    <div className="border-b border-border/30 px-3 py-2">
                        <h2 className="text-sm font-semibold">分类详情</h2>
                    </div>
                    <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
                        {!selected && selectedId ? (
                            <div className="flex flex-col gap-2">
                                <p className="text-sm text-muted-foreground">
                                    当前选中的分类不在筛选结果中。
                                </p>
                                <Button
                                    type="button"
                                    size="sm"
                                    variant="secondary"
                                    className="rounded-lg shadow-none"
                                    onClick={() => {
                                        setSearch("")
                                        setLifecycleStatus("all")
                                    }}
                                >
                                    清除筛选后查看
                                </Button>
                            </div>
                        ) : !selected ? (
                            <p className="text-sm text-muted-foreground">
                                在左侧选择一个分类，查看路径、版本并执行维护。
                            </p>
                        ) : (
                            <>
                                <div className="space-y-1">
                                    <div className="text-lg font-semibold">
                                        {selected.name}
                                    </div>
                                    <div className="num text-sm text-muted-foreground">
                                        {selected.stableNo} · v
                                        {selected.revisionNo}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        路径：{selectedPath}
                                    </div>
                                </div>
                                <div className="flex flex-wrap gap-2">
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={selected.lifecycleStatusLabel}
                                        tone={selected.lifecycleTone}
                                    />
                                    <Badge
                                        variant={
                                            selected.revisionTiming === "FUTURE"
                                                ? "warning"
                                                : "secondary"
                                        }
                                    >
                                        {selected.revisionTimingLabel}
                                    </Badge>
                                </div>
                                <dl className="grid gap-2 text-sm sm:grid-cols-2">
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            {masterDataCopy.categoryColCode}
                                        </dt>
                                        <dd className="num font-medium">
                                            {selected.dictionaryCode ??
                                                selected.keyFacts.find(
                                                    (f) =>
                                                        f.label === "分类代码",
                                                )?.value ??
                                                "—"}
                                        </dd>
                                    </div>
                                    <div>
                                        <dt className="text-xs text-muted-foreground">
                                            {masterDataCopy.categoryColParent}
                                        </dt>
                                        <dd className="font-medium">
                                            {selected.parentStableId
                                                ? (rows.find(
                                                      (r) =>
                                                          r.stableId ===
                                                          selected.parentStableId,
                                                  )?.name ?? "—")
                                                : masterDataCopy.categoryParentRoot}
                                        </dd>
                                    </div>
                                    <div className="sm:col-span-2">
                                        <dt className="text-xs text-muted-foreground">
                                            {masterDataCopy.categoryColKind}
                                        </dt>
                                        <dd className="font-medium">
                                            {selected.productKind ??
                                                selected.keyFacts.find(
                                                    (f) =>
                                                        f.label ===
                                                        "适用商品类型",
                                                )?.value ??
                                                "—"}
                                        </dd>
                                    </div>
                                </dl>
                                <div className="flex flex-wrap gap-2 border-t border-border/30 pt-3">
                                    <Button
                                        type="button"
                                        size="sm"
                                        variant="outline"
                                        render={
                                            <Link
                                                href={`/master-data/categories/${selected.stableId}?section=overview`}
                                            />
                                        }
                                    >
                                        打开完整资料
                                    </Button>
                                    <span
                                        title={
                                            !selected.allowedActions.includes(
                                                "CREATE_REVISION",
                                            )
                                                ? selected.actionBlockers.find(
                                                      (b) =>
                                                          b.action ===
                                                          "CREATE_REVISION",
                                                  )?.message
                                                : undefined
                                        }
                                        className="inline-flex"
                                    >
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={
                                                !selected.allowedActions.includes(
                                                    "CREATE_REVISION",
                                                )
                                            }
                                            onClick={() =>
                                                openCreateChild(selected)
                                            }
                                        >
                                            <PlusIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {masterDataCopy.categoryAddChild}
                                        </Button>
                                    </span>
                                    <span
                                        title={
                                            !selected.allowedActions.includes(
                                                "CREATE_REVISION",
                                            )
                                                ? selected.actionBlockers.find(
                                                      (b) =>
                                                          b.action ===
                                                          "CREATE_REVISION",
                                                  )?.message
                                                : undefined
                                        }
                                        className="inline-flex"
                                    >
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={
                                                !selected.allowedActions.includes(
                                                    "CREATE_REVISION",
                                                )
                                            }
                                            onClick={() =>
                                                setReviseTarget(selected)
                                            }
                                        >
                                            <HistoryIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {masterDataCopy.actionUpdate}
                                        </Button>
                                    </span>
                                    <span
                                        title={
                                            !selected.allowedActions.includes(
                                                "DISABLE",
                                            )
                                                ? selected.actionBlockers.find(
                                                      (b) =>
                                                          b.action ===
                                                          "DISABLE",
                                                  )?.message
                                                : undefined
                                        }
                                        className="inline-flex"
                                    >
                                        <Button
                                            type="button"
                                            size="sm"
                                            variant="outline"
                                            disabled={
                                                !selected.allowedActions.includes(
                                                    "DISABLE",
                                                )
                                            }
                                            onClick={() =>
                                                setDisableTarget(selected)
                                            }
                                        >
                                            <BanIcon
                                                data-icon="inline-start"
                                                aria-hidden
                                            />
                                            {masterDataCopy.actionDisable}
                                        </Button>
                                    </span>
                                </div>
                                {selected.primaryBlocker ? (
                                    <FormalActionResult
                                        status="blocked"
                                        title="当前不可用"
                                        description={selected.primaryBlocker}
                                    />
                                ) : null}
                            </>
                        )}
                    </div>
                </section>
            </div>

            <CategoryCreateDialog
                open={createOpen}
                onOpenChange={setCreateOpen}
                defaultParentId={createParentId}
            />
            <MasterDataReviseDialog
                open={reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) setReviseTarget(null)
                }}
                resource="categories"
                target={reviseTarget}
            />
            <MasterDataDisableDialog
                open={disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) setDisableTarget(null)
                }}
                resource="categories"
                target={disableTarget}
            />
        </PageScaffold>
    )
}

/**
 * 包装新建对话框：支持默认上级（子分类）。
 * 通过 key 强制在 parent 变化时重置表单默认值。
 */
function CategoryCreateDialog({
    open,
    onOpenChange,
    defaultParentId,
}: {
    open: boolean
    onOpenChange: (open: boolean) => void
    defaultParentId?: string
}) {
    return (
        <MasterDataCreateDialog
            key={`cat-create-${defaultParentId ?? "root"}-${open}`}
            open={open}
            onOpenChange={onOpenChange}
            resource="categories"
            defaultFieldValues={
                defaultParentId ? { parentId: defaultParentId } : undefined
            }
        />
    )
}
