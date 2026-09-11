"use client"

import * as React from "react"
import { DownloadIcon, FolderTreeIcon, PlusIcon } from "lucide-react"
import {
    BusinessFailureState,
    PageActions,
    PageScaffold,
} from "@/components/business"
import {
    ListWorkspaceHeader,
    listWorkspaceStyles as styles,
} from "@/components/business/list-workspace"
import { Button } from "@/components/ui/button"
import {
    Sheet,
    SheetContent,
    SheetHeader,
    SheetTitle,
    SheetDescription,
} from "@/components/ui/sheet"
import {
    CategoryCreateDialog,
    CategoryReviseDialog,
} from "@/features/master-data/components/category/category-form-dialogs"
import { CategoryTreeDetailPanel } from "@/features/master-data/components/category/category-tree-detail-panel"
import { CategoryTreeList } from "@/features/master-data/components/category/category-tree-list"
import { CategoryTreeToolbar } from "@/features/master-data/components/category/category-tree-toolbar"
import { CategoryDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useMasterDataCategoryTree } from "@/features/master-data/hooks/use-master-data-category-tree"

/** 商品分类：紧凑导航与分类管理工作区；移动端通过抽屉选择分类。 */
export function CategoryTreePage() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const mobileSearchRef = React.useRef<HTMLInputElement | null>(null)
    const state = useMasterDataCategoryTree(searchInputRef)
    const workspaceRef = React.useRef<HTMLDivElement | null>(null)
    const workspaceScrollKey = `category-workspace-scroll:${state.selectedId ?? "all"}:${state.q}:${state.lifecycleStatus}`
    React.useEffect(() => {
        const node = workspaceRef.current
        if (!node) return
        try {
            node.scrollTo({
                top: Number(sessionStorage.getItem(workspaceScrollKey) ?? 0),
            })
        } catch {
            /* 本地偏好不可用时从顶部浏览。 */
        }
    }, [workspaceScrollKey])
    const [navigationOpen, setNavigationOpen] = React.useState(false)
    const failed = state.fullQuery.isError || state.listQuery.isError
    const pending = state.fullQuery.isPending || state.listQuery.isPending
    const select = (id: string | null) => {
        state.setSelectedId(id)
        setNavigationOpen(false)
    }
    const nodes = state.selectedNode
        ? state.selectedNode.children
        : state.filterActive
          ? state.flat.filter((node) =>
                state.matchedIds.has(node.item.stableId),
            )
          : state.fullForest
    const navigation = (mobile: boolean) => {
        const prefix = mobile ? "category-mobile-nav" : "category-nav"
        return (
            <>
                <CategoryTreeToolbar
                    idPrefix={`${prefix}-toolbar`}
                    searchInputRef={mobile ? mobileSearchRef : searchInputRef}
                    searchDraft={state.searchDraft}
                    setSearchDraft={state.setSearchDraft}
                    applyTreeFilters={state.applyTreeFilters}
                    lifecycleStatus={state.lifecycleStatus}
                    onLifecycleStatusChange={state.setLifecycleStatus}
                    clearFilters={state.clearFilters}
                    clearSearch={state.clearSearch}
                    filterActive={state.filterActive}
                    hasPendingChanges={state.hasPendingChanges}
                    loading={state.listQuery.isFetching}
                />
                <Button
                    id={`${prefix}-all`}
                    variant="ghost"
                    className={`mb-3 w-full justify-start gap-2 ${!state.selectedId ? "bg-muted font-semibold" : ""}`}
                    aria-pressed={!state.selectedId}
                    onClick={() => select(null)}
                >
                    <FolderTreeIcon className="size-4" />
                    {state.filterActive ? "全部匹配分类" : "全部分类"}
                    <span className="num ml-auto text-xs text-muted-foreground">
                        {state.filterActive
                            ? state.matchedRows.length
                            : state.rows.length}
                    </span>
                </Button>
                <div className="mb-2 flex items-center justify-between border-t border-border pt-3">
                    <span className="text-xs font-medium text-muted-foreground">
                        分类目录
                    </span>
                    <div className="flex gap-1">
                        <Button
                            id={`${prefix}-expand`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="h-7 px-2 text-xs"
                            onClick={state.expandAll}
                        >
                            展开
                        </Button>
                        <Button
                            id={`${prefix}-collapse`}
                            type="button"
                            size="sm"
                            variant="ghost"
                            className="h-7 px-2 text-xs"
                            onClick={state.collapseAll}
                        >
                            收起
                        </Button>
                    </div>
                </div>
                {pending ? (
                    <p
                        role="status"
                        className="py-8 text-center text-sm text-muted-foreground"
                    >
                        正在加载分类…
                    </p>
                ) : failed ? (
                    <p className="py-8 text-center text-sm text-muted-foreground">
                        分类加载失败，请重试。
                    </p>
                ) : (
                    <CategoryTreeList
                        idPrefix={`${prefix}-tree`}
                        forest={state.forest}
                        expanded={state.expanded}
                        selectedId={state.selectedId}
                        onToggle={state.toggle}
                        onSelect={(item) => select(item.stableId)}
                        matchedIds={state.matchedIds}
                        filterActive={state.filterActive}
                        query={state.q.trim()}
                        onClearFilters={state.clearFilters}
                        onOpenCreateRoot={() => {
                            setNavigationOpen(false)
                            state.openCreateRoot()
                        }}
                        canCreate={state.canCreate}
                    />
                )}
            </>
        )
    }
    return (
        <PageScaffold
            density="compact"
            className={`${styles.page} lg:min-h-0 lg:overflow-hidden`}
        >
            <ListWorkspaceHeader
                className="shrink-0"
                eyebrow="基础资料"
                title="商品分类"
                description="维护分类资料与上下级，建立清晰的商品归属。"
            >
                <PageActions
                    actions={[
                        {
                            id: "master-data-category-tree-export",
                            actionKey: "export",
                            label: "导出",
                            icon: DownloadIcon,
                            variant: "outline",
                            mobileVisibility: "hide",
                            disabled:
                                failed ||
                                state.listQuery.isFetching ||
                                !state.matchedRows.length,
                            onClick: state.onExport,
                        },
                        {
                            id: "master-data-category-tree-create-root",
                            actionKey: "create-root",
                            label: "新建分类",
                            icon: PlusIcon,
                            disabled: !state.canCreate,
                            title: !state.canCreate
                                ? state.createBlockedReason
                                : undefined,
                            onClick: state.openCreateRoot,
                        },
                    ]}
                />
            </ListWorkspaceHeader>
            <div className="lg:hidden">
                <Button
                    id="category-mobile-nav-open"
                    type="button"
                    variant="outline"
                    className="w-full justify-between"
                    onClick={() => setNavigationOpen(true)}
                >
                    <span className="flex min-w-0 items-center gap-2">
                        <FolderTreeIcon className="size-4 shrink-0" />
                        <span className="truncate">
                            {state.selected?.name ?? "全部分类"}
                        </span>
                    </span>
                    <span className="shrink-0 text-xs text-muted-foreground">
                        选择分类{state.filterActive ? " · 已筛选" : ""}
                    </span>
                </Button>
            </div>
            <div className="grid min-h-[32rem] min-w-0 border-t border-border lg:min-h-0 lg:flex-1 lg:grid-cols-[288px_minmax(0,1fr)] lg:overflow-hidden">
                <aside
                    aria-label="分类导航"
                    className="hidden min-h-0 flex-col overflow-hidden border-r border-border pt-5 pr-5 lg:flex"
                >
                    {navigation(false)}
                </aside>
                <div
                    id="category-workspace-scroll"
                    ref={workspaceRef}
                    className="min-w-0 lg:min-h-0 lg:overflow-y-auto lg:overscroll-contain"
                    onScroll={(event) => {
                        try {
                            sessionStorage.setItem(
                                workspaceScrollKey,
                                String(event.currentTarget.scrollTop),
                            )
                        } catch {
                            /* 不影响浏览。 */
                        }
                    }}
                    aria-busy={pending || state.listQuery.isPlaceholderData}
                >
                    {pending ? (
                        <div
                            role="status"
                            className="p-8 text-sm text-muted-foreground"
                        >
                            正在加载分类资料…
                        </div>
                    ) : failed ? (
                        <BusinessFailureState
                            error={
                                state.fullQuery.error ?? state.listQuery.error
                            }
                            action={
                                <Button
                                    id="category-workspace-retry"
                                    onClick={() => {
                                        void state.fullQuery.refetch()
                                        void state.listQuery.refetch()
                                    }}
                                >
                                    重试
                                </Button>
                            }
                        />
                    ) : state.listQuery.isPlaceholderData ? (
                        <div
                            role="status"
                            className="p-8 text-sm text-muted-foreground"
                        >
                            正在查询分类…
                        </div>
                    ) : (
                        <CategoryTreeDetailPanel
                            selected={state.selected}
                            selectedId={state.selectedId}
                            selectedNode={state.selectedNode}
                            rows={state.rows}
                            nodes={nodes}
                            filterActive={state.filterActive}
                            matchedIds={state.matchedIds}
                            query={state.q.trim()}
                            returnTo={state.returnTo}
                            canCreate={state.canCreate}
                            createBlockedReason={state.createBlockedReason}
                            onSelect={select}
                            onClearFilters={state.clearFilters}
                            onOpenCreateRoot={state.openCreateRoot}
                            onOpenCreateChild={state.openCreateChild}
                            onReviseTarget={state.setReviseTarget}
                            onMoveTarget={state.setMoveTarget}
                            onDisableTarget={state.setDisableTarget}
                        />
                    )}
                </div>
            </div>
            <Sheet open={navigationOpen} onOpenChange={setNavigationOpen}>
                <SheetContent
                    closeButtonId="category-mobile-nav-close"
                    side="left"
                    className="w-[min(360px,100vw)]"
                >
                    <SheetHeader>
                        <SheetTitle>选择分类</SheetTitle>
                        <SheetDescription>
                            查找分类并查看资料或下级分类。
                        </SheetDescription>
                    </SheetHeader>
                    <div className="flex min-h-0 flex-1 flex-col px-7 pb-6">
                        {navigation(true)}
                    </div>
                </SheetContent>
            </Sheet>
            <CategoryCreateDialog
                key={`cat-create-${state.createParentId ?? "root"}-${state.createOpen}`}
                open={state.createOpen}
                onOpenChange={state.setCreateOpen}
                defaultParentId={state.createParentId}
                onCreated={state.onCreated}
            />
            <CategoryReviseDialog
                open={state.reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setReviseTarget(null)
                }}
                target={state.reviseTarget}
            />
            <CategoryReviseDialog
                mode="move"
                open={state.moveTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setMoveTarget(null)
                }}
                target={state.moveTarget}
            />
            <CategoryDisableDialog
                open={state.disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) state.setDisableTarget(null)
                }}
                target={state.disableTarget}
            />
        </PageScaffold>
    )
}
