"use client"

import * as React from "react"
import { useStore } from "@tanstack/react-form"
import { useMutation, useQueryClient } from "@tanstack/react-query"
import { useAppForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { Badge } from "@/components/ui/badge"
import {
    Dialog,
    DialogContent,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import {
    AlertCircle,
    ArrowLeft,
    Check,
    CheckCircle2,
    ChevronDown,
    ChevronRight,
    Clock,
    FileCheck,
    Layers,
    Megaphone,
    Minus,
    Package,
    Plus,
    RefreshCw,
    Search,
    ShieldAlert,
    ShoppingBag,
    Store,
    X,
} from "lucide-react"
import { cn } from "@/lib/utils"
import { publicImageUrl, savePublicSession, submitPublicSession } from "../api"
import { usePublicSelectionQuery, salesSelectionKeys } from "../queries"
import {
    picksFromChoices,
    readPendingRequest,
    requestFailure,
    type PendingSelectionRequest,
    type Pick,
} from "../lib/public-flow"
import { quantityStringSchema } from "../lib/validation"
import type { PublicDisplayItemView, PublicPageView } from "../types"

/** 读取公开选品事实，明确区分加载、结束、回执和可编辑状态。 */
export const PublicSelectionPage = ({ token }: { token: string }) => {
    const query = usePublicSelectionQuery(token)
    const page = query.data

    if (query.isError && (!page || requestFailure(query.error) === "ended"))
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center">
                <div className="mb-4 flex h-14 w-14 items-center justify-center rounded-2xl bg-slate-100 text-slate-400">
                    <Clock className="h-7 w-7" />
                </div>
                <h1 className="text-xl font-bold text-slate-900">
                    {requestFailure(query.error) === "ended"
                        ? "选品链接已失效"
                        : "暂时无法打开选品页"}
                </h1>
                <p className="my-3 text-sm leading-relaxed text-slate-500">
                    {requestFailure(query.error) === "ended"
                        ? "请联系销售核对当前链接。"
                        : "请检查网络连接后重试。"}
                </p>
                {requestFailure(query.error) !== "ended" && (
                    <Button
                        id="sales-selection-public-retry"
                        onClick={() => void query.refetch()}
                        className="mt-2"
                    >
                        重新打开
                    </Button>
                )}
            </main>
        )

    if (!page)
        return (
            <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center text-sm text-slate-500">
                <RefreshCw className="mb-3 h-6 w-6 animate-spin text-blue-600" />
                正在打开选品页…
            </main>
        )

    if (page.kind === "ENDED") return <Ended />
    if (page.kind === "RECEIPT" && page.receipt) return <Receipt page={page} />

    return (
        <SelectionForm
            key={token}
            token={token}
            page={page}
            refresh={async () => (await query.refetch()).data}
        />
    )
}

/** 结束后移除所有可写表单。 */
const Ended = () => {
    return (
        <main className="mx-auto flex min-h-screen max-w-md flex-col items-center justify-center p-6 text-center bg-slate-50">
            <div className="mb-4 flex h-16 w-16 items-center justify-center rounded-3xl bg-slate-200/70 text-slate-400">
                <Clock className="h-8 w-8" />
            </div>
            <h1 className="text-xl font-bold text-slate-900">选品已结束</h1>
            <p className="mt-2 text-sm text-slate-500">
                链接已失效，请联系销售核对。
            </p>
            <p className="mt-4 text-xs text-slate-400">
                如需重新选品或确认报价，请联系您的专属销售经理获取新链接。
            </p>
        </main>
    )
}

/** 显示服务端已提交的客户明细和金额。 */
const Receipt = ({ page }: { page: PublicPageView }) => {
    const receipt = page.receipt!
    return (
        <main className="mx-auto min-h-screen max-w-lg bg-slate-100 p-4 pb-12">
            <div className="mb-4 rounded-3xl bg-white p-6 text-center shadow-xs border border-slate-200/80">
                <div className="mx-auto mb-3 flex h-14 w-14 items-center justify-center rounded-full bg-emerald-50 text-emerald-600">
                    <CheckCircle2 className="h-8 w-8" />
                </div>
                <h1 className="text-xl font-bold text-slate-900">已提交选品</h1>
                <p className="mt-1 text-sm font-semibold text-slate-800">
                    {receipt.customer_name}
                </p>
                <div className="mt-3 inline-flex flex-col items-center gap-1 rounded-xl bg-slate-50 px-4 py-2 text-xs text-slate-600 border border-slate-200">
                    <span className="font-mono font-medium">
                        方案编号 {receipt.proposal_no}
                    </span>
                    <span className="text-slate-400">
                        提交时间：{formatTime(receipt.submitted_at)}
                    </span>
                </div>
            </div>

            <div className="space-y-3">
                <div className="flex items-center gap-2 px-1">
                    <FileCheck className="h-4 w-4 text-blue-600" />
                    <h2 className="text-sm font-semibold text-slate-900">
                        确认选品清单
                    </h2>
                </div>
                <ChoiceSummary page={page} receipt />
                {page.notices.map((notice) => (
                    <div
                        key={notice}
                        className="rounded-xl bg-white p-3 text-xs text-slate-500 border border-slate-200/60 leading-relaxed"
                    >
                        {notice}
                    </div>
                ))}
            </div>
        </main>
    )
}

/** 使用快照名称和后端金额核对所选内容。 */
const ChoiceSummary = ({
    page,
    receipt = false,
}: {
    page: PublicPageView
    receipt?: boolean
}) => {
    const choices = receipt ? page.receipt!.items : page.choices
    const total = receipt ? page.receipt!.total_amount : page.total_amount
    return (
        <div className="space-y-3 rounded-2xl border border-slate-200/80 bg-white p-4 shadow-xs">
            {choices.map((choice) => {
                const item = page.items.find(
                    (item) => item.item_id === choice.item_id,
                )
                return (
                    <div
                        key={choice.item_id}
                        className="border-b border-slate-100 pb-3 last:border-b-0 last:pb-0"
                    >
                        <div className="flex items-baseline justify-between gap-2">
                            <p className="font-medium text-slate-900 text-sm leading-snug">
                                {item?.name ??
                                    "商品资料暂不可用，请联系销售核对"}
                                {choice.quantity != null
                                    ? ` × ${choice.quantity} 份`
                                    : ""}
                            </p>
                            {choice.line_amount != null && (
                                <p className="font-semibold text-blue-600 text-sm shrink-0">
                                    ¥ {choice.line_amount}
                                </p>
                            )}
                        </div>
                        {item?.specification &&
                            item.specification.length > 0 && (
                                <div className="mt-1 flex flex-wrap gap-1">
                                    {item.specification.map((spec) => (
                                        <span
                                            key={spec.name}
                                            className="rounded-md bg-slate-100 px-1.5 py-0.5 text-[11px] text-slate-500"
                                        >
                                            {spec.name}：{spec.value}
                                        </span>
                                    ))}
                                </div>
                            )}
                        {item?.members && item.members.length > 0 && (
                            <div className="mt-1.5 space-y-0.5 pl-2 border-l-2 border-slate-200 text-xs text-slate-500">
                                {item.members.map((member, index) => (
                                    <p
                                        key={`${member.name}-${index}`}
                                        className="text-[11px]"
                                    >
                                        {member.name} ·{" "}
                                        {member.specification
                                            .map((s) => `${s.name}：${s.value}`)
                                            .join(" / ")}{" "}
                                        · 1 {member.unit}
                                    </p>
                                ))}
                            </div>
                        )}
                    </div>
                )
            })}
            {!choices.length && (
                <p className="py-2 text-center text-xs text-slate-400">
                    尚未选择商品
                </p>
            )}
            {page.submit_mode === "BY_QUANTITY" && total != null && (
                <div className="flex items-center justify-between pt-2 border-t border-slate-100">
                    <span className="text-xs font-medium text-slate-500">
                        方案合计金额（含税）
                    </span>
                    <p className="font-bold text-base text-blue-600">
                        合计 ¥ {total}
                    </p>
                </div>
            )}
        </div>
    )
}

/** 分类推断辅助函数（在单品形态无档位时，根据名称与规格智能分类）。 */
const inferItemCategory = (item: PublicDisplayItemView): string => {
    if (item.tier_name) return item.tier_name
    for (const spec of item.specification) {
        if (
            (spec.name === "分类" || spec.name === "品类") &&
            spec.value.trim()
        ) {
            return spec.value.trim()
        }
    }
    const name = item.name.toLowerCase()
    if (
        name.includes("茶") ||
        name.includes("普洱") ||
        name.includes("龙井") ||
        name.includes("滋补")
    )
        return "茗茶冲饮"
    if (
        name.includes("卡") ||
        name.includes("券") ||
        name.includes("密") ||
        name.includes("通")
    )
        return "礼品卡券"
    if (
        name.includes("安装") ||
        name.includes("派送") ||
        name.includes("服务") ||
        name.includes("保洁")
    )
        return "生活服务"
    if (name.includes("数码") || name.includes("家电") || name.includes("电"))
        return "数码家电"
    if (
        name.includes("生鲜") ||
        name.includes("礼包") ||
        name.includes("果") ||
        name.includes("食品") ||
        name.includes("粮油")
    )
        return "节庆生鲜"
    return "精选好物"
}

interface FloorSection {
    id: string
    name: string
    items: PublicDisplayItemView[]
}

/** 安全地格式化分为元字符串，不调用浮点转换函数。 */
const formatCents = (cents: number): string => {
    const whole = Math.floor(cents / 100)
    const frac = cents % 100
    return `${whole}.${frac < 10 ? "0" : ""}${frac}`
}

interface SelectionReviewCenterProps {
    token: string
    page: PublicPageView
    locked: boolean
    conflict: boolean
    dirty: boolean
    onBack: () => void
    onSubmit: () => void
    onRemoveItem: (itemId: string) => void
}

/** 全屏沉浸式·选品方案核对与确认中心（针对海量/几百款商品设计，提供高管KPI看板、已选内实时搜索、品类折叠手风琴与明细微调）。 */
const SelectionReviewCenter = ({
    token,
    page,
    locked,
    conflict,
    dirty,
    onBack,
    onSubmit,
    onRemoveItem,
}: SelectionReviewCenterProps) => {
    const mall = page.submit_mode === "MALL_REDEEM"
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
                subtotalCents: number
            }
        >()

        for (const choice of page.choices) {
            if (optimisticRemoved.has(choice.item_id)) continue
            const item = page.items.find((i) => i.item_id === choice.item_id)
            const cat = item ? inferItemCategory(item) : "精选好物"
            const entry = map.get(cat) ?? {
                category: cat,
                items: [],
                subtotalCents: 0,
            }
            entry.items.push({ choice, item })
            if (choice.line_amount) {
                const [intStr, decStr = ""] = choice.line_amount.split(".")
                const intVal = Number.parseInt(intStr, 10) || 0
                const decVal =
                    Number.parseInt((decStr + "00").slice(0, 2), 10) || 0
                entry.subtotalCents += intVal * 100 + decVal
            }
            map.set(cat, entry)
        }

        return Array.from(map.values())
    }, [page.choices, page.items, optimisticRemoved])

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
        <div className="fixed inset-0 z-40 bg-slate-900/40 backdrop-blur-xs flex flex-col overflow-hidden animate-in fade-in duration-200">
            <div className="mx-auto w-full max-w-lg h-full flex flex-col bg-slate-50 shadow-2xl overflow-hidden relative">
                {/* 1. 顶部导航栏 */}
                <header className="shrink-0 bg-white border-b border-slate-200/80 px-4 py-2.5 flex items-center justify-between z-10 shadow-2xs">
                    <button
                        type="button"
                        onClick={onBack}
                        className="flex items-center gap-1 text-slate-700 hover:text-blue-600 text-xs font-semibold py-1 px-2 rounded-lg hover:bg-slate-100 transition-colors border-0 bg-transparent cursor-pointer"
                    >
                        <ArrowLeft className="h-4 w-4" />
                        <span>返回选品</span>
                    </button>
                    <div className="text-center">
                        <h1 className="text-xs sm:text-sm font-bold text-slate-900">
                            方案核对与确认
                        </h1>
                        <p className="text-[10px] text-slate-400 font-medium truncate max-w-[160px]">
                            {page.customer_name}
                        </p>
                    </div>
                    <Badge
                        variant="outline"
                        className="text-[10px] border-blue-200 bg-blue-50 text-blue-700 font-medium px-2 py-0.5"
                    >
                        核对中
                    </Badge>
                </header>

                {/* 2. 中间可滚动核对区域 */}
                <div className="flex-1 overflow-y-auto p-3.5 space-y-3.5">
                    {/* 方案 KPI 看板 */}
                    <div className="rounded-2xl bg-white p-3.5 border border-slate-200/80 shadow-2xs space-y-3">
                        <div className="flex items-center justify-between">
                            <div className="flex items-center gap-2">
                                <div className="flex h-7 w-7 items-center justify-center rounded-xl bg-blue-50 text-blue-600">
                                    <CheckCircle2 className="h-4 w-4" />
                                </div>
                                <div>
                                    <h2 className="text-xs font-bold text-slate-900">
                                        已选方案概览
                                    </h2>
                                    <p className="text-[10px] text-slate-400">
                                        涵盖 {groupedChoices.length} 个商品大类
                                    </p>
                                </div>
                            </div>
                            <span className="text-[11px] font-semibold text-blue-600 bg-blue-50 px-2 py-0.5 rounded-full border border-blue-100">
                                {mall ? "商城意向可选" : "批量采购方案"}
                            </span>
                        </div>

                        <div className="grid grid-cols-3 gap-2 pt-2 border-t border-slate-100 text-center">
                            <div className="rounded-xl bg-slate-50/80 p-2 border border-slate-150">
                                <p className="text-[10px] text-slate-400">
                                    已选款式
                                </p>
                                <p className="text-sm font-bold text-slate-800 mt-0.5">
                                    {page.choices.length}{" "}
                                    <span className="text-[10px] font-normal text-slate-500">
                                        款
                                    </span>
                                </p>
                            </div>
                            <div className="rounded-xl bg-slate-50/80 p-2 border border-slate-150">
                                <p className="text-[10px] text-slate-400">
                                    采购总件数
                                </p>
                                <p className="text-sm font-bold text-slate-800 mt-0.5">
                                    {totalPieces}{" "}
                                    <span className="text-[10px] font-normal text-slate-500">
                                        份
                                    </span>
                                </p>
                            </div>
                            <div className="rounded-xl bg-blue-50/60 p-2 border border-blue-100">
                                <p className="text-[10px] text-blue-600 font-medium">
                                    {mall ? "模式" : "方案总金额"}
                                </p>
                                <p className="text-sm font-bold text-blue-600 mt-0.5 truncate">
                                    {page.total_amount
                                        ? `¥ ${page.total_amount}`
                                        : "意向库"}
                                </p>
                            </div>
                        </div>
                    </div>

                    {/* 搜索与品类筛选工具栏 */}
                    <div className="space-y-2">
                        <div className="relative">
                            <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-slate-400" />
                            <Input
                                type="text"
                                placeholder="在已选商品中检索名称或规格..."
                                value={searchQuery}
                                onChange={(e) => setSearchQuery(e.target.value)}
                                className="h-8.5 w-full rounded-full bg-white pl-8.5 pr-8 text-xs placeholder:text-slate-400 border border-slate-200 shadow-2xs focus-visible:ring-1 focus-visible:ring-blue-500"
                            />
                            {searchQuery && (
                                <button
                                    type="button"
                                    onClick={() => setSearchQuery("")}
                                    className="absolute right-2.5 top-2.5 text-slate-400 hover:text-slate-600 border-0 bg-transparent cursor-pointer"
                                >
                                    <X className="h-3.5 w-3.5" />
                                </button>
                            )}
                        </div>

                        {/* 品类选择胶囊 + 全部展开/折叠 */}
                        <div className="flex items-center justify-between gap-2 overflow-x-auto no-scrollbar py-0.5">
                            <div className="flex items-center gap-1 shrink-0">
                                <button
                                    type="button"
                                    onClick={() => setActiveCategory("ALL")}
                                    className={cn(
                                        "rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors border cursor-pointer",
                                        activeCategory === "ALL"
                                            ? "bg-slate-900 text-white border-slate-900 shadow-2xs"
                                            : "bg-white text-slate-600 hover:bg-slate-100 border-slate-200",
                                    )}
                                >
                                    全部 ({page.choices.length})
                                </button>
                                {groupedChoices.map((g) => (
                                    <button
                                        key={g.category}
                                        type="button"
                                        onClick={() =>
                                            setActiveCategory(g.category)
                                        }
                                        className={cn(
                                            "rounded-full px-2.5 py-1 text-[11px] font-semibold transition-colors shrink-0 border cursor-pointer",
                                            activeCategory === g.category
                                                ? "bg-slate-900 text-white border-slate-900 shadow-2xs"
                                                : "bg-white text-slate-600 hover:bg-slate-100 border-slate-200",
                                        )}
                                    >
                                        {g.category} ({g.items.length})
                                    </button>
                                ))}
                            </div>

                            <div className="flex items-center gap-1 shrink-0 text-[10px] text-slate-500">
                                <button
                                    type="button"
                                    onClick={expandAll}
                                    className="hover:text-blue-600 px-1 border-0 bg-transparent cursor-pointer font-medium"
                                >
                                    全部展开
                                </button>
                                <span className="text-slate-300">|</span>
                                <button
                                    type="button"
                                    onClick={collapseAll}
                                    className="hover:text-blue-600 px-1 border-0 bg-transparent cursor-pointer font-medium"
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
                            const groupSubtotal = formatCents(
                                group.subtotalCents,
                            )

                            return (
                                <div
                                    key={group.category}
                                    className="rounded-2xl bg-white border border-slate-200/80 shadow-2xs overflow-hidden transition-all"
                                >
                                    {/* 分类栏标头（点击折叠/展开） */}
                                    <button
                                        type="button"
                                        onClick={() =>
                                            toggleCollapse(group.category)
                                        }
                                        className="w-full flex items-center justify-between px-3.5 py-2.5 bg-slate-50/70 hover:bg-slate-100/70 transition-colors border-0 text-left cursor-pointer"
                                    >
                                        <div className="flex items-center gap-2">
                                            {isGroupCollapsed ? (
                                                <ChevronRight className="h-4 w-4 text-slate-400" />
                                            ) : (
                                                <ChevronDown className="h-4 w-4 text-slate-400" />
                                            )}
                                            <span className="text-xs font-bold text-slate-900">
                                                {group.category}
                                            </span>
                                            <span className="rounded-full bg-slate-200/70 text-slate-600 px-1.5 py-0.2 text-[10px] font-bold">
                                                {group.items.length} 款
                                            </span>
                                        </div>
                                        {!mall && group.subtotalCents > 0 && (
                                            <span className="text-xs font-semibold text-blue-600">
                                                小计 ¥ {groupSubtotal}
                                            </span>
                                        )}
                                    </button>

                                    {/* 分类内商品明细 */}
                                    {!isGroupCollapsed && (
                                        <div className="divide-y divide-slate-100 px-3.5">
                                            {group.items.map(
                                                ({ choice, item }) => {
                                                    if (!item) return null
                                                    const img = publicImageUrl(
                                                        token,
                                                        item.cover_path,
                                                    )
                                                    return (
                                                        <div
                                                            key={choice.item_id}
                                                            className="py-2.5 flex items-start gap-2.5"
                                                        >
                                                            {/* 商品小缩略图 */}
                                                            <div className="h-12 w-12 rounded-lg bg-slate-50 shrink-0 overflow-hidden border border-slate-100">
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
                                                                    <div className="flex h-full w-full items-center justify-center text-slate-400">
                                                                        <Package className="h-5 w-5" />
                                                                    </div>
                                                                )}
                                                            </div>

                                                            {/* 名称与规格 */}
                                                            <div className="flex-1 min-w-0">
                                                                <p className="text-xs font-semibold text-slate-900 leading-snug">
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
                                                                                    className="rounded bg-slate-100 px-1 py-0.2 text-[9px] text-slate-500"
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
                                                                    <div className="mt-1 pl-1.5 border-l-2 border-slate-200 text-[10px] text-slate-400 space-y-0.5">
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
                                                                    <span className="inline-block rounded bg-slate-100 px-1.5 py-0.5 text-[10px] font-semibold text-slate-700">
                                                                        ×{" "}
                                                                        {
                                                                            choice.quantity
                                                                        }{" "}
                                                                        份
                                                                    </span>
                                                                )}
                                                                {choice.line_amount !=
                                                                    null && (
                                                                    <p className="mt-0.5 text-xs font-bold text-blue-600">
                                                                        ¥{" "}
                                                                        {
                                                                            choice.line_amount
                                                                        }
                                                                    </p>
                                                                )}
                                                                <button
                                                                    type="button"
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
                                                                    className="mt-1 text-[10px] text-slate-400 hover:text-blue-600 disabled:opacity-40 transition-colors border-0 bg-transparent p-0 cursor-pointer block ml-auto"
                                                                >
                                                                    {removingId ===
                                                                    item.item_id
                                                                        ? "移除中…"
                                                                        : "移除"}
                                                                </button>
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
                            <div className="py-12 text-center text-xs text-slate-400 bg-white rounded-2xl border border-slate-100">
                                未找到与 &quot;{searchQuery}&quot;
                                匹配的已选商品
                            </div>
                        )}
                    </div>

                    {/* 业务提醒与锁定说明 */}
                    <div className="rounded-2xl bg-amber-50/70 p-3 border border-amber-200/60 text-amber-900 space-y-1">
                        <div className="flex items-center gap-1.5 text-xs font-bold">
                            <FileCheck className="h-4 w-4 text-amber-600 shrink-0" />
                            <span>确认提交须知</span>
                        </div>
                        <p className="text-[11px] leading-relaxed text-amber-800/90">
                            确认提交后系统将锁定会话并生成唯一的正式销售方案编号，专属销售团队将按此方案推进合同签署、配货与开票。
                        </p>
                        {page.notices.map((n) => (
                            <p key={n} className="text-[10px] text-amber-700">
                                · {n}
                            </p>
                        ))}
                    </div>
                </div>

                {/* 3. 吸底结算栏 */}
                <aside
                    aria-label="核对并提交"
                    className="shrink-0 border-t border-slate-200/80 bg-white/95 backdrop-blur-md px-4 py-3 shadow-[0_-8px_20px_rgba(0,0,0,0.08)] z-30"
                >
                    <div className="flex items-center justify-between gap-3">
                        <div>
                            <p className="text-[11px] text-slate-500">
                                {mall ? "已选款式总计" : "方案总计金额（含税）"}
                            </p>
                            {page.total_amount != null && !mall ? (
                                <div className="flex items-baseline text-blue-600 font-bold">
                                    <span className="text-xs mr-0.5 font-bold">
                                        ¥
                                    </span>
                                    <span className="text-xl font-bold tracking-tight">
                                        {page.total_amount}
                                    </span>
                                </div>
                            ) : (
                                <p className="text-base font-bold text-blue-600">
                                    已选 {page.choices.length} 款
                                </p>
                            )}
                        </div>

                        <div className="flex items-center gap-2">
                            <Button
                                variant="outline"
                                size="sm"
                                className="rounded-full border-slate-300 text-xs text-slate-700 px-4 h-9"
                                onClick={onBack}
                            >
                                返回修改
                            </Button>
                            <Button
                                id="sales-selection-public-submit"
                                size="sm"
                                className="rounded-full bg-gradient-to-r from-emerald-500 to-emerald-600 text-xs font-bold text-white shadow-md hover:opacity-95 px-6 h-9 active:scale-95 transition-all"
                                disabled={locked || conflict || dirty}
                                onClick={onSubmit}
                            >
                                确认并提交选品
                            </Button>
                        </div>
                    </div>
                </aside>
            </div>
        </div>
    )
}

