/**
 * 公开移动端选品页（客户侧，无需登录）。
 * 移动优先：单品列表 / 套餐按档分组；按份步进 + 后端合计，兑换仅多选。
 * 保存携会话版本 + 幂等键 + 完整选择；冲突保留本地供核对，不自动覆盖。
 */

"use client"

import * as React from "react"
import { Button } from "@/components/ui/button"
import { Card, CardContent } from "@/components/ui/card"
import { Badge } from "@/components/ui/badge"
import { Input } from "@/components/ui/input"
import { Skeleton } from "@/components/ui/skeleton"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogFooter,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { MoneyValue } from "@/components/business/values"
import {
    usePublicReceipt,
    usePublicSelection,
    useSessionSave,
    useSubmit,
} from "@/features/sales-selection/hooks/queries"
import { publicImageUrl } from "@/features/sales-selection/api/public"
import {
    decrementQuantity,
    incrementQuantity,
    isValidQuantity,
} from "@/features/sales-selection/lib/money"
import { createIdempotencyKey } from "@/features/sales-selection/lib/validation"
import type {
    PublicDisplayItemView,
    PublicSelection,
} from "@/features/sales-selection/types"

/** 份数默认值。 */
const DEFAULT_QUANTITY = "1"

/**
 * 解析公开图片地址：绝对地址直用，引用走令牌授权预览接口。
 * @param token 公开令牌
 * @param ref 后端下发的封面引用
 */
const resolvePublicImage = (token: string, ref: string): string =>
    ref.startsWith("http") ? ref : publicImageUrl(token, ref)

/** 规格展示文案：后端仅下发规格数组，展示时拼接为一行。 */
const specLabelFor = (item: PublicDisplayItemView): string =>
    item.specification
        .map((attr) => (attr.value ? `${attr.name}:${attr.value}` : attr.name))
        .join(" · ")

/** 本地份数字符串转后端份数（调用前已用 isValidQuantity 校验）。 */
const toWireQuantity = (raw: string): number => {
    const text = raw.trim() || DEFAULT_QUANTITY
    const parsed = parseInt(text, 10)
    return Number.isSafeInteger(parsed) ? parsed : 1
}

/** 本地选择：肯定式映射（兑换模式值固定为 "1"，仅表选中）。 */
type LocalSelection = Record<string, string>

/**
 * 由服务端已保存构建本地初始选择。
 */
const _initialFromSaved = (
    saved: PublicSelection["choices"],
): LocalSelection => {
    const next: LocalSelection = {}
    for (const item of saved) {
        next[item.item_id] =
            item.quantity === null || item.quantity === undefined
                ? DEFAULT_QUANTITY
                : String(item.quantity)
    }
    return next
}

/** 判断错误是否为版本冲突（旧版本保存/提交）。 */
const isVersionConflict = (error: unknown): boolean =>
    typeof error === "object" &&
    error !== null &&
    "status" in error &&
    error.status === 409

/** 判断错误是否为网络失败（可原键重试）。 */
const isNetworkFailure = (error: unknown): boolean =>
    typeof error === "object" &&
    error !== null &&
    "kind" in error &&
    error.kind === "Network"

/**
 * 结束态：无效/关闭/到期/撤销后只展示说明，不留可写表单。
 */
const EndedView = ({ title, hint }: { title: string; hint: string }) => (
    <div className="mx-auto flex min-h-svh w-full max-w-md flex-col items-center justify-center gap-2 p-6 text-center">
        <h1 className="text-lg font-semibold">{title}</h1>
        <p className="text-sm leading-6 text-muted-foreground">{hint}</p>
        <p className="text-xs text-muted-foreground">
            如有疑问请联系销售核对，不要重复点击旧链接。
        </p>
    </div>
)

/**
 * 公开回执（只读）：编号/客户/时间/明细，按提交方式展示份数与合计。
 */
