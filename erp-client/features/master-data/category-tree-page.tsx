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
  FolderTreeIcon,
  HistoryIcon,
  PlusIcon,
  SearchIcon,
} from "lucide-react"

import {
  BusinessStatusBadge,
  DataFreshness,
  FormalActionResult,
  PageActions,
  PageHeader,
} from "@/components/business"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
  InputGroup,
  InputGroupAddon,
  InputGroupInput,
} from "@/components/ui/input-group"
import { cn } from "@/lib/utils"
import {
  buildCategoryForest,
  flattenCategoryForest,
  type CategoryTreeNode,
} from "@/features/master-data/category-tree-model"
import { masterDataCopy } from "@/features/master-data/copy"
import {
  MasterDataCreateDialog,
  MasterDataDisableDialog,
  MasterDataReviseDialog,
} from "@/features/master-data/master-data-action-dialog"
import { useMasterDataListQuery } from "@/features/master-data/queries"
import {
  MASTER_DATA_RESOURCES,
  type MasterDataListItem,
} from "@/features/master-data/types"

function ResourceNav({
  resource,
  navRef,
}: {
  resource: string
  navRef: React.RefObject<HTMLElement | null>
}) {
  return (
    <nav
      ref={navRef}
      aria-label={masterDataCopy.resourceNavAria}
      role="tablist"
      className="flex flex-wrap gap-2 border-b border-border pb-3"
    >
      {MASTER_DATA_RESOURCES.map((item) => {
        const selected = item.key === resource
        return (
          <Button
            key={item.key}
            size="sm"
            role="tab"
            aria-selected={selected}
            variant={selected ? "secondary" : "ghost"}
            render={<Link href={`/master-data/${item.key}`} />}
          >
            {item.label}
          </Button>
        )
      })}
    </nav>
  )
}

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
          selected && "border-border bg-muted"
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
        <span className="num shrink-0 text-xs text-muted-foreground">{code}</span>
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