/** 方案 2：经典双栏联动楼层系统（固定视口高，右侧瀑布流滚动实时带动左侧菜单更新位置）。 */
const SelectionForm = ({
    token,
    page,
    refresh,
}: {
    token: string
    page: PublicPageView
    refresh: () => Promise<PublicPageView | undefined>
}) => {
    const client = useQueryClient()
    const recoveryKey = `sales-selection-pending:${token}`
    const [request, setRequest] =
        React.useState<PendingSelectionRequest | null>(() =>
            readPendingRequest(recoveryKey),
        )
    const [defaults] = React.useState(() => ({
        picks: picksFromChoices(
            request?.kind === "save" ? request.input.choices : page.choices,
        ),
    }))
    const form = useAppForm({ defaultValues: defaults })
    const picks = useStore(form.store, (state) => state.values.picks)
    const [version, setVersion] = React.useState(page.session_version ?? 1)
    const [confirmed, setConfirmed] = React.useState<PublicPageView | null>(
        null,
    )
    const [latest, setLatest] = React.useState<PublicPageView | null>(null)
    const [conflict, setConflict] = React.useState(false)
    const [ended, setEnded] = React.useState(false)
    const [dirty, setDirty] = React.useState(false)
    const [message, setMessage] = React.useState("")
    const mall = page.submit_mode === "MALL_REDEEM"

    // 交互状态
    const [searchQuery, setSearchQuery] = React.useState("")
    const [activeTabId, setActiveTabId] = React.useState<string>("")
    const [detailItem, setDetailItem] =
        React.useState<PublicDisplayItemView | null>(null)
    const [cartDrawerOpen, setCartDrawerOpen] = React.useState(false)

    // DOM 引用用于双向滚动同步
    const rightContainerRef = React.useRef<HTMLDivElement>(null)
    const leftAsideRef = React.useRef<HTMLElement>(null)
    const sectionRefs = React.useRef<Record<string, HTMLDivElement | null>>({})
    const leftTabRefs = React.useRef<Record<string, HTMLButtonElement | null>>(
        {},
    )
    const isManualScrolling = React.useRef(false)
    const manualScrollTimer = React.useRef<ReturnType<
        typeof setTimeout
    > | null>(null)

    const selectedCount = Object.values(picks).filter(
        (pick) => pick.selected,
    ).length

    // 构建楼层数据（全部商品按分类连续分楼层展示）
    const sections: FloorSection[] = React.useMemo(() => {
        const tierNames = Array.from(
            new Set(page.items.map((i) => i.tier_name).filter(Boolean)),
        ) as string[]

        if (tierNames.length > 0) {
            return tierNames.map((tier) => ({
                id: `tier:${tier}`,
                name: tier,
                items: page.items.filter((item) => item.tier_name === tier),
            }))
        }

        // 单品形态按分类构建楼层
        const categoryMap = new Map<string, PublicDisplayItemView[]>()
        for (const item of page.items) {
            const cat = inferItemCategory(item)
            const list = categoryMap.get(cat) ?? []
            list.push(item)
            categoryMap.set(cat, list)
        }

        return Array.from(categoryMap.entries()).map(([catName, items]) => ({
            id: `cat:${catName}`,
            name: catName,
            items,
        }))
    }, [page.items])

    const activeTabIdRef = React.useRef(activeTabId)
    React.useEffect(() => {
        activeTabIdRef.current = activeTabId
    }, [activeTabId])

    // 初始化默认选中第一个楼层
    React.useEffect(() => {
        if (!activeTabId && sections.length > 0) {
            setActiveTabId(sections[0].id)
        }
    }, [sections, activeTabId])

    // 联动核心 1：右侧滚动时，检测当前视口顶部楼层，带动左侧菜单更新高亮并居中
    const handleRightScroll = React.useCallback(() => {
        if (isManualScrolling.current) return
        if (searchQuery.trim()) return
        const container = rightContainerRef.current
        if (!container || sections.length === 0) return

        const containerTop = container.getBoundingClientRect().top
        let currentId = sections[0].id

        // 检测是否已滚动到底部，若到底部则直接高亮最后一个分类
        const isAtBottom =
            container.scrollTop + container.clientHeight >=
            container.scrollHeight - 25
        if (isAtBottom) {
            currentId = sections[sections.length - 1].id
        } else {
            for (const section of sections) {
                const el = sectionRefs.current[section.id]
                if (!el) continue
                const elTop = el.getBoundingClientRect().top - containerTop
                if (elTop <= 50) {
                    currentId = section.id
                } else {
                    break
                }
            }
        }

        if (currentId && currentId !== activeTabIdRef.current) {
            setActiveTabId(currentId)
            const tabBtn = leftTabRefs.current[currentId]
            if (tabBtn) {
                tabBtn.scrollIntoView({ block: "nearest", behavior: "smooth" })
            }
        }
    }, [sections, searchQuery])

    // 联动核心 2：点击左侧菜单，右侧平滑滚动定位到指定楼层
    const scrollToSection = React.useCallback((sectionId: string) => {
        setActiveTabId(sectionId)
        isManualScrolling.current = true
        if (manualScrollTimer.current) clearTimeout(manualScrollTimer.current)

        const targetEl = sectionRefs.current[sectionId]
        const container = rightContainerRef.current
        if (targetEl && container) {
            const currentScroll = container.scrollTop
            const targetRelativeTop =
                targetEl.getBoundingClientRect().top -
                container.getBoundingClientRect().top
            const targetScrollTop = currentScroll + targetRelativeTop
            container.scrollTo({ top: targetScrollTop, behavior: "smooth" })
        }

        manualScrollTimer.current = setTimeout(() => {
            isManualScrolling.current = false
        }, 500)
    }, [])

    // 搜索过滤视图
    const searchResults = React.useMemo(() => {
        if (!searchQuery.trim()) return null
        const q = searchQuery.trim().toLowerCase()
        return page.items.filter((item) => {
            if (item.name.toLowerCase().includes(q)) return true
            return item.specification.some(
                (s) =>
                    s.name.toLowerCase().includes(q) ||
                    s.value.toLowerCase().includes(q),
            )
        })
    }, [page.items, searchQuery])

    const keepRequest = (next: PendingSelectionRequest | null) => {
        setRequest(next)
        try {
            if (next) sessionStorage.setItem(recoveryKey, JSON.stringify(next))
            else sessionStorage.removeItem(recoveryKey)
        } catch {
            /* 当前页面仍保留请求 */
        }
    }

    const mutation = useMutation({
        meta: { suppressErrorToast: true },
        mutationFn: (operation: PendingSelectionRequest) =>
            operation.kind === "save"
                ? savePublicSession(token, operation.input)
                : submitPublicSession(token, operation.input),
        onSuccess: (saved, operation) => {
            client.setQueryData(salesSelectionKeys.public(token), saved)
            keepRequest(null)
            setMessage(operation.kind === "save" ? "选择已保存" : "选品已提交")
            setVersion(saved.session_version ?? version)
            setDirty(false)
            form.setFieldValue("picks", picksFromChoices(saved.choices))
            setConfirmed(
                operation.kind === "save" &&
                    operation.confirm &&
                    saved.kind === "SELECTING"
                    ? saved
                    : null,
            )
        },
        onError: async (error) => {
            setConfirmed(null)
            const kind = requestFailure(error)
            if (kind === "ended") {
                keepRequest(null)
                setEnded(true)
                return
            }
            if (kind === "unknown") {
                setMessage(
                    "操作结果尚未确认。已保留本次请求，请核对结果后继续。",
                )
                return
            }
            keepRequest(null)
            if (kind === "conflict") {
                setConflict(true)
                setMessage(
                    "其他页面已修改选择。你的本地选择已保留，请核对最新清单后继续。",
                )
                setLatest((await refresh()) ?? null)
            } else
                setMessage(
                    error instanceof Error
                        ? error.message
                        : "保存未成功，请检查份数后重试。",
                )
        },
    })

    const locked = mutation.isPending || request !== null

    const change = (id: string, patch: Partial<Pick>) => {
        if (locked || conflict) return
        form.setFieldValue("picks", (current) => ({
            ...current,
            [id]: {
                ...(current[id] ?? { selected: false, quantity: "1" }),
                ...patch,
            },
        }))
        setConfirmed(null)
        setDirty(true)
        setMessage("")
    }

    const persist = (confirm: boolean) => {
        if (locked || conflict) return
        const choices: { item_id: string; quantity?: number }[] = []
        for (const item of page.items) {
            const pick = picks[item.item_id]
            if (!pick?.selected) continue
            if (
                !mall &&
                !quantityStringSchema.safeParse(pick.quantity).success
            ) {
                setMessage("每项份数必须为 1 到 100000 的整数。")
                return
            }
            choices.push({
                item_id: item.item_id,
                ...(mall
                    ? {}
                    : { quantity: Number.parseInt(pick.quantity, 10) }),
            })
        }
        if (confirm && !choices.length) {
            setMessage("请至少选择一项商品。")
            return
        }
        const next: PendingSelectionRequest = {
            kind: "save",
            confirm,
            input: {
                idempotencyKey: crypto.randomUUID(),
                expectedSessionVersion: version,
                choices,
            },
        }
        keepRequest(next)
        mutation.mutate(next)
    }

    const removeItemInReview = (itemId: string) => {
        if (locked || conflict) return
        const updatedPicks = {
            ...picks,
            [itemId]: {
                ...(picks[itemId] ?? { quantity: "1" }),
                selected: false,
            },
        }
        form.setFieldValue("picks", updatedPicks)

        const choices: { item_id: string; quantity?: number }[] = []
        for (const item of page.items) {
            if (item.item_id === itemId) continue
            const pick = updatedPicks[item.item_id]
            if (!pick?.selected) continue
            choices.push({
                item_id: item.item_id,
                ...(mall
                    ? {}
                    : { quantity: Number.parseInt(pick.quantity, 10) }),
            })
        }

        if (choices.length === 0) {
            setConfirmed(null)
            setDirty(true)
            setMessage("已移除全部商品，请重新挑选。")
            return
        }

        const next: PendingSelectionRequest = {
            kind: "save",
            confirm: true,
            input: {
                idempotencyKey: crypto.randomUUID(),
                expectedSessionVersion: confirmed?.session_version ?? version,
                choices,
            },
        }
        keepRequest(next)
        mutation.mutate(next)
    }

    const submit = () => {
        if (!confirmed || locked || conflict || dirty) return
        const next: PendingSelectionRequest = {
            kind: "submit",
            input: {
                idempotencyKey: crypto.randomUUID(),
                expectedSessionVersion: confirmed.session_version!,
            },
        }
        keepRequest(next)
        mutation.mutate(next)
    }

    if (ended) return <Ended />

    const renderCard = (item: PublicDisplayItemView) => {
        const pick = picks[item.item_id]
        const isSelected = pick?.selected ?? false
        const image = publicImageUrl(token, item.cover_path)
        const [intPart, decPart] = item.price.split(".")

        return (
            <article
                key={item.item_id}
                className={cn(
                    "group relative flex gap-3 rounded-2xl p-3 border transition-all duration-200",
                    isSelected
                        ? "bg-gradient-to-r from-blue-50/25 via-white to-white border-blue-200/90 shadow-xs"
                        : "bg-white border-slate-200/70 hover:border-slate-300 shadow-2xs",
                )}
            >
                {/* 左侧: 1:1 方形图片/占位 */}
                <button
                    type="button"
                    aria-label={`查看${item.name}详情`}
                    className="relative size-20 sm:size-22 rounded-xl overflow-hidden bg-slate-50 shrink-0 text-left block cursor-pointer border border-slate-100 p-0"
                    onClick={() => setDetailItem(item)}
                >
                    {image ? (
                        // eslint-disable-next-line @next/next/no-img-element
                        <img
                            src={image}
                            alt=""
                            className="h-full w-full object-cover transition-transform duration-300 group-hover:scale-105"
                            loading="lazy"
                            referrerPolicy="no-referrer"
                        />
                    ) : (
                        <div className="flex h-full w-full flex-col items-center justify-center bg-gradient-to-br from-slate-50 via-slate-100/60 to-slate-50 p-2 text-center">
                            <Package className="h-6 w-6 stroke-[1.25] text-slate-400" />
                            <span className="mt-1 text-[9px] font-medium text-slate-400 tracking-wider">
                                严选品质
                            </span>
                        </div>
                    )}

                    {/* 档位微标 */}
                    {item.tier_name && (
                        <span className="absolute left-1 top-1 rounded-md bg-slate-900/80 px-1.5 py-0.5 text-[9px] font-medium text-white backdrop-blur-xs">
                            {item.tier_name}
                        </span>
                    )}

                    {/* 套餐件数 */}
                    {item.members.length > 0 && (
                        <span className="absolute left-1 bottom-1 rounded-md bg-slate-900/80 px-1.5 py-0.5 text-[9px] text-white flex items-center gap-1 backdrop-blur-xs font-medium">
                            <Layers className="h-2.5 w-2.5" />
                            {item.members.length}件装
                        </span>
                    )}
                </button>

                {/* 右侧: 商品信息与操作 */}
                <div className="flex-1 min-w-0 flex flex-col justify-between py-0.5">
                    <div>
                        <button
                            type="button"
                            onClick={() => setDetailItem(item)}
                            className="text-left w-full text-xs sm:text-sm font-semibold text-slate-900 leading-snug line-clamp-2 hover:text-blue-600 transition-colors p-0 border-0 bg-transparent cursor-pointer"
                        >
                            {item.name}
                        </button>

                        {/* Specs */}
                        {item.specification.length > 0 && (
                            <div className="mt-1 flex flex-wrap gap-1">
                                {item.specification.slice(0, 2).map((s) => (
                                    <span
                                        key={s.name}
                                        className="rounded-md bg-slate-100/80 px-1.5 py-0.5 text-[10px] text-slate-500 font-normal truncate max-w-full"
                                    >
                                        {s.value || s.name}
                                    </span>
                                ))}
                            </div>
                        )}
                    </div>

                    {/* 价格与操作 */}
                    <div className="mt-2 flex items-end justify-between gap-1.5 pt-1.5 border-t border-slate-100/80">
                        <div>
                            <div className="flex items-baseline font-bold tracking-tight">
                                <span className="text-xs font-bold text-blue-600 mr-0.5">
                                    ¥
                                </span>
                                <span className="text-base sm:text-lg font-black text-slate-900 leading-none">
                                    {intPart}
                                </span>
                                {decPart !== undefined && (
                                    <span className="text-[11px] font-semibold text-slate-400 leading-none">
                                        .{decPart}
                                    </span>
                                )}
                            </div>
                        </div>

                        {/* 选品或步进器 */}
                        <div className="flex items-center gap-1.5">
                            {isSelected && !mall ? (
                                <div className="flex items-center gap-0.5 rounded-lg bg-slate-100/80 p-0.5 border border-slate-200/80">
                                    <button
                                        type="button"
                                        className="flex h-5 w-5 items-center justify-center rounded bg-white text-slate-700 shadow-2xs hover:bg-slate-50 disabled:opacity-40"
                                        disabled={
                                            locked ||
                                            conflict ||
                                            Number.parseInt(
                                                pick.quantity,
                                                10,
                                            ) <= 1
                                        }
                                        onClick={(e) => {
                                            e.preventDefault()
                                            e.stopPropagation()
                                            const current = Number.parseInt(
                                                pick.quantity,
                                                10,
                                            )
                                            if (
                                                Number.isSafeInteger(current) &&
                                                current > 1
                                            ) {
                                                change(item.item_id, {
                                                    quantity: String(
                                                        current - 1,
                                                    ),
                                                })
                                            }
                                        }}
                                    >
                                        <Minus className="h-2.5 w-2.5" />
                                    </button>
                                    <Input
                                        id={`sales-selection-public-qty-${item.item_id}`}
                                        inputMode="numeric"
                                        className="h-5 w-8 border-0 bg-transparent text-center text-xs font-bold p-0 shadow-none focus-visible:ring-0"
                                        value={pick.quantity}
                                        onChange={(e) =>
                                            change(item.item_id, {
                                                quantity: e.target.value,
                                            })
                                        }
                                        onClick={(e) => e.stopPropagation()}
                                    />
                                    <button
                                        type="button"
                                        className="flex h-5 w-5 items-center justify-center rounded bg-white text-slate-700 shadow-2xs hover:bg-slate-50"
                                        disabled={locked || conflict}
                                        onClick={(e) => {
                                            e.preventDefault()
                                            e.stopPropagation()
                                            const current = Number.parseInt(
                                                pick.quantity,
                                                10,
                                            )
                                            const val = Number.isSafeInteger(
                                                current,
                                            )
                                                ? current
                                                : 1
                                            change(item.item_id, {
                                                quantity: String(val + 1),
                                            })
                                        }}
                                    >
                                        <Plus className="h-2.5 w-2.5" />
                                    </button>
                                </div>
                            ) : null}

                            {/* 选择按钮与 Checkbox */}
                            <label
                                aria-label={item.name}
                                htmlFor={`sales-selection-public-select-${item.item_id}`}
                                className={cn(
                                    "flex items-center justify-center gap-1 rounded-full px-3 py-1 text-xs font-semibold transition-all cursor-pointer select-none active:scale-95",
                                    isSelected
                                        ? "bg-blue-600 text-white shadow-xs shadow-blue-500/25"
                                        : "bg-white text-blue-600 border border-blue-500/40 hover:bg-blue-50/60 shadow-2xs",
                                )}
                            >
                                <input
                                    id={`sales-selection-public-select-${item.item_id}`}
                                    aria-label={item.name}
                                    type="checkbox"
                                    checked={isSelected}
                                    onChange={(e) =>
                                        change(item.item_id, {
                                            selected: e.target.checked,
                                        })
                                    }
                                    className="sr-only"
                                />
                                {isSelected ? (
                                    <>
                                        <Check className="h-3 w-3 stroke-[3]" />
                                        <span>已选</span>
                                    </>
                                ) : (
                                    <span>+ 选品</span>
                                )}
                            </label>
                        </div>
                    </div>
                </div>
            </article>
        )
    }

    return (
        <main className="fixed inset-0 h-[100dvh] max-h-[100dvh] overflow-hidden flex flex-col bg-slate-100">
            <div className="mx-auto w-full max-w-lg h-full flex flex-col bg-white shadow-xl overflow-hidden relative">
                {/* 1. 顶部电商商城头部 + 搜索框 (固定不滚动) */}
                <header className="shrink-0 bg-white border-b border-slate-200/80 px-3.5 py-2 z-20">
                    <div className="flex items-center justify-between gap-2.5">
                        <div className="flex items-center gap-2 min-w-0">
                            <div className="flex h-8 w-8 shrink-0 items-center justify-center rounded-xl bg-blue-600 text-white shadow-xs">
                                <Store className="h-4.5 w-4.5" />
                            </div>
                            <div className="min-w-0">
                                <h1 className="text-xs sm:text-sm font-bold text-slate-900 truncate">
                                    {page.customer_name}
                                </h1>
                                <p className="text-[10px] text-slate-500 flex items-center gap-1">
                                    <span className="inline-block h-1.5 w-1.5 rounded-full bg-emerald-500 animate-pulse" />
                                    <span>专属选品会场</span>
                                    <span className="text-slate-300">|</span>
                                    <span>
                                        {mall ? "商城兑换" : "按份采购"}
                                    </span>
                                </p>
                            </div>
                        </div>
                        <Badge
                            variant="outline"
                            className="shrink-0 text-[11px] border-blue-200/80 bg-blue-50/70 text-blue-700 font-medium px-2.5 py-0.5 rounded-full"
                        >
                            {mall ? "意向可选库" : "批量采购"}
                        </Badge>
                    </div>

                    {/* 搜索框 */}
                    <div className="relative mt-1.5">
                        <Search className="absolute left-3 top-2.5 h-3.5 w-3.5 text-slate-400" />
                        <Input
                            type="text"
                            placeholder="搜索几百款商品、规格或名称..."
                            value={searchQuery}
                            onChange={(e) => setSearchQuery(e.target.value)}
                            className="h-8.5 w-full rounded-full bg-slate-100/90 pl-8.5 pr-8 text-xs placeholder:text-slate-400 border border-slate-200/60 shadow-none focus-visible:bg-white focus-visible:border-blue-500 focus-visible:ring-2 focus-visible:ring-blue-500/15 transition-all"
                        />
                        {searchQuery && (
                            <button
                                type="button"
                                onClick={() => setSearchQuery("")}
                                className="absolute right-2.5 top-2 text-slate-400 hover:text-slate-600"
                            >
                                <X className="h-4 w-4" />
                            </button>
                        )}
                    </div>
                </header>

                {/* 2. 业务公告 (固定不滚动) */}
                {page.notices.length > 0 && (
                    <div className="shrink-0 bg-amber-50/90 border-b border-amber-200/60 px-3.5 py-1 text-[11px] text-amber-900 flex items-center gap-1.5 font-medium z-10">
                        <Megaphone className="h-3 w-3 shrink-0 text-amber-600" />
                        <div className="truncate flex-1 space-x-2">
                            {page.notices.map((notice, i) => (
                                <span key={notice}>
                                    {i > 0 ? " · " : ""}
                                    {notice}
                                </span>
                            ))}
                        </div>
                    </div>
                )}

                {/* 状态与未知结果恢复提示 */}
                {(message || request) && (
                    <div
                        role="status"
                        className="shrink-0 mx-3 my-2 rounded-xl border border-amber-200 bg-amber-50 p-2.5 text-xs text-amber-900 shadow-2xs z-10"
                    >
                        <div className="flex items-start gap-2">
                            <AlertCircle className="h-4 w-4 shrink-0 text-amber-600 mt-0.5" />
                            <div className="flex-1">
                                <p className="font-semibold leading-relaxed">
                                    {message ||
                                        "上次操作结果待核对，请先恢复本次请求。"}
                                </p>
                                {request && !mutation.isPending && (
                                    <Button
                                        id="sales-selection-public-reconcile"
                                        size="sm"
                                        className="mt-1.5 h-6 rounded-lg bg-amber-600 text-white hover:bg-amber-700 text-xs px-2"
                                        onClick={() => mutation.mutate(request)}
                                    >
                                        核对并恢复本次操作
                                    </Button>
                                )}
                            </div>
                        </div>
                    </div>
                )}

                {/* 冲突处理区域 */}
                {conflict && (
                    <section
                        className="shrink-0 mx-3 my-2 rounded-2xl border-2 border-amber-300 bg-amber-50/80 p-3 space-y-2 z-10"
                        aria-label="最新保存的选择"
                    >
                        <div className="flex items-center gap-1.5 text-amber-900">
                            <ShieldAlert className="h-4 w-4 text-amber-600" />
                            <h2 className="font-bold text-xs">
                                其他页面最新保存的选择
                            </h2>
                        </div>
                        <p className="text-[11px] text-amber-800">
                            其他设备刚刚更新了该选品册。您的本地修改已保留，请仔细核对最新清单：
                        </p>
                        {latest ? (
                            <ChoiceSummary page={latest} />
                        ) : (
                            <p className="text-xs text-slate-500">
                                最新清单暂时无法读取，请重试。
                            </p>
                        )}
                        <div className="flex flex-wrap gap-2 pt-1">
                            <Button
                                id="sales-selection-public-reload-conflict"
                                variant="outline"
                                size="sm"
                                className="h-7 text-xs rounded-lg"
                                onClick={async () =>
                                    setLatest((await refresh()) ?? null)
                                }
                            >
                                重新读取最新清单
                            </Button>
                            <Button
                                id="sales-selection-public-acknowledge"
                                size="sm"
                                className="h-7 text-xs rounded-lg bg-amber-600 text-white hover:bg-amber-700"
                                disabled={
                                    !latest || latest.kind !== "SELECTING"
                                }
                                onClick={() => {
                                    setVersion(latest!.session_version!)
                                    setConflict(false)
                                    setLatest(null)
                                    setDirty(true)
                                    setMessage(
                                        "已保留你的本地选择。请完成编辑后重新保存核对；保存将替换上面的最新清单。",
                                    )
                                }}
                            >
                                已核对，保留本地选择继续编辑
                            </Button>
                        </div>
                    </section>
                )}

                {/* 3. 核心双栏分层联动区域（只有本区域内部独立滚动，整个页面不乱跑） */}
                <div className="flex-1 flex overflow-hidden min-h-0">
                    {/* 左侧品类侧边栏 */}
                    <aside
                        ref={leftAsideRef}
                        className="w-20 sm:w-22 shrink-0 bg-[#f8fafc] border-r border-slate-200/70 overflow-y-auto no-scrollbar select-none"
                    >
                        {sections.map((section) => {
                            const isActive =
                                activeTabId === section.id && !searchQuery
                            const sectionSelectedCount = section.items.filter(
                                (item) => picks[item.item_id]?.selected,
                            ).length

                            return (
                                <button
                                    key={section.id}
                                    ref={(el) => {
                                        leftTabRefs.current[section.id] = el
                                    }}
                                    type="button"
                                    onClick={() => scrollToSection(section.id)}
                                    className={cn(
                                        "relative flex w-full flex-col items-center justify-center py-3.5 px-2 text-center transition-colors border-0 cursor-pointer",
                                        isActive
                                            ? "bg-white text-slate-900 font-semibold before:absolute before:left-0 before:top-3 before:bottom-3 before:w-1 before:rounded-r-full before:bg-blue-600"
                                            : "text-slate-600 hover:text-slate-900 hover:bg-slate-100/60 font-normal bg-transparent",
                                    )}
                                >
                                    <span className="text-xs line-clamp-2 leading-tight">
                                        {section.name}
                                    </span>
                                    {sectionSelectedCount > 0 && (
                                        <span className="absolute top-1.5 right-1.5 flex h-4 min-w-4 items-center justify-center rounded-full bg-blue-600 px-1 text-[9px] font-bold text-white shadow-xs">
                                            {sectionSelectedCount}
                                        </span>
                                    )}
                                </button>
                            )
                        })}
                    </aside>

                    {/* 右侧商品瀑布流 (监听滚动以联动左侧高亮) */}
                    <section
                        ref={rightContainerRef}
                        onScroll={handleRightScroll}
                        onWheel={() => {
                            isManualScrolling.current = false
                        }}
                        onTouchStart={() => {
                            isManualScrolling.current = false
                        }}
                        className="flex-1 overflow-y-auto p-2.5 sm:p-3 space-y-4 bg-white"
                    >
                        <fieldset
                            disabled={locked || conflict}
                            className="space-y-4"
                        >
                            {/* 如果处于搜索状态，显示搜索结果 */}
                            {searchResults ? (
                                <div className="space-y-2.5">
                                    <div className="flex items-center justify-between pb-1 border-b border-slate-100">
                                        <span className="text-xs font-bold text-slate-800">
                                            搜索结果 &quot;{searchQuery}&quot; (
                                            {searchResults.length})
                                        </span>
                                    </div>
                                    {searchResults.map(renderCard)}
                                    {searchResults.length === 0 && (
                                        <div className="py-16 text-center text-xs text-slate-400">
                                            未找到匹配的商品
                                        </div>
                                    )}
                                </div>
                            ) : (
                                /* 默认全楼层连续滚动陈列 */
                                sections.map((section) => (
                                    <div
                                        key={section.id}
                                        ref={(el) => {
                                            sectionRefs.current[section.id] = el
                                        }}
                                        data-section-id={section.id}
                                        className="space-y-2"
                                    >
                                        {/* 品类楼层吸顶标题 */}
                                        <div className="sticky top-0 z-10 bg-white/95 backdrop-blur-sm py-2 px-0.5 flex items-center justify-between border-b border-slate-150/70">
                                            <div className="flex items-center gap-2">
                                                <span className="h-3.5 w-1 rounded-full bg-blue-600" />
                                                <h2 className="text-xs font-bold text-slate-900 tracking-tight">
                                                    {section.name}
                                                </h2>
                                                <span className="text-[11px] font-normal text-slate-400">
                                                    共 {section.items.length} 款
                                                </span>
                                            </div>
                                        </div>

                                        {/* 楼层内商品列表 */}
                                        <div className="space-y-2.5">
                                            {section.items.map(renderCard)}
                                        </div>
                                    </div>
                                ))
                            )}
                        </fieldset>
                    </section>
                </div>

                {/* 4. 底部吸底结算栏 (固定不滚动) */}
                <aside
                    aria-label="核对并提交"
                    className="shrink-0 border-t border-slate-200/80 bg-white/95 backdrop-blur-md px-4 py-2.5 shadow-[0_-6px_20px_rgba(0,0,0,0.06)] z-30"
                >
                    <div className="flex items-center justify-between gap-3">
                        {/* 左侧：点击呼出已选清单 */}
                        <button
                            type="button"
                            className="flex items-center gap-2.5 text-left cursor-pointer select-none active:opacity-80 transition-opacity border-0 bg-transparent p-0"
                            onClick={() => setCartDrawerOpen(true)}
                        >
                            <div className="relative flex h-10 w-10 items-center justify-center rounded-2xl bg-blue-600 text-white shadow-sm active:scale-95 transition-transform">
                                <ShoppingBag className="h-5 w-5" />
                                {selectedCount > 0 && (
                                    <span className="absolute -right-1 -top-1 flex h-4.5 min-w-4.5 items-center justify-center rounded-full bg-amber-400 px-1 text-[10px] font-bold text-slate-950 shadow-xs animate-in zoom-in">
                                        {selectedCount}
                                    </span>
                                )}
                            </div>
                            <div>
                                {mall ? (
                                    <div>
                                        <p className="text-xs sm:text-sm font-bold text-slate-900">
                                            已选{" "}
                                            <span className="text-blue-600">
                                                {selectedCount}
                                            </span>{" "}
                                            款
                                        </p>
                                        <p className="text-[10px] text-slate-400">
                                            {dirty
                                                ? "修改待保存"
                                                : "点击查看清单"}
                                        </p>
                                    </div>
                                ) : (
                                    <div>
                                        <p className="text-sm sm:text-base font-bold text-blue-600 leading-none">
                                            已选 {selectedCount} 项
                                        </p>
                                        <p className="text-[10px] text-slate-500 mt-0.5">
                                            {dirty
                                                ? "尚有修改未保存"
                                                : "已选内容已同步"}
                                        </p>
                                    </div>
                                )}
                            </div>
                        </button>

                        {/* 右侧：电商结算按钮组 */}
                        <div className="flex items-center gap-2">
                            <Button
                                id="sales-selection-public-save"
                                variant="outline"
                                size="sm"
                                className="rounded-full border-slate-200 bg-slate-50 hover:bg-slate-100 text-xs font-medium text-slate-700 px-4 h-9 shadow-2xs"
                                disabled={locked || conflict}
                                onClick={() => persist(false)}
                            >
                                保存选择
                            </Button>
                            {!confirmed && (
                                <Button
                                    id="sales-selection-public-review"
                                    size="sm"
                                    className="rounded-full bg-blue-600 hover:bg-blue-700 text-xs font-bold text-white shadow-sm shadow-blue-500/25 px-5 h-9 active:scale-95 transition-all"
                                    disabled={locked || conflict}
                                    onClick={() => persist(true)}
                                >
                                    核对并提交
                                </Button>
                            )}
                        </div>
                    </div>
                </aside>
            </div>

            {/* 5. 全屏沉浸式方案核对与确认中心（针对海量商品提供高管概览与分类折叠） */}
            {confirmed && (
                <SelectionReviewCenter
                    token={token}
                    page={confirmed}
                    locked={locked}
                    conflict={conflict}
                    dirty={dirty}
                    onBack={() => setConfirmed(null)}
                    onSubmit={submit}
                    onRemoveItem={removeItemInReview}
                />
            )}

            {/* 5. 商品详情弹窗 (Detail Dialog) */}
            <Dialog
                open={detailItem !== null}
                onOpenChange={(open) => {
                    if (!open) setDetailItem(null)
                }}
            >
                <DialogContent className="max-w-lg rounded-3xl p-5 max-h-[85vh] overflow-y-auto space-y-4">
                    {detailItem && (
                        <>
                            <DialogHeader>
                                <DialogTitle className="text-base font-bold text-slate-900 line-clamp-1">
                                    {detailItem.name}
                                </DialogTitle>
                            </DialogHeader>

                            {/* 大图预览 */}
                            <div className="relative aspect-video w-full rounded-2xl overflow-hidden bg-slate-50">
                                {publicImageUrl(
                                    token,
                                    detailItem.cover_path,
                                ) ? (
                                    // eslint-disable-next-line @next/next/no-img-element
                                    <img
                                        src={publicImageUrl(
                                            token,
                                            detailItem.cover_path,
                                        )}
                                        alt=""
                                        className="h-full w-full object-cover"
                                    />
                                ) : (
                                    <div className="flex h-full w-full flex-col items-center justify-center text-slate-400 bg-slate-100">
                                        <Package className="h-10 w-10 stroke-[1.5]" />
                                        <span className="mt-2 text-xs font-medium text-slate-500">
                                            精选商品快照
                                        </span>
                                    </div>
                                )}
                            </div>

                            {/* 价格与信息 */}
                            <div className="flex items-baseline justify-between">
                                <div className="flex items-baseline text-blue-700 font-bold">
                                    <span className="text-xs mr-0.5">¥</span>
                                    <span className="text-2xl">
                                        {detailItem.price}
                                    </span>
                                    {mall && (
                                        <span className="ml-2 text-xs text-slate-400 font-normal">
                                            商城兑换参考价值
                                        </span>
                                    )}
                                </div>
                                {detailItem.tier_name && (
                                    <Badge className="bg-slate-900 text-white">
                                        {detailItem.tier_name}
                                    </Badge>
                                )}
                            </div>

                            {/* 详细规格 */}
                            {detailItem.specification.length > 0 && (
                                <div className="rounded-2xl bg-slate-50 p-3 text-xs space-y-1.5 border border-slate-150">
                                    <p className="font-semibold text-slate-700">
                                        规格参数
                                    </p>
                                    <div className="grid grid-cols-2 gap-2 text-slate-600">
                                        {detailItem.specification.map(
                                            (spec) => (
                                                <p key={spec.name}>
                                                    <span className="text-slate-400">
                                                        {spec.name}：
                                                    </span>
                                                    {spec.value}
                                                </p>
                                            ),
                                        )}
                                    </div>
                                </div>
                            )}

                            {/* 套餐明细清单 */}
                            {detailItem.members.length > 0 && (
                                <div className="rounded-2xl bg-blue-50/40 p-3 text-xs space-y-2 border border-blue-100">
                                    <p className="font-bold text-blue-950 flex items-center gap-1.5">
                                        <Layers className="h-4 w-4 text-blue-600" />
                                        套餐包含 {detailItem.members.length}{" "}
                                        款组合商品
                                    </p>
                                    <div className="space-y-1.5">
                                        {detailItem.members.map((m, idx) => (
                                            <div
                                                key={idx}
                                                className="flex items-center justify-between text-slate-700 border-b border-blue-100/60 pb-1 last:border-0 last:pb-0"
                                            >
                                                <span className="font-medium">
                                                    {m.name}
                                                </span>
                                                <span className="text-slate-400">
                                                    {m.specification
                                                        .map((s) => s.value)
                                                        .join("/")}{" "}
                                                    · 1{m.unit}
                                                </span>
                                            </div>
                                        ))}
                                    </div>
                                </div>
                            )}

                            <div className="pt-2">
                                <Button
                                    className="w-full rounded-2xl bg-blue-600 hover:bg-blue-700 text-white font-bold h-11 shadow-sm"
                                    onClick={() => {
                                        change(detailItem.item_id, {
                                            selected:
                                                !picks[detailItem.item_id]
                                                    ?.selected,
                                        })
                                        setDetailItem(null)
                                    }}
                                >
                                    {picks[detailItem.item_id]?.selected
                                        ? "从选品中取消"
                                        : "加入选品"}
                                </Button>
                            </div>
                        </>
                    )}
                </DialogContent>
            </Dialog>

            {/* 6. 已选清单抽屉 (Cart Sheet) */}
            <Dialog open={cartDrawerOpen} onOpenChange={setCartDrawerOpen}>
                <DialogContent className="max-w-lg rounded-3xl p-5 max-h-[75vh] flex flex-col">
                    <DialogHeader>
                        <DialogTitle className="text-base font-bold text-slate-900 flex items-center gap-2">
                            <ShoppingBag className="h-5 w-5 text-blue-600" />
                            已选商品清单 ({selectedCount} 款)
                        </DialogTitle>
                    </DialogHeader>

                    <div className="flex-1 overflow-y-auto py-2 space-y-3">
                        {page.items
                            .filter((item) => picks[item.item_id]?.selected)
                            .map((item) => {
                                const pick = picks[item.item_id]
                                return (
                                    <div
                                        key={item.item_id}
                                        className="flex items-center justify-between gap-3 border-b border-slate-100 pb-2.5"
                                    >
                                        <div className="min-w-0 flex-1">
                                            <p className="font-medium text-xs sm:text-sm text-slate-900 truncate">
                                                {item.name}
                                            </p>
                                            <p className="text-xs font-bold text-blue-700 mt-0.5">
                                                ¥ {item.price}
                                            </p>
                                        </div>
                                        <div className="flex items-center gap-2">
                                            {!mall && (
                                                <span className="text-xs font-semibold text-slate-700 bg-slate-100 px-2 py-0.5 rounded-md">
                                                    {pick?.quantity} 份
                                                </span>
                                            )}
                                            <Button
                                                variant="ghost"
                                                size="sm"
                                                className="h-7 text-xs text-blue-600 hover:bg-blue-50 px-2"
                                                onClick={() =>
                                                    change(item.item_id, {
                                                        selected: false,
                                                    })
                                                }
                                            >
                                                移除
                                            </Button>
                                        </div>
                                    </div>
                                )
                            })}
                        {selectedCount === 0 && (
                            <p className="text-center py-8 text-xs text-slate-400">
                                尚未选择任何商品
                            </p>
                        )}
                    </div>

                    <div className="pt-2 border-t border-slate-100">
                        <Button
                            className="w-full rounded-full bg-slate-900 text-white"
                            onClick={() => setCartDrawerOpen(false)}
                        >
                            继续选品
                        </Button>
                    </div>
                </DialogContent>
            </Dialog>
        </main>
    )
}

/** 将服务端秒时间戳格式化为客户本地时间。 */
const formatTime = (value: number) => {
    return new Date(value * 1000).toLocaleString("zh-CN")
}