const ReceiptView = ({ page }: { page: PublicSelection }) => {
    const receipt = page.receipt
    if (!receipt) {
        return (
            <EndedView
                title="选品已提交"
                hint="本次选择已提交，详细回执暂时无法展示，请联系销售核对。"
            />
        )
    }
    const byQuantity = page.submit_mode === "BY_QUANTITY"
    const itemById = new Map<string, PublicDisplayItemView>()
    for (const item of page.items) itemById.set(item.item_id, item)
    return (
        <div className="mx-auto flex min-h-svh w-full max-w-md flex-col gap-3 p-4 pb-10">
            <div className="flex flex-col gap-1 pt-4">
                <Badge variant="success" className="self-start">
                    已提交
                </Badge>
                <h1 className="text-lg font-semibold">选品已提交</h1>
                <p className="num text-sm text-muted-foreground">
                    {receipt.proposal_no} · {receipt.submitted_at}
                </p>
                <p className="text-sm">{receipt.customer_name}</p>
            </div>
            <Card>
                <CardContent className="flex flex-col gap-2 p-4">
                    {receipt.items.map((line) => {
                        const display = itemById.get(line.item_id)
                        const amount =
                            line.line_amount ?? display?.price ?? "0.00"
                        return (
                            <div
                                key={line.item_id}
                                className="flex items-center justify-between gap-3 border-b pb-2 text-sm last:border-0 last:pb-0"
                            >
                                <span className="min-w-0 flex-1 truncate">
                                    {display?.name ?? line.item_id}
                                    {line.quantity !== null &&
                                    line.quantity !== undefined
                                        ? ` × ${line.quantity} 份`
                                        : ""}
                                </span>
                                <MoneyValue value={amount} taxBasis="gross" />
                            </div>
                        )
                    })}
                    {byQuantity ? (
                        <div className="flex items-center justify-between pt-1">
                            <span className="text-sm text-muted-foreground">
                                合计（含税）
                            </span>
                            <MoneyValue
                                value={receipt.total_amount ?? "0.00"}
                                taxBasis="gross"
                            />
                        </div>
                    ) : null}
                </CardContent>
            </Card>
        </div>
    )
}

/**
 * 陈列卡片：封面只读后端字段，不展示供应商/成本/编码/修订。
 */
const ItemCard = ({
    token,
    item,
    byQuantity,
    checked,
    quantity,
    onToggle,
    onQuantity,
}: {
    token: string
    item: PublicDisplayItemView
    byQuantity: boolean
    checked: boolean
    quantity: string
    onToggle: () => void
    onQuantity: (next: string) => void
}) => {
    const specLabel = specLabelFor(item)
    const hasMembers = item.members.length > 0
    return (
        <Card className={checked ? "border-primary ring-1 ring-primary" : ""}>
            <div className="aspect-[4/3] w-full overflow-hidden rounded-t-xl bg-muted">
                {item.cover_path ? (
                    // eslint-disable-next-line @next/next/no-img-element
                    <img
                        src={resolvePublicImage(token, item.cover_path)}
                        alt={item.name}
                        className="h-full w-full object-cover"
                        loading="lazy"
                        referrerPolicy="no-referrer"
                    />
                ) : (
                    <div className="flex h-full w-full items-center justify-center text-xs text-muted-foreground">
                        暂无图片
                    </div>
                )}
            </div>
            <CardContent className="flex flex-col gap-1.5 p-3">
                <p className="truncate text-sm font-medium">{item.name}</p>
                {specLabel ? (
                    <p className="truncate text-xs text-muted-foreground">
                        {specLabel}
                    </p>
                ) : null}
                {hasMembers ? (
                    <p className="line-clamp-2 text-xs leading-5 text-muted-foreground">
                        {item.members.map((member) => member.name).join(" · ")}
                    </p>
                ) : item.tier_name ? (
                    <p className="truncate text-xs text-muted-foreground">
                        {item.tier_name}
                    </p>
                ) : null}
                <MoneyValue value={item.price} taxBasis="gross" />
                <div className="flex items-center justify-between gap-2 pt-1">
                    <Button
                        id={`public-item-${item.item_id}-toggle`}
                        type="button"
                        variant={checked ? "default" : "outline"}
                        size="sm"
                        className="min-w-0 flex-1"
                        aria-pressed={checked}
                        onClick={onToggle}
                    >
                        {checked ? "已选" : "选择"}
                    </Button>
                    {byQuantity && checked ? (
                        <div className="flex shrink-0 items-center gap-1">
                            <Button
                                id={`public-item-${item.item_id}-decrease`}
                                type="button"
                                variant="outline"
                                size="sm"
                                aria-label={`减少${item.name}份数`}
                                onClick={() =>
                                    onQuantity(decrementQuantity(quantity))
                                }
                            >
                                −
                            </Button>
                            <Input
                                id={`public-item-${item.item_id}-quantity`}
                                className="w-16 text-center"
                                value={quantity}
                                inputMode="numeric"
                                aria-label={`${item.name}份数`}
                                onChange={(event) => {
                                    const next = event.target.value.trim()
                                    if (next === "" || isValidQuantity(next)) {
                                        onQuantity(next === "" ? "" : next)
                                    }
                                }}
                                onBlur={() => {
                                    if (!isValidQuantity(quantity)) {
                                        onQuantity(DEFAULT_QUANTITY)
                                    }
                                }}
                            />
                            <Button
                                id={`public-item-${item.item_id}-increase`}
                                type="button"
                                variant="outline"
                                size="sm"
                                aria-label={`增加${item.name}份数`}
                                onClick={() =>
                                    onQuantity(incrementQuantity(quantity))
                                }
                            >
                                ＋
                            </Button>
                        </div>
                    ) : null}
                </div>
            </CardContent>
        </Card>
    )
}