export function CategoryTreePage({
  navRef,
}: {
  navRef: React.RefObject<HTMLElement | null>
}) {
  const [search, setSearch] = React.useState("")
  const [lifecycleStatus, setLifecycleStatus] = React.useState<
    "enabled" | "disabled" | "all"
  >("all")
  const [selectedId, setSelectedId] = React.useState<string | null>(null)
  const [expanded, setExpanded] = React.useState<Set<string>>(() => new Set())
  const [createOpen, setCreateOpen] = React.useState(false)
  const [createParentId, setCreateParentId] = React.useState<string | undefined>()
  const [reviseTarget, setReviseTarget] =
    React.useState<MasterDataListItem | null>(null)
  const [disableTarget, setDisableTarget] =
    React.useState<MasterDataListItem | null>(null)

  const listQuery = useMasterDataListQuery({
    resource: "categories",
    q: search,
    lifecycleStatus,
    revisionTiming: "all",
  })

  const rows = listQuery.data?.rows ?? []
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
    flat.find((n) => n.item.stableId === selectedId)?.pathLabel ?? selected?.name

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
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={masterDataCopy.pageTitle("商品分类")} />
        <ResourceNav resource="categories" navRef={navRef} />
        <div className="h-40 animate-pulse rounded-lg bg-muted" aria-busy />
      </div>
    )
  }

  if (listQuery.isError || !listQuery.data) {
    return (
      <div className="mx-auto flex w-full max-w-shell flex-col gap-4 p-4 md:p-5">
        <PageHeader title={masterDataCopy.pageTitle("商品分类")} />
        <ResourceNav resource="categories" navRef={navRef} />
        <Button type="button" onClick={() => void listQuery.refetch()}>
          重试
        </Button>
      </div>
    )
  }

  return (
    <div className="mx-auto flex w-full max-w-shell flex-col gap-3 p-3 md:gap-3.5 md:p-4">
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
                actionKey: "create-root",
                label: masterDataCopy.categoryAddRoot,
                icon: PlusIcon,
                onClick: openCreateRoot,
              },
            ]}
          />
        }
      />

      <ResourceNav resource="categories" navRef={navRef} />

      <p className="text-sm text-muted-foreground">
        {masterDataCopy.categoryTreeDesc(rows.length)}
      </p>

      <div className="flex flex-wrap items-center gap-2">
        <InputGroup className="max-w-xs">
          <InputGroupAddon>
            <SearchIcon aria-hidden />
          </InputGroupAddon>
          <InputGroupInput
            value={search}
            onChange={(e) => setSearch(e.target.value)}
            placeholder={masterDataCopy.categoryTreeSearch}
            aria-label={masterDataCopy.categoryTreeSearch}
          />
        </InputGroup>
        <div className="flex gap-1">
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
              variant={lifecycleStatus === value ? "secondary" : "ghost"}
              onClick={() => setLifecycleStatus(value)}
            >
              {label}
            </Button>
          ))}
        </div>
        <div className="ml-auto flex gap-1">
          <Button type="button" size="sm" variant="ghost" onClick={expandAll}>
            {masterDataCopy.categoryExpandAll}
          </Button>
          <Button type="button" size="sm" variant="ghost" onClick={collapseAll}>
            {masterDataCopy.categoryCollapseAll}
          </Button>
        </div>
      </div>

      <div className="grid min-h-[28rem] gap-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
        <section
          className="flex min-h-0 flex-col rounded-xl border border-border bg-card shadow-sm"
          aria-label={masterDataCopy.categoryTreeTitle}
        >
          <div className="flex items-center justify-between border-b border-border px-3 py-2">
            <h2 className="text-sm font-semibold">
              {masterDataCopy.categoryTreeTitle}
            </h2>
            <span className="text-xs text-muted-foreground">
              {rows.length} 项
            </span>
          </div>
          <div className="min-h-0 flex-1 overflow-y-auto p-2">
            {forest.length === 0 ? (
              <div className="flex flex-col items-center gap-3 py-12 text-center">
                <p className="text-sm text-muted-foreground">
                  {masterDataCopy.categoryTreeEmpty}
                </p>
                <Button type="button" size="sm" onClick={openCreateRoot}>
                  <PlusIcon data-icon="inline-start" aria-hidden />
                  {masterDataCopy.categoryAddRoot}
                </Button>
              </div>
            ) : (
              <ul role="tree" className="m-0 list-none p-0">
                {forest.map((node) => (
                  <TreeRow
                    key={node.item.stableId}
                    node={node}
                    expanded={expanded}
                    selectedId={selectedId}
                    onToggle={toggle}
                    onSelect={(item) => setSelectedId(item.stableId)}
                  />
                ))}
              </ul>
            )}
          </div>
        </section>

        <section
          className="flex min-h-0 flex-col rounded-xl border border-border bg-card shadow-sm"
          aria-label="分类详情"
        >
          <div className="border-b border-border px-3 py-2">
            <h2 className="text-sm font-semibold">分类详情</h2>
          </div>
          <div className="min-h-0 flex-1 space-y-4 overflow-y-auto p-4">
            {!selected ? (
              <p className="text-sm text-muted-foreground">
                在左侧选择一个分类，查看路径、版本并执行维护。
              </p>
            ) : (
              <>
                <div className="space-y-1">
                  <div className="text-lg font-semibold">{selected.name}</div>
                  <div className="num text-sm text-muted-foreground">
                    {selected.stableNo} · v{selected.revisionNo}
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
                        selected.keyFacts.find((f) => f.label === "分类代码")
                          ?.value ??
                        "—"}
                    </dd>
                  </div>
                  <div>
                    <dt className="text-xs text-muted-foreground">
                      {masterDataCopy.categoryColParent}
                    </dt>
                    <dd className="font-medium">
                      {selected.parentStableId
                        ? (rows.find((r) => r.stableId === selected.parentStableId)
                            ?.name ?? "—")
                        : masterDataCopy.categoryParentRoot}
                    </dd>
                  </div>
                  <div className="sm:col-span-2">
                    <dt className="text-xs text-muted-foreground">
                      {masterDataCopy.categoryColKind}
                    </dt>
                    <dd className="font-medium">
                      {selected.productKind ??
                        selected.keyFacts.find((f) => f.label === "适用商品类型")
                          ?.value ??
                        "—"}
                    </dd>
                  </div>
                </dl>
                <div className="flex flex-wrap gap-2 border-t border-border pt-3">
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={
                      !selected.allowedActions.includes("CREATE_REVISION")
                    }
                    onClick={() => openCreateChild(selected)}
                  >
                    <PlusIcon data-icon="inline-start" aria-hidden />
                    {masterDataCopy.categoryAddChild}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={
                      !selected.allowedActions.includes("CREATE_REVISION")
                    }
                    onClick={() => setReviseTarget(selected)}
                  >
                    <HistoryIcon data-icon="inline-start" aria-hidden />
                    {masterDataCopy.actionUpdate}
                  </Button>
                  <Button
                    type="button"
                    size="sm"
                    variant="outline"
                    disabled={!selected.allowedActions.includes("DISABLE")}
                    onClick={() => setDisableTarget(selected)}
                  >
                    <BanIcon data-icon="inline-start" aria-hidden />
                    {masterDataCopy.actionDisable}
                  </Button>
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
    </div>
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
