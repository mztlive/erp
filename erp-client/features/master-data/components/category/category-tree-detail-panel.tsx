"use client"

import * as React from "react"
import Link from "next/link"
import {
    ArrowUpRightIcon,
    BanIcon,
    CheckIcon,
    ChevronRightIcon,
    CopyIcon,
    FolderIcon,
    FolderInputIcon,
    FolderOpenIcon,
    FolderTreeIcon,
    MoreHorizontalIcon,
    PencilIcon,
    PlusIcon,
} from "lucide-react"

import { PermissionGuard } from "@/features/auth/components/permission-guard"
import {
    BusinessEmptyState,
    BusinessStatusBadge,
    DetailWorkspaceMeta,
    FormalActionResult,
    QuantityValue,
    TableEmptyCell,
    TableIdentityCell,
    TableRowActions,
    type TableRowAction,
} from "@/components/business"
import { listWorkspaceEmptyStateClassName } from "@/components/business/list-workspace"
import { Badge, type badgeVariants } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import {
    Table,
    TableBody,
    TableCell,
    TableHead,
    TableHeader,
    TableRow,
} from "@/components/ui/table"
import { CategoryName } from "./category-tree-list"
import {
    countCategoryDescendants,
    type CategoryTreeNode,
} from "@/features/master-data/lib/category-tree-model"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import type { VariantProps } from "class-variance-authority"

type BadgeVariant = NonNullable<VariantProps<typeof badgeVariants>["variant"]>

function productKindBadgeVariant(kind?: string): BadgeVariant {
    switch (kind) {
        case "虚拟":
            return "violet"
        case "服务":
            return "teal"
        case "卡券":
            return "orange"
        default:
            return "neutral"
    }
}

function collectAncestorItems(
    rows: readonly MasterDataListItem[],
    startParentId: string | null | undefined,
): MasterDataListItem[] {
    const path: MasterDataListItem[] = []
    const seen = new Set<string>()
    let currentId = startParentId
    while (currentId && !seen.has(currentId)) {
        seen.add(currentId)
        const item = rows.find((row) => row.stableId === currentId)
        if (!item) break
        path.push(item)
        currentId = item.parentStableId
    }
    return path.reverse()
}

function ProductKindBadge({ kind }: { kind?: string }) {
    if (!kind) {
        return <TableEmptyCell variant="label" label="未填写" />
    }
    return <Badge variant={productKindBadgeVariant(kind)}>{kind}</Badge>
}

function CopyCodeButton({ id, code }: { id: string; code: string }) {
    const [copied, setCopied] = React.useState(false)
    React.useEffect(() => {
        if (!copied) return
        const timer = window.setTimeout(() => setCopied(false), 1800)
        return () => window.clearTimeout(timer)
    }, [copied])

    return (
        <Button
            id={id}
            type="button"
            variant="ghost"
            size="icon-xs"
            className="size-5 rounded-sm text-muted-foreground/70 hover:bg-muted hover:text-foreground"
            aria-label={copied ? "已复制分类代码" : "复制分类代码"}
            title={copied ? "已复制" : "复制分类代码"}
            onClick={() => {
                void navigator.clipboard?.writeText(code).then(() => {
                    setCopied(true)
                })
            }}
        >
            {copied ? (
                <CheckIcon aria-hidden="true" className="size-3 text-success" />
            ) : (
                <CopyIcon aria-hidden="true" className="size-3" />
            )}
        </Button>
    )
}

