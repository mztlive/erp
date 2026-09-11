"use client"

import Link from "next/link"
import {
    ArrowUpRightIcon,
    ChevronRightIcon,
    FolderTreeIcon,
    MoreHorizontalIcon,
    PlusIcon,
    PencilIcon,
} from "lucide-react"
import { BusinessStatusBadge, FormalActionResult } from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    DropdownMenu,
    DropdownMenuContent,
    DropdownMenuItem,
    DropdownMenuSeparator,
    DropdownMenuTrigger,
} from "@/components/ui/dropdown-menu"
import { CategoryName } from "./category-tree-list"
import type { CategoryTreeNode } from "@/features/master-data/lib/category-tree-model"
import type { MasterDataListItem } from "@/features/master-data/types"
import { toAutomationIdSegment } from "@/lib/automation-id"

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

/** 分类工作区：对象资料、明确动作与直接子分类；筛选结果保留完整路径。 */
export function CategoryTreeDetailPanel({
    selected,
    selectedId,
    selectedNode,
    rows,
    nodes,
    filterActive,
    matchedIds,
    query,
    returnTo,
    canCreate,
    createBlockedReason,
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
    nodes: readonly CategoryTreeNode[]
    filterActive: boolean
    matchedIds: ReadonlySet<string>
    query: string
    returnTo: string
    canCreate: boolean
    createBlockedReason: string
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
    if (selectedId && !selected)
        return (
            <section className="px-6 py-12 text-sm text-muted-foreground">
                <p>该分类已不可用，请重新选择。</p>
                <Button
                    id={`${prefix}-missing-back`}
                    className="mt-4"
                    variant="outline"
                    onClick={() => onSelect(null)}
                >
                    查看全部分类
                </Button>
            </section>
        )
    return (
        <section
            aria-label="分类管理工作区"
            className="min-w-0 px-1 py-2 lg:px-7 lg:py-5"
        >
            <nav
                aria-label="分类路径"
                className="mb-5 flex flex-wrap items-center gap-1 text-xs text-muted-foreground"
            >
                <Button
                    id={`${prefix}-all`}
                    variant="ghost"
                    className="h-auto px-0 py-1 text-xs text-muted-foreground"
                    onClick={() => onSelect(null)}
                >
                    全部分类
                </Button>
                {ancestors.map((item) => (
                    <span
                        key={item.stableId}
                        className="flex items-center gap-1"
                    >
                        <ChevronRightIcon className="size-3" />
                        <Button
                            id={`${prefix}-ancestor-${toAutomationIdSegment(item.stableId)}`}
                            variant="ghost"
                            className="h-auto px-1 py-1 text-xs text-muted-foreground"
                            onClick={() => onSelect(item.stableId)}
                        >
                            {item.name}
                        </Button>
                    </span>
                ))}
                {selected ? (
                    <>
                        <ChevronRightIcon className="size-3" />
                        <span
                            aria-current="page"
                            className="break-all text-foreground"
                        >
                            {selected.name}
                        </span>
                    </>
                ) : null}
            </nav>
            <div className="flex flex-wrap items-start justify-between gap-4">
                <div className="min-w-0 space-y-2">
                    <div className="flex flex-wrap items-center gap-3">
                        <h2 className="text-2xl font-semibold tracking-tight break-words">
                            {selected?.name ??
                                (filterActive ? "分类查询结果" : "全部分类")}
                        </h2>
                        {selected ? (
                            <BusinessStatusBadge
                                context="detail"
                                label={selected.lifecycleStatusLabel}
                                tone={selected.lifecycleTone}
                            />
                        ) : null}
                    </div>
                    <p className="text-sm text-muted-foreground">
                        {selected
                            ? `直接子分类 ${selectedNode?.children.length ?? 0} 个`
                            : filterActive
                              ? `匹配 ${nodes.length} 个分类，可选择分类查看资料`
                              : `共 ${rows.length} 个分类，从一级分类开始浏览和维护`}
                    </p>
                </div>
                {selected ? (
                    <div className="flex flex-wrap items-center gap-2">
                        <span title={!canEdit ? editBlocker : undefined}>
                            <Button
                                id={`${prefix}-edit`}
                                size="sm"
                                disabled={!canEdit}
                                onClick={() => onReviseTarget(selected)}
                            >
                                <PencilIcon className="size-3.5" />
                                编辑分类
                            </Button>
                        </span>
                        <span
                            title={
                                !canAddChild
                                    ? !canCreate
                                        ? createBlockedReason
                                        : editBlocker
                                    : undefined
                            }
                        >
                            <Button
                                id={`${prefix}-create-child`}
                                variant="outline"
                                size="sm"
                                disabled={!canAddChild}
                                onClick={() => onOpenCreateChild(selected)}
                            >
                                <PlusIcon className="size-3.5" />
                                添加子分类
                            </Button>
                        </span>
                        <DropdownMenu>
                            <DropdownMenuTrigger
                                id={`${prefix}-more`}
                                render={
                                    <Button
                                        variant="ghost"
                                        size="icon-sm"
                                        aria-label="更多分类操作"
                                    />
                                }
                            >
                                <MoreHorizontalIcon />
                            </DropdownMenuTrigger>
                            <DropdownMenuContent
                                align="end"
                                className="min-w-44"
                            >
                                <DropdownMenuItem
                                    id={`${prefix}-move`}
                                    disabled={!canEdit}
                                    title={!canEdit ? editBlocker : undefined}
                                    onClick={() => onMoveTarget(selected)}
                                >
                                    调整上级
                                </DropdownMenuItem>
                                <DropdownMenuItem
                                    id={`${prefix}-detail`}
                                    render={
                                        <Link
                                            href={`/master-data/categories/${selected.stableId}?section=overview&returnTo=${encodeURIComponent(returnTo)}`}
                                        />
                                    }
                                >
                                    打开完整资料
                                    <ArrowUpRightIcon className="ml-auto size-3.5" />
                                </DropdownMenuItem>
                                <DropdownMenuSeparator />
                                <DropdownMenuItem
                                    id={`${prefix}-disable`}
                                    disabled={!canDisable}
                                    title={
                                        !canDisable ? disableBlocker : undefined
                                    }
                                    onClick={() => onDisableTarget(selected)}
                                >
                                    停用分类
                                </DropdownMenuItem>
                            </DropdownMenuContent>
                        </DropdownMenu>
                    </div>
                ) : null}
            </div>
            {selected ? (
                <dl className="mt-6 grid gap-x-8 gap-y-5 border-y border-border py-5 sm:grid-cols-3">
                    <div className="space-y-1.5">
                        <dt className="text-xs text-muted-foreground">
                            分类代码
                        </dt>
                        <dd className="num break-all text-sm">
                            {selected.dictionaryCode ?? selected.stableNo}
                        </dd>
                    </div>
                    <div className="space-y-1.5">
                        <dt className="text-xs text-muted-foreground">
                            上级分类
                        </dt>
                        <dd className="text-sm">
                            {selected.parentStableId
                                ? (rows.find(
                                      (item) =>
                                          item.stableId ===
                                          selected.parentStableId,
                                  )?.name ?? "上级分类不可用")
                                : "无（一级分类）"}
                        </dd>
                    </div>
                    <div className="space-y-1.5">
                        <dt className="text-xs text-muted-foreground">
                            适用商品类型
                        </dt>
                        <dd className="text-sm">
                            {selected.productKind ?? "未填写"}
                        </dd>
                    </div>
                </dl>
            ) : null}
            {selected?.primaryBlocker ? (
                <div className="mt-4">
                    <FormalActionResult
                        status="blocked"
                        title="当前操作受限"
                        description={selected.primaryBlocker}
                    />
                </div>
            ) : null}
            {selected && filterActive ? (
                <div className="mt-4 flex flex-wrap items-center justify-between gap-2 text-xs text-muted-foreground">
                    <span>
                        {matchedIds.has(selected.stableId)
                            ? "左侧按条件筛选，此处展示该分类的全部直接子分类。"
                            : "当前查看的分类未匹配筛选条件，资料仍保留显示。"}
                    </span>
                    <Button
                        id={`${prefix}-reset`}
                        variant="ghost"
                        size="sm"
                        onClick={onClearFilters}
                    >
                        清除筛选
                    </Button>
                </div>
            ) : null}
            <div className="mt-7 flex items-center justify-between gap-3">
                <h3 className="text-sm font-semibold">
                    {selected
                        ? "直接子分类"
                        : filterActive
                          ? "匹配分类"
                          : "一级分类"}
                    <span className="num ml-2 font-normal text-muted-foreground">
                        {nodes.length}
                    </span>
                </h3>
            </div>
            {nodes.length ? (
                <div className="mt-3">
                    <div className="hidden grid-cols-[minmax(0,1fr)_7rem_5rem_3rem_1rem] gap-4 border-b border-border px-3 py-3 text-xs text-muted-foreground sm:grid">
                        <span>分类名称 / 代码</span>
                        <span>适用类型</span>
                        <span>状态</span>
                        <span className="text-right">子分类</span>
                        <span />
                    </div>
                    <ul className="divide-y divide-border">
                        {nodes.map((node) => (
                            <li key={node.item.stableId}>
                                <button
                                    id={`${prefix}-category-${toAutomationIdSegment(node.item.stableId)}`}
                                    type="button"
                                    className="group grid w-full grid-cols-[minmax(0,1fr)_auto] items-center gap-4 rounded-lg px-3 py-4 text-left outline-none hover:bg-muted/50 focus-visible:ring-2 focus-visible:ring-ring sm:grid-cols-[minmax(0,1fr)_7rem_5rem_3rem_1rem]"
                                    onClick={() => onSelect(node.item.stableId)}
                                >
                                    <span className="min-w-0">
                                        <span
                                            className="block truncate text-sm font-medium"
                                            title={node.item.name}
                                        >
                                            <CategoryName
                                                name={node.item.name}
                                                query={query}
                                            />
                                        </span>
                                        <span
                                            className="num mt-1 block truncate text-xs text-muted-foreground"
                                            title={
                                                filterActive &&
                                                !selected &&
                                                node.depth > 0
                                                    ? node.pathLabel
                                                    : (node.item
                                                          .dictionaryCode ??
                                                      node.item.stableNo)
                                            }
                                        >
                                            {filterActive &&
                                            !selected &&
                                            node.depth > 0
                                                ? node.pathLabel
                                                : (node.item.dictionaryCode ??
                                                  node.item.stableNo)}
                                        </span>
                                        <span className="mt-1 block text-xs text-muted-foreground sm:hidden">
                                            {node.item.productKind ?? "未填写"}{" "}
                                            · {node.item.lifecycleStatusLabel} ·{" "}
                                            {node.children.length} 个子分类
                                        </span>
                                    </span>
                                    <span className="hidden text-sm text-muted-foreground sm:block">
                                        {node.item.productKind ?? "未填写"}
                                    </span>
                                    <span className="hidden sm:block">
                                        <BusinessStatusBadge
                                            context="list"
                                            label={
                                                node.item.lifecycleStatusLabel
                                            }
                                            tone={node.item.lifecycleTone}
                                        />
                                    </span>
                                    <span className="num hidden text-right text-sm text-muted-foreground sm:block">
                                        {node.children.length}
                                    </span>
                                    <ChevronRightIcon className="size-4 text-muted-foreground" />
                                </button>
                            </li>
                        ))}
                    </ul>
                </div>
            ) : (
                <div className="flex flex-col items-center gap-3 py-14 text-center">
                    <FolderTreeIcon className="size-8 text-muted-foreground/60" />
                    <p className="text-sm font-medium">
                        {selected
                            ? "暂无子分类"
                            : filterActive
                              ? "没有匹配的分类"
                              : "开始建立商品分类"}
                    </p>
                    <p className="max-w-sm text-sm text-muted-foreground">
                        {selected
                            ? "可在当前分类下继续细分，建立清晰的商品归属。"
                            : filterActive
                              ? "尝试其他名称、代码，或清除筛选条件。"
                              : "先新建一级分类，再按业务需要添加子分类。"}
                    </p>
                    {filterActive && !selected ? (
                        <Button
                            id={`${prefix}-empty-reset`}
                            variant="outline"
                            size="sm"
                            onClick={onClearFilters}
                        >
                            清除筛选
                        </Button>
                    ) : (selected ? canAddChild : canCreate) ? (
                        <Button
                            id={`${prefix}-empty-create`}
                            variant="outline"
                            size="sm"
                            onClick={() =>
                                selected
                                    ? onOpenCreateChild(selected)
                                    : onOpenCreateRoot()
                            }
                        >
                            <PlusIcon className="size-4" />
                            {selected ? "添加子分类" : "新建分类"}
                        </Button>
                    ) : null}
                </div>
            )}
        </section>
    )
}