/**
 * 公开选品客户端：浏览、选择、保存、确认并提交。
 * @param token 公开令牌
 */
export const PublicSelectionClient = ({ token }: { token: string }) => {
    const selectionQuery = usePublicSelection(token)
    const saveMutation = useSessionSave(token)
    const submitMutation = useSubmit(token)

    const [local, setLocal] = React.useState<LocalSelection>({})
    const [hydratedVersion, setHydratedVersion] = React.useState<number | null>(
        null,
    )
    const [saveKey, setSaveKey] = React.useState(createIdempotencyKey)
    const [submitKey, setSubmitKey] = React.useState(createIdempotencyKey)
    const [confirmOpen, setConfirmOpen] = React.useState(false)
    const [conflict, setConflict] = React.useState<string | null>(null)
    const [notice, setNotice] = React.useState<string | null>(null)

    const data = selectionQuery.data
    const sessionVersion = data?.session_version ?? null
    const { mutateAsync: saveAsync } = saveMutation
    const { mutateAsync: submitAsync } = submitMutation
    const { refetch: refetchSelection } = selectionQuery

    // 服务端版本变化时只做初始同步；冲突后不自动覆盖本地（供核对）。
    React.useEffect(() => {
        if (!data || data.kind !== "SELECTING" || conflict !== null) return
        if (hydratedVersion === null) {
            setHydratedVersion(data.session_version ?? null)
            return
        }
        if (hydratedVersion !== (data.session_version ?? null)) {
            setHydratedVersion(data.session_version ?? null)
        }
    }, [conflict, data, hydratedVersion])

    const byQuantity = data?.submit_mode === "BY_QUANTITY"
    const selectedIds = React.useMemo(() => Object.keys(local), [local])
    const selectedCount = selectedIds.length
    const dirty = React.useMemo(() => {
        if (!data) return false
        if (data.choices.length !== selectedIds.length) return true
        return data.choices.some((item) => {
            const current = local[item.item_id]
            if (current === undefined) return true
            if (!byQuantity) return false
            const savedQty =
                item.quantity === null || item.quantity === undefined
                    ? DEFAULT_QUANTITY
                    : String(item.quantity)
            return current !== savedQty
        })
    }, [byQuantity, data, local, selectedIds.length])

    const itemById = React.useMemo(() => {
        const map = new Map<string, PublicDisplayItemView>()
        for (const item of data?.items ?? []) map.set(item.item_id, item)
        return map
    }, [data?.items])

    const groups = React.useMemo(() => {
        if (!data || data.form !== "PACKAGE") return null
        const map = new Map<string, PublicDisplayItemView[]>()
        for (const item of data.items) {
            const key = item.tier_name ?? "未分组"
            const bucket = map.get(key) ?? []
            bucket.push(item)
            map.set(key, bucket)
        }
        return [...map.entries()]
    }, [data])

    const toggleItem = React.useCallback((itemId: string) => {
        setConflict(null)
        setNotice(null)
        setLocal((prev) => {
            if (prev[itemId] !== undefined) {
                const next = { ...prev }
                delete next[itemId]
                return next
            }
            return { ...prev, [itemId]: DEFAULT_QUANTITY }
        })
    }, [])

    const changeQuantity = React.useCallback((itemId: string, next: string) => {
        setConflict(null)
        setLocal((prev) => ({ ...prev, [itemId]: next }))
    }, [])

    /** 保存当前完整选择：同键重试不另生键，成功后轮换。 */
    const persistSelection = React.useCallback(
        async (keyOverride?: string): Promise<boolean> => {
            if (sessionVersion === null || sessionVersion === undefined)
                return false
            const key = keyOverride ?? saveKey
            const choices = selectedIds.map((itemId) => ({
                item_id: itemId,
                ...(byQuantity
                    ? {
                          quantity: toWireQuantity(
                              local[itemId] || DEFAULT_QUANTITY,
                          ),
                      }
                    : {}),
            }))
            try {
                await saveAsync({
                    expected_session_version: sessionVersion,
                    idempotency_key: key,
                    choices,
                })
                setSaveKey(createIdempotencyKey())
                setConflict(null)
                setNotice(null)
                return true
            } catch (error) {
                if (isVersionConflict(error)) {
                    setConflict(
                        "页面不是最新，他人的修改已先保存。下方保留你的选择供核对，可复制份数后刷新重试，不要直接覆盖。",
                    )
                    void refetchSelection()
                } else if (isNetworkFailure(error)) {
                    setNotice(
                        "网络中断，已保留本次保存凭证，请恢复网络后重试，不要重复选择。",
                    )
                }
                return false
            }
        },
        [
            byQuantity,
            sessionVersion,
            local,
            refetchSelection,
            saveAsync,
            saveKey,
            selectedIds,
        ],
    )

    /** 首击确认：空选择拒绝；有未保存先存，再展示后端确认清单。 */
    const handleConfirmEntry = React.useCallback(async () => {
        setNotice(null)
        if (selectedCount === 0) {
            setNotice("请至少选择 1 项后再提交。")
            return
        }
        if (
            byQuantity &&
            selectedIds.some((id) => !isValidQuantity(local[id] || ""))
        ) {
            setNotice("份数必须为 1 到 100000 的整数，请检查后再提交。")
            return
        }
        if (dirty) {
            const ok = await persistSelection()
            if (!ok) return
            await refetchSelection()
        }
        setConfirmOpen(true)
    }, [
        byQuantity,
        dirty,
        local,
        persistSelection,
        refetchSelection,
        selectedCount,
        selectedIds,
    ])

    /** 确认清单内提交：版本变化重示再提交，同键重试。 */
    const handleSubmit = React.useCallback(async () => {
        if (sessionVersion === null || sessionVersion === undefined) return
        try {
            await submitAsync({
                expected_session_version: sessionVersion,
                idempotency_key: submitKey,
            })
            setSubmitKey(createIdempotencyKey())
            setConfirmOpen(false)
        } catch (error) {
            if (isVersionConflict(error)) {
                setConfirmOpen(false)
                setConflict(
                    "提交时清单已变化，已重新拉取最新结果。请核对后再提交。",
                )
                void refetchSelection()
            } else if (isNetworkFailure(error)) {
                setNotice(
                    "网络中断，提交结果未知。请用原凭证重试或查看回执，不要另行提交。",
                )
            }
        }
    }, [sessionVersion, refetchSelection, submitAsync, submitKey])

    if (selectionQuery.isPending) {
        return (
            <div className="mx-auto flex w-full max-w-md flex-col gap-3 p-4">
                <Skeleton className="h-8 w-40" />
                <div className="grid grid-cols-2 gap-2">
                    <Skeleton className="h-56 w-full" />
                    <Skeleton className="h-56 w-full" />
                    <Skeleton className="h-56 w-full" />
                    <Skeleton className="h-56 w-full" />
                </div>
            </div>
        )
    }

    if (selectionQuery.isError || !data) {
        return (
            <EndedView
                title="链接无效或已失效"
                hint="该选品链接不存在、已更换或已到期，无法继续选择。请联系销售确认最新链接。"
            />
        )
    }

    if (data.kind === "ENDED") {
        return (
            <EndedView
                title="本次选品已结束"
                hint="该选品已关闭或链接已失效，不再接受选择与提交。如有需要请联系销售。"
            />
        )
    }

    if (data.kind === "RECEIPT") {
        if (data.receipt) return <ReceiptView page={data} />
        return <SubmittedReceipt token={token} />
    }

    const cards = (items: readonly PublicDisplayItemView[]) => (
        <div className="grid min-w-0 grid-cols-2 gap-2">
            {items.map((item) => (
                <ItemCard
                    key={item.item_id}
                    token={token}
                    item={item}
                    byQuantity={byQuantity}
                    checked={local[item.item_id] !== undefined}
                    quantity={local[item.item_id] ?? DEFAULT_QUANTITY}
                    onToggle={() => toggleItem(item.item_id)}
                    onQuantity={(next) => changeQuantity(item.item_id, next)}
                />
            ))}
        </div>
    )

    return (
        <div className="mx-auto flex min-h-svh w-full max-w-md flex-col bg-background">
            <div className="flex min-w-0 flex-1 flex-col gap-3 p-4 pb-44">
                <div className="flex flex-col gap-1 pt-2">
                    <h1 className="text-lg font-semibold">
                        {data.form === "PACKAGE" ? "套餐选品" : "商品选品"}
                    </h1>
                </div>

                {data.items.length === 0 ? (
                    <Card>
                        <CardContent className="p-6 text-center text-sm text-muted-foreground">
                            本次暂无可选陈列，请联系销售确认。
                        </CardContent>
                    </Card>
                ) : groups ? (
                    groups.map(([tierName, tierItems]) => (
                        <section
                            key={tierName}
                            aria-label={tierName}
                            className="flex min-w-0 flex-col gap-2"
                        >
                            <div className="flex items-center gap-2">
                                <h2 className="text-sm font-semibold">
                                    {tierName}
                                </h2>
                                <Badge variant="secondary">
                                    {tierItems.length} 套
                                </Badge>
                            </div>
                            {cards(tierItems)}
                        </section>
                    ))
                ) : (
                    cards(data.items)
                )}

                {conflict ? (
                    <Card className="border-destructive">
                        <CardContent className="flex flex-col gap-2 p-4">
                            <p className="text-sm font-medium text-destructive">
                                选择有冲突，需人工核对
                            </p>
                            <p className="text-xs leading-5 text-muted-foreground">
                                {conflict}
                            </p>
                            <div className="flex flex-col gap-1 text-xs">
                                <span className="font-medium">
                                    你的选择（已保留，共 {selectedCount} 项）：
                                </span>
                                {selectedIds.map((id) => (
                                    <span
                                        key={id}
                                        className="truncate text-muted-foreground"
                                    >
                                        {itemById.get(id)?.name ?? id}
                                        {byQuantity ? ` × ${local[id]} 份` : ""}
                                    </span>
                                ))}
                                <span className="pt-1 font-medium">
                                    他人已保存（v{data.session_version ?? "—"}
                                    ，共 {data.choices.length} 项）：
                                </span>
                                {data.choices.map((item) => (
                                    <span
                                        key={item.item_id}
                                        className="truncate text-muted-foreground"
                                    >
                                        {itemById.get(item.item_id)?.name ??
                                            item.item_id}
                                        {item.quantity !== null &&
                                        item.quantity !== undefined
                                            ? ` × ${item.quantity} 份`
                                            : ""}
                                    </span>
                                ))}
                            </div>
                            <Button
                                id="public-selection-conflict-keep"
                                type="button"
                                variant="outline"
                                size="sm"
                                disabled={saveMutation.isPending}
                                onClick={() => {
                                    const fresh = createIdempotencyKey()
                                    setSaveKey(fresh)
                                    setConflict(null)
                                    void persistSelection(fresh)
                                }}
                            >
                                我已核对，用我的选择重新保存
                            </Button>
                        </CardContent>
                    </Card>
                ) : null}

                {notice ? (
                    <p
                        role="alert"
                        className="text-xs leading-5 text-destructive"
                    >
                        {notice}
                    </p>
                ) : null}
            </div>

            <div className="fixed inset-x-0 bottom-0 z-10 border-t bg-card pb-[env(safe-area-inset-bottom)]">
                <div className="mx-auto flex w-full max-w-md flex-col gap-2 p-3">
                    <div className="flex min-w-0 items-center justify-between gap-2 text-sm">
                        <span className="truncate text-muted-foreground">
                            已选 {selectedCount} 项{dirty ? "（未保存）" : ""}
                        </span>
                        {byQuantity ? (
                            dirty ? (
                                <span className="shrink-0 text-xs text-muted-foreground">
                                    保存后显示合计
                                </span>
                            ) : data.total_amount ? (
                                <MoneyValue
                                    value={data.total_amount}
                                    taxBasis="gross"
                                />
                            ) : null
                        ) : null}
                    </div>
                    <div className="flex gap-2">
                        <Button
                            id="public-selection-save"
                            type="button"
                            variant="outline"
                            className="shrink-0"
                            disabled={
                                saveMutation.isPending || selectedCount === 0
                            }
                            onClick={() => void persistSelection()}
                        >
                            {saveMutation.isPending ? "保存中…" : "仅保存选择"}
                        </Button>
                        <Button
                            id="public-selection-submit-entry"
                            type="button"
                            className="min-w-0 flex-1"
                            disabled={
                                saveMutation.isPending ||
                                submitMutation.isPending ||
                                selectedCount === 0
                            }
                            onClick={() => void handleConfirmEntry()}
                        >
                            确认并提交选品
                        </Button>
                    </div>
                </div>
            </div>

            <Dialog open={confirmOpen} onOpenChange={setConfirmOpen}>
                <DialogContent
                    className="max-h-[90svh] overflow-y-auto"
                    closeButtonId="public-selection-confirm-close"
                >
                    <DialogHeader>
                        <DialogTitle>确认提交清单</DialogTitle>
                        <DialogDescription>
                            以下为后端确认清单 v{data.session_version ?? "—"}
                            ，提交后将生成唯一销售方案。
                        </DialogDescription>
                    </DialogHeader>
                    <div className="flex flex-col gap-2">
                        {data.choices.map((item) => (
                            <div
                                key={item.item_id}
                                className="flex items-center justify-between gap-3 border-b pb-2 text-sm last:border-0"
                            >
                                <span className="min-w-0 flex-1 truncate">
                                    {itemById.get(item.item_id)?.name ??
                                        item.item_id}
                                    {item.quantity !== null &&
                                    item.quantity !== undefined
                                        ? ` × ${item.quantity} 份`
                                        : ""}
                                </span>
                                <MoneyValue
                                    value={
                                        itemById.get(item.item_id)?.price ??
                                        "0.00"
                                    }
                                    taxBasis="gross"
                                />
                            </div>
                        ))}
                        {byQuantity && data.total_amount ? (
                            <div className="flex items-center justify-between pt-1">
                                <span className="text-sm text-muted-foreground">
                                    合计（含税）
                                </span>
                                <MoneyValue
                                    value={data.total_amount}
                                    taxBasis="gross"
                                />
                            </div>
                        ) : null}
                    </div>
                    <DialogFooter>
                        <Button
                            id="public-selection-confirm-cancel"
                            type="button"
                            variant="outline"
                            onClick={() => setConfirmOpen(false)}
                        >
                            再想想
                        </Button>
                        <Button
                            id="public-selection-confirm-submit"
                            type="button"
                            disabled={submitMutation.isPending}
                            onClick={() => void handleSubmit()}
                        >
                            {submitMutation.isPending
                                ? "提交中…"
                                : "确认并提交选品"}
                        </Button>
                    </DialogFooter>
                </DialogContent>
            </Dialog>
        </div>
    )
}

/**
 * 已提交回执容器：有效链接内只读查看本次提交。
 */
const SubmittedReceipt = ({ token }: { token: string }) => {
    const receiptQuery = usePublicReceipt(token, true)

    if (receiptQuery.isPending) {
        return (
            <div className="mx-auto flex w-full max-w-md flex-col gap-3 p-4">
                <Skeleton className="h-8 w-40" />
                <Skeleton className="h-64 w-full" />
            </div>
        )
    }

    if (receiptQuery.isError || !receiptQuery.data?.receipt) {
        return (
            <EndedView
                title="选品已提交"
                hint="本次选择已提交，详细回执暂时无法展示，请联系销售核对。"
            />
        )
    }

    return <ReceiptView page={receiptQuery.data} />
}