/** 分类工作区：飞书纯白通栏高密度管理视图；支持总览、当前节点属性摘要与子分类快捷维护。 */
export function CategoryTreeDetailPanel({
    selected,
    selectedId,
    selectedNode,
    rows,
    rootCount,
    nodes,
    filterActive,
    matchedIds,
    query,
    returnTo,
    canCreate,
    canUpdate,
    onSelect,
    onClearFilters,
    onOpenCreateRoot,
    onOpenCreateChild,
    onReviseTarget,
    onMoveTarget,
    onDisableTarget,
}: {
    selected: MasterDataListItem | null
    selectedId: string | null
    selectedNode: CategoryTreeNode | undefined
    rows: readonly MasterDataListItem[]
    rootCount: number
    nodes: readonly CategoryTreeNode[]
    filterActive: boolean
    matchedIds: ReadonlySet<string>
    query: string
    returnTo: string
    canCreate: boolean
    canUpdate: boolean
    onSelect: (id: string | null) => void
    onClearFilters: () => void
    onOpenCreateRoot: () => void
    onOpenCreateChild: (item: MasterDataListItem) => void
    onReviseTarget: (item: MasterDataListItem) => void
    onMoveTarget: (item: MasterDataListItem) => void
    onDisableTarget: (item: MasterDataListItem) => void
}) {
    const prefix = "master-data-category-workspace"
    const canEdit =
        selected?.allowedActions.includes("CREATE_REVISION") ?? false
    const canDisable = selected?.allowedActions.includes("DISABLE") ?? false
    const editBlocker =
        selected?.actionBlockers.find(
            (blocker) => blocker.action === "CREATE_REVISION",
        )?.message ?? "当前分类不可编辑"
    const disableBlocker =
        selected?.actionBlockers.find((blocker) => blocker.action === "DISABLE")
            ?.message ?? "当前分类不可停用"
    const canAddChild = canCreate && canEdit
    const ancestors = collectAncestorItems(rows, selected?.parentStableId)
    const parentItem = selected?.parentStableId
        ? rows.find((item) => item.stableId === selected.parentStableId)
        : null
    const selectedCode = selected
        ? (selected.dictionaryCode ?? selected.stableNo)
        : ""
    const directChildCount = selectedNode?.children.length ?? 0
    const descendantCount = selectedNode
        ? countCategoryDescendants(selectedNode)
        : 0
    const subCategoryCount = Math.max(0, rows.length - rootCount)

    if (selectedId && !selected) {
        return (
            <section className="px-6 py-12">
                <BusinessEmptyState
                    kind="no-data"
                    icon={FolderTreeIcon}
                    title="该分类已不可用"
                    description="分类可能已被移除或当前链接已失效，请重新选择。"
                    className={listWorkspaceEmptyStateClassName}
                    action={
                        <Button
                            id={`${prefix}-missing-back`}
                            variant="outline"
                            size="sm"
                            onClick={() => onSelect(null)}
                        >
                            查看全部分类
                        </Button>
                    }
                />
            </section>
        )
    }

    const buildRowActions = (node: CategoryTreeNode): TableRowAction[] => {
        const item = node.item
        const seg = toAutomationIdSegment(item.stableId)
        const rowCanEdit = item.allowedActions.includes("CREATE_REVISION")
        const rowCanDisable = item.allowedActions.includes("DISABLE")
        const rowEditBlocker =
            item.actionBlockers.find((b) => b.action === "CREATE_REVISION")
                ?.message ?? "当前分类不可编辑"
        const rowDisableBlocker =
            item.actionBlockers.find((b) => b.action === "DISABLE")?.message ??
            "当前分类不可停用"
        const rowCanAddChild = canCreate && rowCanEdit

        return [
            {
                id: `${prefix}-row-${seg}-add-child`,
                label: "添加子分类",
                icon: PlusIcon,
                hidden: !canCreate,
                disabled: !rowCanAddChild,
                disabledReason: !rowCanAddChild ? rowEditBlocker : undefined,
                onClick: () => onOpenCreateChild(item),
            },
            {
                id: `${prefix}-row-${seg}-edit`,
                label: "编辑",
                icon: PencilIcon,
                hidden: !canUpdate,
                disabled: !rowCanEdit,
                disabledReason: !rowCanEdit ? rowEditBlocker : undefined,
                onClick: () => onReviseTarget(item),
            },
            {
                id: `${prefix}-row-${seg}-move`,
                label: "调整上级",
                icon: FolderInputIcon,
                placement: "menu",
                hidden: !canUpdate,
                disabled: !rowCanEdit,
                disabledReason: !rowCanEdit ? rowEditBlocker : undefined,
                onClick: () => onMoveTarget(item),
            },
            {
                id: `${prefix}-row-${seg}-detail`,
                label: "打开完整资料",
                icon: ArrowUpRightIcon,
                placement: "menu",
                href: `/master-data/categories/${item.stableId}?section=overview&returnTo=${encodeURIComponent(returnTo)}`,
            },
            {
                id: `${prefix}-row-${seg}-disable`,
                label: "停用分类",
                icon: BanIcon,
                placement: "menu",
                destructive: true,
                hidden: !canUpdate,
                disabled: !rowCanDisable,
                disabledReason: !rowCanDisable ? rowDisableBlocker : undefined,
                onClick: () => onDisableTarget(item),
            },
        ]
    }

    return (
        <section
            aria-label="分类管理工作区"
            className="flex min-h-full min-w-0 flex-col bg-card"
        >
            {selected ? (
                <div className="shrink-0 space-y-4 border-b border-border/60 px-4 py-4 lg:px-6 lg:py-5">
                    <nav
                        aria-label="分类路径"
                        className="flex flex-wrap items-center gap-1 text-xs text-muted-foreground"
                    >
                        <Button
                            id={`${prefix}-all`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="h-auto px-1 py-0.5 text-xs font-normal text-muted-foreground hover:bg-muted/70 hover:text-foreground"
                            onClick={() => onSelect(null)}
                        >
                            全部分类
                        </Button>
                        {ancestors.map((item) => (
                            <span
                                key={item.stableId}
                                className="flex items-center gap-1"
                            >
                                <ChevronRightIcon
                                    aria-hidden="true"
                                    className="size-3 text-muted-foreground/55"
                                />
                                <Button
                                    id={`${prefix}-ancestor-${toAutomationIdSegment(item.stableId)}`}
                                    type="button"
                                    variant="ghost"
                                    size="xs"
                                    className="h-auto px-1 py-0.5 text-xs font-normal text-muted-foreground hover:bg-muted/70 hover:text-foreground"
                                    onClick={() => onSelect(item.stableId)}
                                >
                                    {item.name}
                                </Button>
                            </span>
                        ))}
                        <ChevronRightIcon
                            aria-hidden="true"
                            className="size-3 text-muted-foreground/55"
                        />
                        <span
                            aria-current="page"
                            className="px-1 py-0.5 font-medium break-all text-foreground"
                        >
                            {selected.name}
                        </span>
                    </nav>

                    <div className="flex flex-wrap items-start justify-between gap-4">
                        <div className="flex min-w-0 flex-1 items-start gap-3">
                            <div
                                aria-hidden="true"
                                className={cn(
                                    "mt-0.5 flex size-9 shrink-0 items-center justify-center rounded-lg",
                                    selected.lifecycleStatus === "DISABLED"
                                        ? "bg-muted text-muted-foreground"
                                        : "bg-sidebar-accent/80 text-primary",
                                )}
                            >
                                {directChildCount > 0 ? (
                                    <FolderOpenIcon className="size-4" />
                                ) : (
                                    <FolderIcon className="size-4" />
                                )}
                            </div>
                            <div className="min-w-0 flex-1 space-y-1">
                                <div className="flex flex-wrap items-center gap-2">
                                    <h2 className="text-section-title font-semibold tracking-tight break-words text-foreground">
                                        {selected.name}
                                    </h2>
                                    <BusinessStatusBadge
                                        context="detail"
                                        label={selected.lifecycleStatusLabel}
                                        tone={selected.lifecycleTone}
                                    />
                                    {selected.productKind ? (
                                        <ProductKindBadge
                                            kind={selected.productKind}
                                        />
                                    ) : null}
                                </div>
                                <DetailWorkspaceMeta
                                    items={[
                                        <span key="depth" className="num">
                                            第 {(selectedNode?.depth ?? 0) + 1}{" "}
                                            级分类
                                        </span>,
                                        <span key="direct" className="num">
                                            直接子分类 {directChildCount} 个
                                        </span>,
                                        descendantCount > directChildCount ? (
                                            <span
                                                key="all-desc"
                                                className="num"
                                            >
                                                含全部下级 {descendantCount} 个
                                            </span>
                                        ) : null,
                                    ]}
                                />
                            </div>
                        </div>

                        <div className="flex shrink-0 flex-wrap items-center gap-1.5">
                            {canCreate ? (
                                <span
                                    title={
                                        !canAddChild ? editBlocker : undefined
                                    }
                                >
                                    <Button
                                        id={`${prefix}-create-child`}
                                        type="button"
                                        variant="ghost"
                                        size="sm"
                                        disabled={!canAddChild}
                                        className="h-control border border-primary/20 bg-sidebar-accent/70 px-3 font-medium text-sidebar-accent-foreground shadow-none hover:bg-sidebar-accent disabled:border-border disabled:bg-muted disabled:text-muted-foreground"
                                        onClick={() =>
                                            onOpenCreateChild(selected)
                                        }
                                    >
                                        <PlusIcon
                                            aria-hidden="true"
                                            className="size-3.5"
                                        />
                                        添加子分类
                                    </Button>
                                </span>
                            ) : null}
                            <PermissionGuard permission="product_category:update">
                                <span
                                    title={!canEdit ? editBlocker : undefined}
                                >
                                    <Button
                                        id={`${prefix}-edit`}
                                        type="button"
                                        variant="outline"
                                        size="sm"
                                        disabled={!canEdit}
                                        className="h-control px-3 shadow-none"
                                        onClick={() => onReviseTarget(selected)}
                                    >
                                        <PencilIcon
                                            aria-hidden="true"
                                            className="size-3.5"
                                        />
                                        编辑分类
                                    </Button>
                                </span>
                            </PermissionGuard>
                            <DropdownMenu>
                                <DropdownMenuTrigger
                                    id={`${prefix}-more`}
                                    render={
                                        <Button
                                            type="button"
                                            variant="ghost"
                                            size="icon-sm"
                                            className="size-control text-muted-foreground hover:bg-muted/80 hover:text-foreground"
                                            aria-label="更多分类操作"
                                        />
                                    }
                                >
                                    <MoreHorizontalIcon className="size-4" />
                                </DropdownMenuTrigger>
                                <DropdownMenuContent
                                    align="end"
                                    className="min-w-44 rounded-lg"
                                >
                                    <PermissionGuard permission="product_category:update">
                                        <DropdownMenuItem
                                            id={`${prefix}-move`}
                                            disabled={!canEdit}
                                            title={
                                                !canEdit
                                                    ? editBlocker
                                                    : undefined
                                            }
                                            className="min-h-8 gap-2 rounded-md text-body-compact [&_svg]:text-muted-foreground"
                                            onClick={() =>
                                                onMoveTarget(selected)
                                            }
                                        >
                                            <FolderInputIcon className="size-3.5" />
                                            调整上级
                                        </DropdownMenuItem>
                                    </PermissionGuard>
                                    <DropdownMenuItem
                                        id={`${prefix}-detail`}
                                        className="min-h-8 gap-2 rounded-md text-body-compact [&_svg]:text-muted-foreground"
                                        render={
                                            <Link
                                                href={`/master-data/categories/${selected.stableId}?section=overview&returnTo=${encodeURIComponent(returnTo)}`}
                                            />
                                        }
                                    >
                                        <ArrowUpRightIcon className="size-3.5" />
                                        打开完整资料
                                    </DropdownMenuItem>
                                    <PermissionGuard permission="product_category:update">
                                        <DropdownMenuSeparator />
                                        <DropdownMenuItem
                                            id={`${prefix}-disable`}
                                            variant="destructive"
                                            disabled={!canDisable}
                                            title={
                                                !canDisable
                                                    ? disableBlocker
                                                    : undefined
                                            }
                                            className="min-h-8 gap-2 rounded-md text-body-compact"
                                            onClick={() =>
                                                onDisableTarget(selected)
                                            }
                                        >
                                            <BanIcon className="size-3.5" />
                                            停用分类
                                        </DropdownMenuItem>
                                    </PermissionGuard>
                                </DropdownMenuContent>
                            </DropdownMenu>
                        </div>
                    </div>

                    <dl className="grid gap-x-6 gap-y-3 rounded-lg border border-border/55 bg-surface-sunken/75 px-4 py-3 sm:grid-cols-2 @[1120px]/workspace:grid-cols-4">
                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                分类代码
                            </dt>
                            <dd className="flex items-center gap-1 text-body-compact font-medium text-foreground">
                                <span
                                    className="num truncate"
                                    title={selectedCode}
                                >
                                    {selectedCode}
                                </span>
                                <CopyCodeButton
                                    id={`${prefix}-copy-code`}
                                    code={selectedCode}
                                />
                            </dd>
                        </div>
                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                上级分类
                            </dt>
                            <dd className="min-w-0 text-body-compact text-foreground">
                                {selected.parentStableId ? (
                                    parentItem ? (
                                        <Button
                                            id={`${prefix}-parent-link`}
                                            type="button"
                                            variant="link"
                                            size="xs"
                                            className="h-auto max-w-full truncate p-0 text-body-compact font-medium text-primary"
                                            onClick={() =>
                                                onSelect(parentItem.stableId)
                                            }
                                        >
                                            {parentItem.name}
                                        </Button>
                                    ) : (
                                        <span className="text-muted-foreground">
                                            上级分类不可用
                                        </span>
                                    )
                                ) : (
                                    <span className="text-muted-foreground">
                                        无（一级分类）
                                    </span>
                                )}
                            </dd>
                        </div>
                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                适用商品类型
                            </dt>
                            <dd className="pt-0.5 text-body-compact">
                                <ProductKindBadge kind={selected.productKind} />
                            </dd>
                        </div>
                        <div className="min-w-0 space-y-0.5">
                            <dt className="text-xs text-muted-foreground">
                                下级结构
                            </dt>
                            <dd className="num text-body-compact text-foreground">
                                {directChildCount > 0 ? (
                                    <>
                                        <span className="font-medium">
                                            {directChildCount}
                                        </span>{" "}
                                        <span className="text-xs text-muted-foreground">
                                            个直接子分类
                                            {descendantCount > directChildCount
                                                ? `（共 ${descendantCount} 个下级）`
                                                : ""}
                                        </span>
                                    </>
                                ) : (
                                    <span className="text-xs text-muted-foreground">
                                        末级分类，暂无下级
                                    </span>
                                )}
                            </dd>
                        </div>
                    </dl>

                    {selected.primaryBlocker ? (
                        <FormalActionResult
                            status="blocked"
                            title="当前操作受限"
                            description={selected.primaryBlocker}
                        />
                    ) : null}

                    {filterActive ? (
                        <div className="flex flex-wrap items-center justify-between gap-2 rounded-md border border-primary/15 bg-sidebar-accent/55 px-3 py-2 text-xs text-sidebar-accent-foreground">
                            <span>
                                {matchedIds.has(selected.stableId)
                                    ? "当前处于筛选模式，下方仍展示该分类的全部直接子分类。"
                                    : "当前查看的分类未命中筛选条件，资料仍保留显示。"}
                            </span>
                            <Button
                                id={`${prefix}-reset`}
                                type="button"
                                variant="ghost"
                                size="xs"
                                className="h-auto px-1.5 py-0.5 text-xs font-medium text-primary hover:bg-primary/10"
                                onClick={onClearFilters}
                            >
                                清除筛选
                            </Button>
                        </div>
                    ) : null}
                </div>
            ) : (
                <div className="flex shrink-0 flex-wrap items-center justify-between gap-3 border-b border-border/60 px-4 py-3.5 lg:px-6">
                    <div className="flex min-w-0 flex-wrap items-baseline gap-x-3 gap-y-1">
                        <h2 className="text-section-title font-semibold tracking-tight text-foreground">
                            {filterActive ? "分类查询结果" : "一级分类"}
                        </h2>
                        <DetailWorkspaceMeta
                            items={
                                filterActive
                                    ? [
                                          <span key="matched" className="num">
                                              匹配 {nodes.length} 个分类
                                          </span>,
                                      ]
                                    : [
                                          <span key="roots" className="num">
                                              一级 {rootCount} 个
                                          </span>,
                                          <span key="subs" className="num">
                                              子分类 {subCategoryCount} 个
                                          </span>,
                                          <span key="total" className="num">
                                              共 {rows.length} 个分类
                                          </span>,
                                      ]
                            }
                        />
                    </div>
                    {filterActive ? (
                        <Button
                            id={`${prefix}-reset`}
                            type="button"
                            variant="ghost"
                            size="xs"
                            className="text-xs font-medium text-primary hover:bg-sidebar-accent/70"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : null}
                </div>
            )}

            {selected ? (
                <div className="flex shrink-0 items-center justify-between gap-3 px-4 pt-4 pb-2 lg:px-6">
                    <div className="flex items-baseline gap-2">
                        <h3 className="font-semibold text-foreground">
                            直接子分类
                        </h3>
                        <span className="num text-xs text-muted-foreground">
                            {nodes.length}
                        </span>
                    </div>
                </div>
            ) : null}

            {nodes.length ? (
                <div className="min-w-0 flex-1">
                    <Table data-density="comfortable">
                        <TableHeader>
                            <TableRow className="border-b border-border/80">
                                <TableHead className="w-full min-w-52 pl-4 lg:pl-6">
                                    分类名称 / 代码
                                </TableHead>
                                <TableHead className="w-24">适用类型</TableHead>
                                <TableHead className="w-28">启用状态</TableHead>
                                <TableHead
                                    data-align="end"
                                    className="w-24 text-right"
                                >
                                    下级分类
                                </TableHead>
                                <TableHead
                                    data-align="end"
                                    className="w-40 text-right"
                                >
                                    操作
                                </TableHead>
                                <TableHead
                                    data-align="end"
                                    className="w-10 pl-1 pr-4 lg:pr-6"
                                >
                                    <span className="sr-only">进入分类</span>
                                </TableHead>
                            </TableRow>
                        </TableHeader>
                        <TableBody>
                            {nodes.map((node) => {
                                const seg = toAutomationIdSegment(
                                    node.item.stableId,
                                )
                                const code =
                                    node.item.dictionaryCode ??
                                    node.item.stableNo
                                const showPath =
                                    filterActive && !selected && node.depth > 0
                                const secondary = showPath
                                    ? `${node.pathLabel} · ${code}`
                                    : code
                                const childCount = node.children.length
                                const totalDescendants =
                                    countCategoryDescendants(node)
                                const rowActions = buildRowActions(node)

                                return (
                                    <TableRow
                                        key={node.item.stableId}
                                        className="cursor-pointer border-b border-grid/70"
                                        onClick={() =>
                                            onSelect(node.item.stableId)
                                        }
                                    >
                                        <TableCell className="min-w-52 max-w-0 pl-4 lg:pl-6">
                                            <div className="flex min-w-0 items-center gap-2.5">
                                                <span
                                                    aria-hidden="true"
                                                    className={cn(
                                                        "flex size-7 shrink-0 items-center justify-center rounded-md transition-colors",
                                                        node.item
                                                            .lifecycleStatus ===
                                                            "DISABLED"
                                                            ? "bg-muted/70 text-muted-foreground/70"
                                                            : childCount > 0
                                                              ? "bg-sidebar-accent/70 text-primary"
                                                              : "bg-muted/75 text-muted-foreground/80 group-hover/row:bg-sidebar-accent/60 group-hover/row:text-primary",
                                                    )}
                                                >
                                                    {childCount > 0 ? (
                                                        <FolderOpenIcon className="size-3.5" />
                                                    ) : (
                                                        <FolderIcon className="size-3.5" />
                                                    )}
                                                </span>
                                                <button
                                                    id={`${prefix}-category-${seg}`}
                                                    type="button"
                                                    className="min-w-0 flex-1 cursor-pointer rounded-sm text-left outline-none focus-visible:ring-2 focus-visible:ring-ring"
                                                    onClick={(event) => {
                                                        event.stopPropagation()
                                                        onSelect(
                                                            node.item.stableId,
                                                        )
                                                    }}
                                                >
                                                    <TableIdentityCell
                                                        primary={
                                                            <span className="transition-colors group-hover/row:text-primary">
                                                                <CategoryName
                                                                    name={
                                                                        node
                                                                            .item
                                                                            .name
                                                                    }
                                                                    query={
                                                                        query
                                                                    }
                                                                />
                                                            </span>
                                                        }
                                                        primaryTitle={
                                                            node.item.name
                                                        }
                                                        secondary={secondary}
                                                        secondaryTitle={
                                                            secondary
                                                        }
                                                        numericSecondary
                                                    />
                                                </button>
                                            </div>
                                        </TableCell>
                                        <TableCell>
                                            <ProductKindBadge
                                                kind={node.item.productKind}
                                            />
                                        </TableCell>
                                        <TableCell>
                                            <BusinessStatusBadge
                                                context="list"
                                                label={
                                                    node.item
                                                        .lifecycleStatusLabel
                                                }
                                                tone={node.item.lifecycleTone}
                                                appearance={
                                                    node.item.lifecycleTone ===
                                                    "success"
                                                        ? "quiet"
                                                        : "badge"
                                                }
                                            />
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            title={
                                                totalDescendants > childCount
                                                    ? `直接子分类 ${childCount} 个，全部下级 ${totalDescendants} 个`
                                                    : undefined
                                            }
                                        >
                                            {childCount > 0 ? (
                                                <QuantityValue
                                                    value={String(childCount)}
                                                    unit="个"
                                                />
                                            ) : (
                                                <TableEmptyCell />
                                            )}
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            onClick={(event) =>
                                                event.stopPropagation()
                                            }
                                        >
                                            <TableRowActions
                                                actions={rowActions}
                                                moreId={`${prefix}-row-${seg}-more`}
                                                moreLabel={`${node.item.name} 更多操作`}
                                                maxInline={2}
                                            />
                                        </TableCell>
                                        <TableCell
                                            data-align="end"
                                            className="pl-1 pr-4 lg:pr-6"
                                        >
                                            <ChevronRightIcon
                                                aria-hidden="true"
                                                className="ml-auto size-4 text-muted-foreground/50 opacity-0 transition-[opacity,transform,color] duration-150 group-focus-within/row:text-primary group-focus-within/row:opacity-100 group-hover/row:translate-x-0.5 group-hover/row:text-primary group-hover/row:opacity-100 [@media(hover:none)]:opacity-50"
                                            />
                                        </TableCell>
                                    </TableRow>
                                )
                            })}
                        </TableBody>
                    </Table>
                </div>
            ) : (
                <div className="flex flex-1 items-center justify-center px-6 py-12">
                    <BusinessEmptyState
                        kind={filterActive && !selected ? "filter" : "no-data"}
                        icon={FolderTreeIcon}
                        title={
                            selected
                                ? "暂无子分类"
                                : filterActive
                                  ? "没有匹配的分类"
                                  : "还没有商品分类"
                        }
                        description={
                            selected
                                ? "可在当前分类下继续细分，建立清晰的商品归属层级。"
                                : filterActive
                                  ? "尝试其他分类名称或代码，或清除筛选条件后重试。"
                                  : "先新建一级分类，再按业务需要逐级维护子分类。"
                        }
                        className={listWorkspaceEmptyStateClassName}
                        action={
                            filterActive && !selected ? (
                                <Button
                                    id={`${prefix}-empty-reset`}
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={onClearFilters}
                                >
                                    清除筛选
                                </Button>
                            ) : (selected ? canAddChild : canCreate) ? (
                                <Button
                                    id={`${prefix}-empty-create`}
                                    type="button"
                                    variant="secondary"
                                    size="sm"
                                    className="rounded-lg shadow-none"
                                    onClick={() =>
                                        selected
                                            ? onOpenCreateChild(selected)
                                            : onOpenCreateRoot()
                                    }
                                >
                                    <PlusIcon
                                        aria-hidden="true"
                                        className="size-3.5"
                                    />
                                    {selected ? "添加子分类" : "新建分类"}
                                </Button>
                            ) : undefined
                        }
                    />
                </div>
            )}
        </section>
    )
}
