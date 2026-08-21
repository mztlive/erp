"use client"

/**
 * W14 商品分类 · 树形维护页
 * 路由：/master-data/categories
 * 布局：左侧树 + 右侧摘要；新建根/子分类、更新资料、停用。
 */

import * as React from "react"
import { DownloadIcon, PlusIcon } from "lucide-react"

import {
    BusinessFailureState,
    DataFreshness,
    PageActions,
    PageHeader,
    PageScaffold,
    surfacePanelClassName,
} from "@/components/business"
import { Button } from "@/components/ui/button"
import {
    CategoryCreateDialog,
    CategoryReviseDialog,
} from "@/features/master-data/components/category/category-form-dialogs"
import { CategoryTreeDetailPanel } from "@/features/master-data/components/category/category-tree-detail-panel"
import { CategoryTreeList } from "@/features/master-data/components/category/category-tree-list"
import { CategoryTreeToolbar } from "@/features/master-data/components/category/category-tree-toolbar"
import { CategoryDisableDialog } from "@/features/master-data/components/shared/disable-action-dialog"
import { useMasterDataCategoryTree } from "@/features/master-data/hooks/use-master-data-category-tree"
import { masterDataCopy } from "@/features/master-data/lib/copy"
import { cn } from "@/lib/utils"

export function CategoryTreePage() {
    const searchInputRef = React.useRef<HTMLInputElement | null>(null)
    const {
        search,
        setSearch,
        lifecycleStatus,
        setLifecycleStatus,
        selectedId,
        setSelectedId,
        expanded,
        createOpen,
        setCreateOpen,
        createParentId,
        reviseTarget,
        setReviseTarget,
        disableTarget,
        setDisableTarget,
        exportMeta,
        listQuery,
        rows,
        forest,
        selected,
        selectedPath,
        visibleCount,
        filterActive,
        toggle,
        expandAll,
        collapseAll,
        clearFilters,
        openCreateRoot,
        openCreateChild,
        onExport,
    } = useMasterDataCategoryTree(searchInputRef)

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
                                onClick: onExport,
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

            <CategoryTreeToolbar
                searchInputRef={searchInputRef}
                search={search}
                onSearchChange={setSearch}
                lifecycleStatus={lifecycleStatus}
                onLifecycleStatusChange={setLifecycleStatus}
                onExpandAll={expandAll}
                onCollapseAll={collapseAll}
            />

            <div className="grid min-h-[28rem] gap-3 lg:grid-cols-[minmax(0,1.2fr)_minmax(0,1fr)]">
                <section
                    className={cn(
                        surfacePanelClassName,
                        "flex min-h-0 flex-col",
                    )}
                    aria-label={masterDataCopy.categoryTreeTitle}
                >
                    <div className="flex items-center justify-between border-b border-grid px-3 py-2">
                        <h2 className="text-sm font-semibold">
                            {masterDataCopy.categoryTreeTitle}
                        </h2>
                        <span className="text-xs text-muted-foreground">
                            可见 {visibleCount} 项 · 共 {rows.length} 项
                        </span>
                    </div>
                    <CategoryTreeList
                        forest={forest}
                        expanded={expanded}
                        selectedId={selectedId}
                        onToggle={toggle}
                        onSelect={(item) => setSelectedId(item.stableId)}
                        filterActive={filterActive}
                        onClearFilters={clearFilters}
                        onOpenCreateRoot={openCreateRoot}
                    />
                </section>

                <CategoryTreeDetailPanel
                    selected={selected}
                    selectedId={selectedId}
                    selectedPath={selectedPath}
                    rows={rows}
                    onClearFilters={clearFilters}
                    onOpenCreateChild={openCreateChild}
                    onReviseTarget={setReviseTarget}
                    onDisableTarget={setDisableTarget}
                />
            </div>

            <CategoryCreateDialog
                key={`cat-create-${createParentId ?? "root"}-${createOpen}`}
                open={createOpen}
                onOpenChange={setCreateOpen}
                defaultParentId={createParentId}
            />
            <CategoryReviseDialog
                open={reviseTarget != null}
                onOpenChange={(open) => {
                    if (!open) setReviseTarget(null)
                }}
                target={reviseTarget}
            />
            <CategoryDisableDialog
                open={disableTarget != null}
                onOpenChange={(open) => {
                    if (!open) setDisableTarget(null)
                }}
                target={disableTarget}
            />
        </PageScaffold>
    )
}
