"use client"

import * as React from "react"
import {
    ArrowLeft,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    FileCheck,
    Package,
    Search,
    X,
} from "lucide-react"

import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { LoadingButton } from "@/components/ui/loading-button"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { publicImageUrl } from "../api"
import { compareMoney, sumAmounts } from "../lib/money"
import type { PendingSelectionRequest } from "../lib/public-flow"
import { inferItemCategory } from "./public-selection-summary"
import type { PublicDisplayItemView, PublicPageView } from "../types"

export interface FloorSection {
    id: string
    name: string
    items: PublicDisplayItemView[]
}

interface SelectionReviewCenterProps {
    accessToken: string
    token: string
    page: PublicPageView
    locked: boolean
    submitting: boolean
    saving: boolean
    conflict: boolean
    dirty: boolean
    message: string
    request: PendingSelectionRequest | null
    reconciling: boolean
    onRecover: () => void
    onBack: () => void
    onSubmit: () => void
    onRemoveItem: (itemId: string) => void
}

/** 全屏沉浸式·选品方案核对与确认中心（针对海量/几百款商品设计，提供高管KPI看板、已选内实时搜索、品类折叠手风琴与明细微调）。 */
export const SelectionReviewCenter = ({
    accessToken,
    token,
    page,
    locked,
    submitting,
    saving,
    conflict,
    dirty,
    message,
    request,
    reconciling,
    onRecover,
    onBack,
    onSubmit,
    onRemoveItem,
}: SelectionReviewCenterProps) => {
    const [searchQuery, setSearchQuery] = React.useState("")
    const [activeCategory, setActiveCategory] = React.useState<string>("ALL")
    const [collapsed, setCollapsed] = React.useState<Record<string, boolean>>(
        {},
    )
    const [removingId, setRemovingId] = React.useState<string | null>(null)
    const [optimisticRemoved, setOptimisticRemoved] = React.useState<
        Set<string>
    >(() => new Set())

    // 当底层数据刷新时清空乐观移除暂存
    React.useEffect(() => {
        setOptimisticRemoved(new Set())
        setRemovingId(null)
    }, [page])

    const handleRemove = (itemId: string) => {
        if (locked || conflict) return
        setRemovingId(itemId)
        setOptimisticRemoved((prev) => new Set(prev).add(itemId))
        onRemoveItem(itemId)
    }

    // 按品类/档位归类已选商品并计算小计
    const groupedChoices = React.useMemo(() => {
        const map = new Map<
            string,
            {
                category: string
                items: Array<{
                    choice: (typeof page.choices)[number]
                    item: PublicDisplayItemView | undefined
                }>
                subtotal: string
            }
        >()

        for (const choice of page.choices) {
            if (optimisticRemoved.has(choice.item_id) && !saving) continue
            const item = page.items.find((i) => i.item_id === choice.item_id)
            const cat = item ? inferItemCategory(item) : "精选好物"
            const entry = map.get(cat) ?? {
                category: cat,
                items: [],
                subtotal: "0.00",
            }
            entry.items.push({ choice, item })
            if (choice.line_amount)
                entry.subtotal = sumAmounts([
                    entry.subtotal,
                    choice.line_amount,
                ])
            map.set(cat, entry)
        }

        return Array.from(map.values())
    }, [page.choices, page.items, optimisticRemoved, saving])

    // 计算总件数/份数
    const totalPieces = React.useMemo(() => {
        return page.choices.reduce((sum, c) => sum + (c.quantity ?? 1), 0)
    }, [page.choices])

    // 搜索与品类筛选过滤
    const filteredGroups = React.useMemo(() => {
        const q = searchQuery.trim().toLowerCase()
        return groupedChoices
            .filter((group) => {
                if (
                    activeCategory !== "ALL" &&
                    group.category !== activeCategory
                ) {
                    return false
                }
                return true
            })
            .map((group) => {
                if (!q) return group
                const matchedItems = group.items.filter(({ item }) => {
                    if (!item) return false
                    if (item.name.toLowerCase().includes(q)) return true
                    return item.specification.some(
                        (s) =>
                            s.name.toLowerCase().includes(q) ||
                            s.value.toLowerCase().includes(q),
                    )
                })
                return { ...group, items: matchedItems }
            })
            .filter((group) => group.items.length > 0)
    }, [groupedChoices, activeCategory, searchQuery])

    const toggleCollapse = (cat: string) => {
        setCollapsed((prev) => ({ ...prev, [cat]: !prev[cat] }))
    }

    const expandAll = () => setCollapsed({})
    const collapseAll = () => {
        const next: Record<string, boolean> = {}
        for (const g of groupedChoices) {
            next[g.category] = true
        }
        setCollapsed(next)
    }

    return (
        <div className="fixed inset-0 z-40 bg-overlay backdrop-blur-xs flex flex-col overflow-hidden animate-in fade-in duration-200">
            <div className="mx-auto w-full max-w-lg h-full flex flex-col bg-background shadow-2xl overflow-hidden relative">
                {/* 1. 顶部导航栏 */}
                <header className="shrink-0 bg-card border-b border-border/80 px-4 py-2.5 flex items-center justify-between z-10 shadow-2xs">
                    <button
                        id="sales-selection-public-review-back"
                        type="button"
                        onClick={onBack}
                        className="flex items-center gap-1 text-foreground hover:text-primary text-xs font-semibold py-1 px-2 rounded-lg hover:bg-muted transition-colors border-0 bg-transparent cursor-pointer"
                    >
                        <ArrowLeft className="h-4 w-4" />
                        <span>返回选品</span>
                    </button>
                    <div className="text-center">
                        <h1 className="text-xs sm:text-sm font-bold text-foreground">
                            方案核对与确认
                        </h1>
                        <p className="text-xs text-muted-foreground font-medium truncate max-w-[160px]">
                            {page.customer_name}
                        </p>
                    </div>
                    <Badge variant="info">核对中</Badge>
                </header>

                {/* 2. 中间可滚动核对区域 */}
                <div className="flex-1 overflow-y-auto p-3.5 space-y-3.5">
                    {(message || request) && (
                        <section
                            role="status"
                            className="space-y-2 rounded-xl border border-warning-border bg-warning-soft p-3 text-xs text-warning-soft-foreground"
                        >
                            <p>
                                {message ||
                                    "刚才的操作尚未完成确认，请点击下方按钮继续。"}
                            </p>
                            {request &&
                                ((!submitting && !saving) || reconciling) && (
                                    <LoadingButton
                                        id="sales-selection-public-reconcile"
                                        size="sm"
                                        loading={
                                            (submitting || saving) &&
                                            reconciling
                                        }
                                        onClick={onRecover}
                                    >
                                        继续确认
                                    </LoadingButton>
                                )}
                        </section>
                    )}
                    {/* 方案 KPI 看板 */}
                    <div className="rounded-2xl bg-card p-3.5 border border-border/80 shadow-2xs space-y-3">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                                <div className="flex h-7 w-7 items-center justify-center rounded-xl bg-info-soft text-info-soft-foreground">
                                    <CheckCircle2 className="h-4 w-4" />
                                </div>
                                <div>
                                    <h2 className="text-xs font-bold text-foreground">
                                        已选方案概览
                                    </h2>
                                    <p className="text-xs text-muted-foreground">
                                        涵盖 {groupedChoices.length} 个商品大类
                                    </p>
                                </div>
                            </div>
                            <span className="text-tiny font-semibold text-info-soft-foreground bg-info-soft px-2 py-0.5 rounded-full border border-info-border">
                                批量采购方案
                            </span>
                        </div>

                        <div className="grid grid-cols-3 gap-2 pt-2 border-t border-grid text-center">
                            <div className="rounded-xl bg-background/80 p-2 border border-border">
                                <p className="text-xs text-muted-foreground">
                                    已选款式
                                </p>
                                <p className="text-sm font-bold text-foreground mt-0.5">
                                    {page.choices.length}{" "}
                                    <span className="text-xs font-normal text-muted-foreground">
                                        款
                                    </span>
                                </p>
                            </div>
                            <div className="rounded-xl bg-background/80 p-2 border border-border">
                                <p className="text-xs text-muted-foreground">
                                    采购总件数
                                </p>
                                <p className="text-sm font-bold text-foreground mt-0.5">
                                    {totalPieces}{" "}
                                    <span className="text-xs font-normal text-muted-foreground">
                                        份
                                    </span>
                                </p>
                            </div>
                            <div className="rounded-xl bg-info-soft/60 p-2 border border-info-border">
                                <p className="text-xs text-primary font-medium">
                                    方案总金额
                                </p>
                                <p className="text-sm font-bold text-primary mt-0.5 truncate">
                                    {page.total_amount
                                        ? `¥ ${page.total_amount}`
                                        : "—"}
                                </p>
                            </div>
                        </div>
                    </div>

                    {/* 搜索与品类筛选工具栏 */}
                    <div className="space-y-2">
                        <div className="relative">
                            <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-muted-foreground" />
                            <Input
                                id="sales-selection-public-review-search"
                                type="text"
                                placeholder="在已选商品中检索名称或规格..."
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                                className="h-control-sm w-full rounded-full pl-8 pr-8"
                            />
                            {searchQuery && (
                                <button
                                    id="sales-selection-public-review-search-clear"
                                    type="button"
                                    onClick={() => setSearchQuery("")}
                                    className="absolute right-2.5 top-2.5 text-muted-foreground hover:text-muted-foreground border-0 bg-transparent cursor-pointer"
                                >
                                    <X className="h-3.5 w-3.5" />
                                </button>
                            )}
                        </div>

                        {/* 品类选择胶囊 + 全部展开/折叠 */}
                        <div className="flex items-center justify-between gap-2 overflow-x-auto no-scrollbar py-0.5">
                            <div className="flex items-center gap-1 shrink-0">
                                <button
                                    id="sales-selection-public-review-category-all"
                                    type="button"
                                    onClick={() => setActiveCategory("ALL")}
                                    className={cn(
                                        "rounded-full px-2.5 py-1 text-tiny font-semibold transition-colors border cursor-pointer",
                                        activeCategory === "ALL"
                                            ? "bg-primary text-primary-foreground border-primary shadow-2xs"
                                            : "bg-card text-muted-foreground hover:bg-muted border-border",
                                    )}
                                >
                                    全部 ({page.choices.length})
                                </button>
                                {groupedChoices.map((g) => (
                                    <button
                                        id={`sales-selection-public-review-category-${toAutomationIdSegment(g.category)}`}
                                        key={g.category}
                                        type="button"
                                        onClick={() =>
                                            setActiveCategory(g.category)
                                        }
                                        className={cn(
                                            "rounded-full px-2.5 py-1 text-tiny font-semibold transition-colors shrink-0 border cursor-pointer",
                                            activeCategory === g.category
                                                ? "bg-primary text-primary-foreground border-primary shadow-2xs"
                                                : "bg-card text-muted-foreground hover:bg-muted border-border",
                                        )}
                                    >
                                        {g.category} ({g.items.length})
                                    </button>
                                ))}
                            </div>

                            <div className="flex items-center gap-1 shrink-0 text-xs text-muted-foreground">
                                <button
                                    id="sales-selection-public-review-expand-all"
                                    type="button"
                                    onClick={expandAll}
                                    className="hover:text-primary px-1 border-0 bg-transparent cursor-pointer font-medium"
                                >
                                    全部展开
                                </button>
                                <span className="text-muted-foreground">|</span>
                                <button
                                    id="sales-selection-public-review-collapse-all"
                                    type="button"
                                    onClick={collapseAll}
                                    className="hover:text-primary px-1 border-0 bg-transparent cursor-pointer font-medium"
                                >
                                    全部收起
                                </button>
                            </div>
                        </div>
                    </div>

                    {/* 分类折叠卡片列表 */}
                    <div className="space-y-2.5">
                        {filteredGroups.map((group) => {
                            const isGroupCollapsed = !!collapsed[group.category]
                            const groupSubtotal = group.subtotal

                            return (
                                <div
                                    key={group.category}
                                    className="rounded-2xl bg-card border border-border/80 shadow-2xs overflow-hidden transition-all"
                                >
                                    {/* 分类栏标头（点击折叠/展开） */}
                                    <button
                                        id={`sales-selection-public-review-group-${toAutomationIdSegment(group.category)}`}
                                        type="button"
                                        onClick={() =>
                                            toggleCollapse(group.category)
                                        }
                                        className="w-full flex items-center justify-between px-3.5 py-2.5 bg-background/70 hover:bg-muted/70 transition-colors border-0 text-left cursor-pointer"
                                    >
                                        <div className="flex items-center gap-2">
                                            {isGroupCollapsed ? (
                                                <ChevronRight className="h-4 w-4 text-muted-foreground" />
                                            ) : (
                                                <ChevronDown className="h-4 w-4 text-muted-foreground" />
                                            )}
                                            <span className="text-xs font-bold text-foreground">
                                                {group.category}
                                            </span>
                                            <span className="rounded-full bg-muted/70 text-muted-foreground px-1.5 py-0.2 text-tiny font-bold">
                                                {group.items.length} 款
                                            </span>
                                        </div>
                                        {compareMoney(group.subtotal, "0") >
                                            0 && (
                                            <span className="text-xs font-semibold text-primary">
                                                小计 ¥ {groupSubtotal}
                                            </span>
                                        )}
                                    </button>

                                    {/* 分类内商品明细 */}
                                    {!isGroupCollapsed && (
                                        <div className="divide-y divide-grid px-3.5">
                                            {group.items.map(
                                                ({ choice, item }) => {
                                                    if (!item) return null
                                                    const img = publicImageUrl(
                                                        token,
                                                        item.cover_path,
                                                        accessToken,
                                                    )
                                                    return (
                                                        <div
                                                            key={choice.item_id}
                                                            className="py-2.5 flex items-start gap-2.5"
                                                        >
                                                            {/* 商品小缩略图 */}
                                                            <div className="h-12 w-12 rounded-lg bg-background shrink-0 overflow-hidden border border-grid">
                                                                {img ? (
                                                                    // eslint-disable-next-line @next/next/no-img-element
                                                                    <img
                                                                        src={
                                                                            img
                                                                        }
                                                                        alt=""
                                                                        className="h-full w-full object-cover"
                                                                        referrerPolicy="no-referrer"
                                                                    />
                                                                ) : (
                                                                    <div className="flex h-full w-full items-center justify-center text-muted-foreground">
                                                                        <Package className="h-5 w-5" />
                                                                    </div>
                                                                )}
                                                            </div>

                                                            {/* 名称与规格 */}
                                                            <div className="flex-1 min-w-0">
                                                                <p className="text-xs font-semibold text-foreground leading-snug">
                                                                    {item.name}
                                                                </p>
                                                                {item
                                                                    .specification
                                                                    .length >
                                                                    0 && (
                                                                    <div className="mt-0.5 flex flex-wrap gap-1">
                                                                        {item.specification.map(
                                                                            (
                                                                                s,
                                                                            ) => (
                                                                                <span
                                                                                    key={
                                                                                        s.name
                                                                                    }
                                                                                    className="rounded bg-muted px-1 py-0.2 text-xs text-muted-foreground"
                                                                                >
                                                                                    {
                                                                                        s.name
                                                                                    }
                                                                                    ：
                                                                                    {
                                                                                        s.value
                                                                                    }
                                                                                </span>
                                                                            ),
                                                                        )}
                                                                    </div>
                                                                )}
                                                                {item.members
                                                                    .length >
                                                                    0 && (
                                                                    <div className="mt-1 pl-1.5 border-l-2 border-border text-xs text-muted-foreground space-y-0.5">
                                                                        {item.members.map(
                                                                            (
                                                                                m,
                                                                                idx,
                                                                            ) => (
                                                                                <p
                                                                                    key={
                                                                                        idx
                                                                                    }
                                                                                >
                                                                                    {
                                                                                        m.name
                                                                                    }{" "}
                                                                                    ·{" "}
                                                                                    {m.specification
                                                                                        .map(
                                                                                            (
                                                                                                s,
                                                                                            ) =>
                                                                                                s.value,
                                                                                        )
                                                                                        .join(
                                                                                            "/",
                                                                                        )}{" "}
                                                                                    ·
                                                                                    1
                                                                                    {
                                                                                        m.unit
                                                                                    }
                                                                                </p>
                                                                            ),
                                                                        )}
                                                                    </div>
                                                                )}
                                                            </div>

                                                            {/* 数量、金额与快捷移除 */}
                                                            <div className="text-right shrink-0">
                                                                {choice.quantity !=
                                                                    null && (
                                                                    <span className="inline-block rounded bg-muted px-1.5 py-0.5 text-xs font-semibold text-foreground">
                                                                        ×{" "}
                                                                        {
                                                                            choice.quantity
                                                                        }{" "}
                                                                        份
                                                                    </span>
                                                                )}
                                                                {choice.line_amount !=
                                                                    null && (
                                                                    <p className="mt-0.5 text-xs font-bold text-primary">
                                                                        ¥{" "}
                                                                        {
                                                                            choice.line_amount
                                                                        }
                                                                    </p>
                                                                )}
                                                                <LoadingButton
                                                                    id={`sales-selection-public-review-remove-${toAutomationIdSegment(item.item_id)}`}
                                                                    type="button"
                                                                    variant="link"
                                                                    size="sm"
                                                                    loading={
                                                                        saving &&
                                                                        removingId ===
                                                                            item.item_id
                                                                    }
                                                                    disabled={
                                                                        locked ||
                                                                        conflict ||
                                                                        removingId ===
                                                                            item.item_id
                                                                    }
                                                                    onClick={() =>
                                                                        handleRemove(
                                                                            item.item_id,
                                                                        )
                                                                    }
                                                                    className="mt-1 h-auto text-xs text-muted-foreground hover:text-primary disabled:opacity-40 transition-colors border-0 bg-transparent p-0 cursor-pointer ml-auto"
                                                                >
                                                                    {removingId ===
                                                                    item.item_id
                                                                        ? "移除中…"
                                                                        : "移除"}
                                                                </LoadingButton>
                                                            </div>
                                                        </div>
                                                    )
                                                },
                                            )}
                                        </div>
                                    )}
                                </div>
                            )
                        })}

                        {filteredGroups.length === 0 && (
                            <div className="py-12 text-center text-xs text-muted-foreground bg-card rounded-2xl border border-grid">
                                未找到与 &quot;{searchQuery}&quot;
                                匹配的已选商品
                            </div>
                        )}
                    </div>

                    {/* 业务提醒与锁定说明 */}
                    <div className="rounded-2xl bg-warning-soft/70 p-3 border border-warning-border/60 text-warning-soft-foreground space-y-1">
                        <div className="flex items-center gap-1.5 text-xs font-bold">
                            <FileCheck className="h-4 w-4 text-warning-soft-foreground shrink-0" />
                            <span>确认提交须知</span>
                        </div>
                        <p className="text-tiny leading-relaxed text-warning-soft-foreground/90">
                            确认提交后将保存选品结果并生成销售方案。如需调整，请联系销售。
                        </p>
                        {page.notices.map((n) => (
                            <p
                                key={n}
                                className="text-xs text-warning-soft-foreground"
                            >
                                · {n}
                            </p>
                        ))}
                    </div>
                </div>

                {/* 3. 吸底结算栏 */}
                <aside
                    aria-label="核对并提交"
                    className="shrink-0 border-t border-border/80 bg-card/95 backdrop-blur-md px-4 py-3 shadow-footer z-30"
                >
                    <div className="flex items-center justify-between gap-3">
                        <div>
                            <p className="text-tiny text-muted-foreground">
                                方案总计金额（含税）
                            </p>
                            {page.total_amount != null ? (
                                <div className="flex items-baseline text-primary font-bold">
                                    <span className="text-xs mr-0.5 font-bold">
                                        ¥
                                    </span>
                                    <span className="text-xl font-bold tracking-tight">
                                        {page.total_amount}
                                    </span>
                                </div>
                            ) : (
                                <p className="text-base font-bold text-primary">
                                    已选 {page.choices.length} 款
                                </p>
                            )}
                        </div>

                        <div className="flex items-center gap-2">
                            <Button
                                id="sales-selection-public-review-modify"
                                variant="outline"
                                className="rounded-full"
                                onClick={onBack}
                            >
                                返回修改
                            </Button>
                            <LoadingButton
                                id="sales-selection-public-submit"
                                className="rounded-full"
                                disabled={locked || conflict || dirty}
                                loading={submitting}
                                onClick={() => onSubmit()}
                            >
                                确认并提交选品
                            </LoadingButton>
                        </div>
                    </div>
                </aside>
            </div>
        </div>
    )
}
