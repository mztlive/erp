"use client"

import * as React from "react"
import Link from "next/link"
import { usePathname, useRouter, useSearchParams } from "next/navigation"
import { ArrowLeftIcon, ExternalLinkIcon, RefreshCwIcon } from "lucide-react"

import {
    BusinessEmptyState,
    BusinessFailureState,
    BusinessStatusBadge,
    CostCoverageNotice,
    DocumentHeader,
    DocumentSection,
    DocumentSummary,
    MoneyValue,
    PageHeader,
    PageScaffold,
    RelatedDocumentList,
    StatusTrackSummary,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { Separator } from "@/components/ui/separator"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useConsumptionOrderDetailQuery } from "@/features/mall-consumption-orders/queries"
import type {
    MallConsumptionOrderView,
    MallOrderFactView,
    ObjectCenterSectionId,
    PaymentSourceView,
} from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    ATTRIBUTION_STATUS_TONE,
    COST_BASIS_LABEL,
    COST_BASIS_TONE,
    FACT_TYPE_LABEL,
    FACT_TYPE_TONE,
    FULFILLMENT_CHAIN_LABEL,
    FULFILLMENT_CHAIN_TONE,
    OBJECT_CENTER_SECTIONS,
    PROCESSING_STATUS_LABEL,
    SUPPLIER_CANCEL_LABEL,
    SUPPLIER_REFUND_LABEL,
    SUPPLIER_STATUS_LABEL,
} from "@/features/mall-consumption-orders/types"
import { customerLabelFor } from "@/features/mall-consumption-orders/customer-title"
import { cn } from "@/lib/utils"
import { openWorkspaceLabel } from "@/lib/ui-text"
import { formatDateTime } from "@/lib/datetime"

/** 动作阻断的中文动作名（审计 tab；命令码不允许上屏） */
const BLOCKED_ACTION_LABEL: Record<string, string> = {
    EDIT_MALL_ORDER: "修改商城订单",
    RETRY_SUPPLIER: "重试供应商下单",
}

function parseSection(raw: string | null): ObjectCenterSectionId {
    const found = OBJECT_CENTER_SECTIONS.find((s) => s.id === raw)
    return found?.id ?? "overview"
}

function sourceColumnTitle(source: PaymentSourceView) {
    if (source.sourceType === "CARD") {
        return (
            <span>
                卡券 {source.sourceReference}
                <Badge variant="outline" className="ml-1">
                    非卡号
                </Badge>
            </span>
        )
    }
    return <span>微信 {source.sourceReference}</span>
}

function allocationAmount(
    view: MallConsumptionOrderView,
    itemId: string,
    sourceId: string,
): string {
    const hit = view.fundingAllocations.find(
        (a) => a.mallOrderItemId === itemId && a.paymentSourceId === sourceId,
    )
    return hit?.allocatedPaymentAmount ?? "0.00"
}

function PaymentMatrix({ view }: { view: MallConsumptionOrderView }) {
    const sources = view.paymentSources
    const items = view.items
    const anyInvalid =
        !view.conservation.orderTotal.valid ||
        view.conservation.itemRowResults.some((r) => !r.valid) ||
        view.conservation.sourceColumnResults.some((r) => !r.valid)

    return (
        <div className="space-y-3 overflow-x-auto">
            {anyInvalid ? (
                <Alert variant="destructive" role="alert">
                    <AlertTitle>分摊不守恒</AlertTitle>
                    <AlertDescription>
                        系统校验与页面存在差异时高亮无效单元格；页面不推算优惠、运费或分摊。
                    </AlertDescription>
                </Alert>
            ) : (
                <Alert variant="success">
                    <AlertTitle>行列守恒有效</AlertTitle>
                    <AlertDescription>
                        行合计、列合计与订单实付均由系统给出：
                        <span className="num mx-1">
                            {view.conservation.orderTotal.actual}
                        </span>
                        （含税实付）。
                    </AlertDescription>
                </Alert>
            )}

            <table className="w-full min-w-[40rem] border-collapse text-sm">
                <caption className="sr-only">
                    商品 × 支付来源分摊矩阵（仅卡券 / 微信）
                </caption>
                <thead>
                    <tr className="border-b border-border/30 text-left">
                        <th className="sticky left-0 bg-card p-2 font-medium">
                            商品明细
                        </th>
                        {sources.map((s) => (
                            <th
                                key={s.paymentSourceId}
                                className="p-2 font-medium"
                            >
                                {sourceColumnTitle(s)}
                            </th>
                        ))}
                        <th className="p-2 text-right font-medium">明细实付</th>
                    </tr>
                </thead>
                <tbody>
                    {items.map((item) => {
                        const rowResult = view.conservation.itemRowResults.find(
                            (r) => r.mallOrderItemId === item.mallOrderItemId,
                        )
                        return (
                            <tr
                                key={item.mallOrderItemId}
                                className="border-b border-border/70"
                            >
                                <th
                                    scope="row"
                                    className="sticky left-0 bg-card p-2 text-left font-normal"
                                >
                                    <div className="font-medium">
                                        {item.nameSnapshot}
                                    </div>
                                    <div className="text-xs text-muted-foreground">
                                        {item.specSnapshot}
                                        <span className="mx-1">·</span>
                                        <span className="num">
                                            {item.externalItemId}
                                        </span>
                                    </div>
                                </th>
                                {sources.map((s) => {
                                    const amount = allocationAmount(
                                        view,
                                        item.mallOrderItemId,
                                        s.paymentSourceId,
                                    )
                                    return (
                                        <td
                                            key={s.paymentSourceId}
                                            className="p-2"
                                        >
                                            <MoneyValue value={amount} />
                                        </td>
                                    )
                                })}
                                <td
                                    className={cn(
                                        "p-2 text-right",
                                        rowResult &&
                                            !rowResult.valid &&
                                            "bg-destructive/10",
                                    )}
                                >
                                    <MoneyValue
                                        value={item.paidAmount}
                                        taxBasis="gross"
                                    />
                                    {rowResult && !rowResult.valid ? (
                                        <div className="text-xs text-destructive">
                                            期望 {rowResult.expected} / 实际{" "}
                                            {rowResult.actual}
                                        </div>
                                    ) : null}
                                </td>
                            </tr>
                        )
                    })}
                </tbody>
                <tfoot>
                    <tr className="border-t border-border/30 font-medium">
                        <th
                            scope="row"
                            className="sticky left-0 bg-card p-2 text-left"
                        >
                            来源合计
                        </th>
                        {sources.map((s) => {
                            const col =
                                view.conservation.sourceColumnResults.find(
                                    (r) =>
                                        r.paymentSourceId === s.paymentSourceId,
                                )
                            return (
                                <td
                                    key={s.paymentSourceId}
                                    className={cn(
                                        "p-2",
                                        col &&
                                            !col.valid &&
                                            "bg-destructive/10",
                                    )}
                                >
                                    <MoneyValue value={s.amount} />
                                    {col && !col.valid ? (
                                        <div className="text-xs text-destructive">
                                            期望 {col.expected}
                                        </div>
                                    ) : null}
                                </td>
                            )
                        })}
                        <td className="p-2 text-right">
                            <MoneyValue
                                value={view.conservation.orderTotal.actual}
                                taxBasis="gross"
                            />
                        </td>
                    </tr>
                </tfoot>
            </table>
            <p className="text-xs text-muted-foreground">
                支付来源仅卡券与微信；不存在福利账户分支。成本不进入本矩阵。
            </p>
        </div>
    )
}

function FactCard({
    fact,
    selected,
    onSelect,
}: {
    fact: MallOrderFactView
    selected: boolean
    onSelect: () => void
}) {
    return (
        <button
            type="button"
            onClick={onSelect}
            className={cn(
                "w-full rounded-lg p-3 text-left transition-colors ring-1",
                selected
                    ? "bg-primary/5 ring-primary/40"
                    : "bg-card ring-foreground/[0.04] hover:bg-muted/40",
            )}
            aria-current={selected ? "true" : undefined}
        >
            <div className="flex flex-wrap items-center gap-2">
                <BusinessStatusBadge
                    context="detail"
                    label={FACT_TYPE_LABEL[fact.factType]}
                    tone={FACT_TYPE_TONE[fact.factType]}
                />
                <Badge variant="outline">
                    {fact.dataSource === "BACKFILL" ? "回填" : "实时"}
                </Badge>
                <span className="num text-xs text-muted-foreground">
                    {fact.businessFactKeySummary}
                </span>
            </div>
            <dl className="mt-2 grid gap-1 text-xs sm:grid-cols-2">
                <div>
                    <dt className="text-muted-foreground">发生时间</dt>
                    <dd className="num">
                        {formatDateTime(fact.occurredAt, "default")}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">接收时间</dt>
                    <dd className="num">
                        {formatDateTime(fact.receivedAt, "default")}
                    </dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">商城版本</dt>
                    <dd className="num">{fact.externalOrderVersion}</dd>
                </div>
                <div>
                    <dt className="text-muted-foreground">处理状态</dt>
                    <dd>{PROCESSING_STATUS_LABEL[fact.processingStatus]}</dd>
                </div>
                {fact.afterSalesRequestId ? (
                    <div>
                        <dt className="text-muted-foreground">售后请求</dt>
                        <dd className="num">{fact.afterSalesRequestId}</dd>
                    </div>
                ) : null}
            </dl>
            {Object.keys(fact.resultDetails).length > 0 ? (
                <ul className="mt-2 space-y-0.5 text-xs text-muted-foreground">
                    {Object.entries(fact.resultDetails).map(([k, v]) => (
                        <li key={k}>
                            {k}:{" "}
                            <span className="text-foreground">
                                {String(v ?? "—")}
                            </span>
                        </li>
                    ))}
                </ul>
            ) : null}
        </button>
    )
}

export function ConsumptionOrderCenterPage({
    mallOrderId,
}: {
    mallOrderId: string
}) {
    const router = useRouter()
    const pathname = usePathname()
    const searchParams = useSearchParams()
    const section = parseSection(searchParams.get("section"))
    const factId = searchParams.get("fact") ?? undefined
    /** 从列表页进入时携带的返回地址（保留筛选/分页上下文） */
    const backToListHref =
        searchParams.get("returnTo") ?? "/commerce/consumption-orders"

    const detailQuery = useConsumptionOrderDetailQuery(mallOrderId)
    const view = detailQuery.data

    const setSection = React.useCallback(
        (next: ObjectCenterSectionId, fact?: string) => {
            const sp = new URLSearchParams(searchParams.toString())
            sp.set("section", next)
            if (fact) sp.set("fact", fact)
            else if (next !== "facts") sp.delete("fact")
            const qs = sp.toString()
            router.replace(qs ? `${pathname}?${qs}` : pathname)
        },
        [pathname, router, searchParams],
    )

    const selectedFactId =
        factId ??
        view?.facts.find((f) => f.factType === "PAYMENT_SUCCEEDED")?.factId ??
        view?.facts[0]?.factId

    if (detailQuery.isPending) {
        return (
            <PageScaffold>
                <div className="space-y-3" aria-busy="true" aria-label="加载中">
                    <div className="h-10 w-56 animate-pulse rounded-lg bg-muted" />
                    <div className="h-24 animate-pulse rounded-lg bg-muted" />
                    <div className="h-96 animate-pulse rounded-lg bg-muted" />
                </div>
            </PageScaffold>
        )
    }

    if (detailQuery.isError) {
        return (
            <PageScaffold>
                <BusinessFailureState
                    title="加载失败"
                    error={detailQuery.error}
                    action={
                        <div className="flex flex-wrap gap-2">
                            <Button
                                type="button"
                                variant="secondary"
                                className="rounded-lg shadow-none"
                                onClick={() => void detailQuery.refetch()}
                            >
                                重试
                            </Button>
                            <Button
                                type="button"
                                variant="outline"
                                render={<Link href={backToListHref} />}
                            >
                                返回列表
                            </Button>
                        </div>
                    }
                />
            </PageScaffold>
        )
    }

    if (!view) {
        return (
            <PageScaffold>
                <BusinessEmptyState
                    kind="no-data"
                    title="未找到消费订单"
                    description="该消费订单不存在或当前账号无权访问。"
                    action={
                        <Button
                            type="button"
                            variant="secondary"
                            className="rounded-lg shadow-none"
                            render={
                                <Link href="/commerce/consumption-orders" />
                            }
                        >
                            返回列表
                        </Button>
                    }
                />
            </PageScaffold>
        )
    }

    const noneEntries = view.consumptionEntries.filter(
        (e) => e.currentCostAssessment.costBasis === "NONE",
    )
    const costBasisPrimary =
        noneEntries.length === view.consumptionEntries.length &&
        view.consumptionEntries.length > 0
            ? "NONE"
            : view.consumptionEntries.some(
                    (e) => e.currentCostAssessment.costBasis === "ACTUAL",
                )
              ? "ACTUAL"
              : view.consumptionEntries.some(
                      (e) => e.currentCostAssessment.costBasis === "STANDARD",
                  )
                ? "STANDARD"
                : "NONE"

    /** 成本覆盖率按消费条目逐条统计，不做占位伪造 */
    const costEntries = view.consumptionEntries
    const costCoverage = (() => {
        const total = costEntries.length
        const actualCount = costEntries.filter(
            (e) => e.currentCostAssessment.costBasis === "ACTUAL",
        ).length
        const standardCount = costEntries.filter(
            (e) => e.currentCostAssessment.costBasis === "STANDARD",
        ).length
        const coveredCount = actualCount + standardCount
        return {
            total,
            coveredCount,
            percent: total === 0 ? 0 : Math.round((coveredCount / total) * 100),
            state:
                total === 0 || coveredCount === 0
                    ? ("none" as const)
                    : coveredCount === total
                      ? ("complete" as const)
                      : ("partial" as const),
        }
    })()

    const sortedFacts = [...view.facts].sort(
        (a, b) =>
            new Date(a.occurredAt).getTime() - new Date(b.occurredAt).getTime(),
    )

    return (
        <PageScaffold>
            <PageHeader
                variant="object-chrome"
                breadcrumbs={[
                    {
                        id: "com",
                        label: "商城消费订单",
                        href: "/commerce/consumption-orders",
                    },
                    {
                        id: "detail",
                        label: view.identity.externalOrderNo,
                        current: true,
                    },
                ]}
                actions={
                    <div className="flex flex-wrap gap-2">
                        <Button
                            type="button"
                            variant="outline"
                            size="sm"
                            render={<Link href={backToListHref} />}
                        >
                            <ArrowLeftIcon data-icon="inline-start" />
                            返回列表
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            onClick={() => {
                                void navigator.clipboard.writeText(
                                    view.identity.externalOrderNo,
                                )
                            }}
                            title="复制商城订单号到剪贴板"
                        >
                            复制单号
                        </Button>
                        <Button
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={detailQuery.isFetching}
                            onClick={() => void detailQuery.refetch()}
                        >
                            <RefreshCwIcon
                                data-icon="inline-start"
                                className={
                                    detailQuery.isFetching
                                        ? "animate-spin"
                                        : undefined
                                }
                            />
                            刷新
                        </Button>
                    </div>
                }
            />

            <DocumentHeader
                density="compact"
                title={`${view.identity.mallName} · ${customerLabelFor(view)}`}
                documentNumber={view.identity.externalOrderNo}
                primaryStatus={{
                    label: FULFILLMENT_CHAIN_LABEL[view.fulfillment.chain],
                    tone: FULFILLMENT_CHAIN_TONE[view.fulfillment.chain],
                }}
                meta={
                    <span className="text-muted-foreground">
                        记录更新{" "}
                        {formatDateTime(
                            view.freshness.factWatermark,
                            "default",
                        )}
                    </span>
                }
            />
            <StatusTrackSummary
                tracks={[
                    {
                        id: "fact",
                        label: "关键记录",
                        status: {
                            label: `${view.facts.length} 条`,
                            tone: "info",
                        },
                    },
                    {
                        id: "fulfillment",
                        label: "履约链",
                        status: {
                            label: FULFILLMENT_CHAIN_LABEL[
                                view.fulfillment.chain
                            ],
                            tone: FULFILLMENT_CHAIN_TONE[
                                view.fulfillment.chain
                            ],
                        },
                    },
                    {
                        id: "attr",
                        label: "归集",
                        status: {
                            label: ATTRIBUTION_STATUS_LABEL[
                                view.customer.attributionStatus
                            ],
                            tone: ATTRIBUTION_STATUS_TONE[
                                view.customer.attributionStatus
                            ],
                        },
                    },
                ]}
            />

            {view.paymentOccurredAlert ? (
                <Alert
                    variant={
                        view.paymentOccurredAlert.severity === "destructive"
                            ? "destructive"
                            : "warning"
                    }
                    role="alert"
                >
                    <AlertTitle>{view.paymentOccurredAlert.title}</AlertTitle>
                    <AlertDescription>
                        {view.paymentOccurredAlert.message}
                        <div className="mt-2 flex flex-wrap gap-2">
                            {view.supplierOrders[0] ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    render={
                                        <Link
                                            href={`/supplier-api/orders?supplierOrderId=${view.supplierOrders[0].supplierFulfillmentOrderId}&from=W25&mallOrderId=${view.identity.mallOrderId}`}
                                        />
                                    }
                                >
                                    打开供应商子订单
                                    <ExternalLinkIcon data-icon="inline-end" />
                                </Button>
                            ) : null}
                            {view.workItemIds[0] ? (
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="outline"
                                    render={
                                        <Link
                                            href={`/governance/integration-errors?resolveWorkItemId=${view.workItemIds[0]}&queueContextId=queue:W29:mine:all`}
                                        />
                                    }
                                >
                                    打开接口错误差异
                                    <ExternalLinkIcon data-icon="inline-end" />
                                </Button>
                            ) : null}
                        </div>
                    </AlertDescription>
                </Alert>
            ) : null}

            <Alert variant="info">
                <AlertTitle>记录追溯边界</AlertTitle>
                <AlertDescription>
                    {view.boundaryNotice}
                    <span className="mt-1 block text-xs text-muted-foreground">
                        不提供修改商城订单、补支付记录、编辑分摊或旁路重试供应商动作。
                    </span>
                </AlertDescription>
            </Alert>

            <div
                className={cn(surfacePanelClassName, "min-w-0 overflow-hidden")}
            >
                <Tabs
                    value={section}
                    onValueChange={(v) =>
                        setSection(v as ObjectCenterSectionId)
                    }
                >
                    <TabsList
                        variant="line"
                        className="sticky top-0 z-10 h-auto w-full flex-wrap justify-start gap-1 overflow-x-auto rounded-none border-b border-border/30 bg-card/95 px-3 py-1.5 backdrop-blur supports-backdrop-filter:bg-card/80"
                    >
                        {OBJECT_CENTER_SECTIONS.map((s) => (
                            <TabsTrigger
                                key={s.id}
                                value={s.id}
                                className="flex-none"
                            >
                                {s.label}
                            </TabsTrigger>
                        ))}
                    </TabsList>

                    <TabsContent
                        value="overview"
                        className="space-y-4 px-3 pt-4 pb-4 md:px-4"
                    >
                        <div className="space-y-4">
                            <DocumentSection title="金额与身份">
                                <DocumentSummary
                                    columns="three"
                                    items={[
                                        {
                                            id: "f-57558",
                                            label: "商城订单",
                                            value: (
                                                <span className="num">
                                                    {
                                                        view.identity
                                                            .externalOrderNo
                                                    }
                                                </span>
                                            ),
                                        },
                                        {
                                            id: "f-17653",
                                            label: "ERP 订单编号",
                                            value: (
                                                <span className="num">
                                                    {view.identity.mallOrderId}
                                                </span>
                                            ),
                                        },
                                        {
                                            id: "f-51562",
                                            label: "来源商城",
                                            value: view.identity.mallName,
                                        },
                                        {
                                            id: "f-63424",
                                            label: "客户",
                                            value:
                                                view.fieldPermissions
                                                    .customer === "masked"
                                                    ? "****（打码）"
                                                    : view.customer
                                                          .customerLabel,
                                        },
                                        {
                                            id: "f-28981",
                                            label: "下单时间",
                                            value: (
                                                <span className="num">
                                                    {formatDateTime(
                                                        view.orderedAt,
                                                        "default",
                                                    )}
                                                </span>
                                            ),
                                        },
                                        {
                                            id: "f-38567",
                                            label: "支付时间（决定履约链）",
                                            value: (
                                                <span className="num">
                                                    {formatDateTime(
                                                        view.paidAt,
                                                        "default",
                                                    )}
                                                </span>
                                            ),
                                        },
                                        {
                                            id: "f-15545",
                                            label: "商品原价",
                                            value: (
                                                <MoneyValue
                                                    value={view.amounts.gross}
                                                    taxBasis="gross"
                                                />
                                            ),
                                        },
                                        {
                                            id: "f-82950",
                                            label: "优惠",
                                            value: (
                                                <MoneyValue
                                                    value={
                                                        view.amounts.discount
                                                    }
                                                />
                                            ),
                                        },
                                        {
                                            id: "f-38831",
                                            label: "运费",
                                            value: (
                                                <MoneyValue
                                                    value={view.amounts.freight}
                                                />
                                            ),
                                        },
                                        {
                                            id: "f-21324",
                                            label: "实付",
                                            value: (
                                                <span className="text-lg font-semibold">
                                                    <MoneyValue
                                                        value={
                                                            view.amounts.paid
                                                        }
                                                        taxBasis="gross"
                                                    />
                                                </span>
                                            ),
                                        },
                                        {
                                            id: "f-95351",
                                            label: "守恒",
                                            value:
                                                view.amounts
                                                    .conservationStatus ===
                                                "VALID" ? (
                                                    "有效"
                                                ) : (
                                                    <span className="text-destructive">
                                                        差异
                                                    </span>
                                                ),
                                        },
                                        {
                                            id: "f-8625",
                                            label: "履约判定",
                                            value: (
                                                <span className="text-sm">
                                                    {
                                                        FULFILLMENT_CHAIN_LABEL[
                                                            view.fulfillment
                                                                .chain
                                                        ]
                                                    }
                                                    <span className="mx-1 text-muted-foreground">
                                                        ·
                                                    </span>
                                                    支付成功时间{" "}
                                                    {formatDateTime(
                                                        view.fulfillment
                                                            .decidedByOccurredAt,
                                                        "default",
                                                    )}
                                                    {view.fulfillment.chain ===
                                                    "LEGACY_MANUAL"
                                                        ? "，早于切换时点"
                                                        : "，不早于切换时点"}
                                                </span>
                                            ),
                                        },
                                    ]}
                                />
                                {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                                    <Alert variant="default" className="mt-3">
                                        <AlertTitle>原人工履约链</AlertTitle>
                                        <AlertDescription>
                                            该支付发生在履约主责切换之前，仅作历史记录，不创建供应商子订单。
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                                {view.fulfillment.autoFulfillmentBlocker ? (
                                    <Alert variant="warning" className="mt-3">
                                        <AlertTitle>
                                            自动履约条件不足
                                        </AlertTitle>
                                        <AlertDescription>
                                            {
                                                view.fulfillment
                                                    .autoFulfillmentBlocker
                                            }
                                        </AlertDescription>
                                    </Alert>
                                ) : null}
                            </DocumentSection>

                            <DocumentSection title="敏感字段（按权限打码）">
                                <DocumentSummary
                                    columns="three"
                                    items={[
                                        {
                                            id: "f-52328",
                                            label: "收货地址",
                                            value: view.address.maskedSummary,
                                        },
                                        {
                                            id: "f-33695",
                                            label: "手机号",
                                            value: view.phoneMasked,
                                        },
                                        {
                                            id: "f-91754",
                                            label: "支付引用",
                                            value: view.paymentRefMasked,
                                        },
                                    ]}
                                />
                                <p className="mt-2 text-xs text-muted-foreground">
                                    地址、手机号与支付引用按权限打码展示，完整值不在此页显示；卡号与卡密永不展示。
                                </p>
                            </DocumentSection>
                        </div>
                    </TabsContent>

                    <TabsContent
                        value="facts"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection
                            title="五类关键记录时间线"
                            description="以业务发生时间排序，并展示接收时间。多次部分退款与余额恢复逐笔展示，不按订单号合并。"
                        >
                            <div className="grid gap-3 lg:grid-cols-2">
                                {sortedFacts.map((fact) => (
                                    <FactCard
                                        key={fact.factId}
                                        fact={fact}
                                        selected={
                                            fact.factId === selectedFactId
                                        }
                                        onSelect={() =>
                                            setSection("facts", fact.factId)
                                        }
                                    />
                                ))}
                            </div>
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="items"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection title="商品明细（下单时）">
                            <div className="space-y-3">
                                {view.items.map((item) => (
                                    <Card
                                        key={item.mallOrderItemId}
                                        className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                                    >
                                        <CardHeader className="border-b border-border/30 pb-2">
                                            <CardTitle className="text-base">
                                                {item.nameSnapshot}
                                            </CardTitle>
                                            <CardDescription>
                                                {item.specSnapshot}
                                                <span className="mx-1">·</span>
                                                <span className="num">
                                                    {item.externalItemId}
                                                </span>
                                                {item.skuId ? (
                                                    <>
                                                        <span className="mx-1">
                                                            ·
                                                        </span>
                                                        <span className="num">
                                                            SKU {item.skuId}
                                                        </span>
                                                    </>
                                                ) : (
                                                    <Badge
                                                        variant="warning"
                                                        className="ml-2"
                                                    >
                                                        待映射
                                                    </Badge>
                                                )}
                                            </CardDescription>
                                        </CardHeader>
                                        <CardContent>
                                            <DocumentSummary
                                                columns="four"
                                                items={[
                                                    {
                                                        id: "f-93130",
                                                        label: "数量",
                                                        value: (
                                                            <span className="num">
                                                                {item.quantity}
                                                            </span>
                                                        ),
                                                    },
                                                    {
                                                        id: "f-49274",
                                                        label: "含税单价",
                                                        value: (
                                                            <MoneyValue
                                                                value={
                                                                    item.unitPriceGross
                                                                }
                                                                taxBasis="gross"
                                                            />
                                                        ),
                                                    },
                                                    {
                                                        id: "f-72923",
                                                        label: "明细原价",
                                                        value: (
                                                            <MoneyValue
                                                                value={
                                                                    item.lineGrossAmount
                                                                }
                                                                taxBasis="gross"
                                                            />
                                                        ),
                                                    },
                                                    {
                                                        id: "f-28117",
                                                        label: "明细实付",
                                                        value: (
                                                            <MoneyValue
                                                                value={
                                                                    item.paidAmount
                                                                }
                                                                taxBasis="gross"
                                                            />
                                                        ),
                                                    },
                                                    {
                                                        id: "f-49028",
                                                        label: "分摊优惠",
                                                        value: (
                                                            <MoneyValue
                                                                value={
                                                                    item.allocatedDiscountAmount
                                                                }
                                                            />
                                                        ),
                                                    },
                                                    {
                                                        id: "f-58253",
                                                        label: "分摊运费",
                                                        value: (
                                                            <MoneyValue
                                                                value={
                                                                    item.allocatedFreightAmount
                                                                }
                                                            />
                                                        ),
                                                    },
                                                    {
                                                        id: "f-47772",
                                                        label: "归集",
                                                        value: ATTRIBUTION_STATUS_LABEL[
                                                            item
                                                                .attributionStatus
                                                        ],
                                                    },
                                                    {
                                                        id: "f-25032",
                                                        label: "下单时商城成本",
                                                        value:
                                                            view
                                                                .fieldPermissions
                                                                .cost ===
                                                            "masked" ? (
                                                                "****"
                                                            ) : item.costSnapshotTotal ? (
                                                                <MoneyValue
                                                                    value={
                                                                        item.costSnapshotTotal
                                                                    }
                                                                />
                                                            ) : (
                                                                "—"
                                                            ),
                                                    },
                                                ]}
                                            />
                                        </CardContent>
                                    </Card>
                                ))}
                            </div>
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="payment"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection
                            title="支付与分摊"
                            description="商品与支付来源的守恒校验；合计与状态以系统结果为准。"
                        >
                            <div className="mb-4 flex flex-wrap gap-2">
                                {view.paymentSources.map((s) => (
                                    <Badge
                                        key={s.paymentSourceId}
                                        variant="secondary"
                                    >
                                        {s.sourceType === "CARD"
                                            ? "卡券"
                                            : "微信"}{" "}
                                        {s.sourceReference}
                                        {s.sourceType === "CARD"
                                            ? " · 非卡号"
                                            : ""}{" "}
                                        · ¥{s.amount}
                                    </Badge>
                                ))}
                            </div>
                            <PaymentMatrix view={view} />
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="origin"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection
                            title="来源追溯"
                            description="从卡券引用可追溯到客户、原销售单与对应卡券明细；不展示卡号与卡密。"
                        >
                            <div className="space-y-3">
                                {view.paymentSources.map((s) => (
                                    <Card
                                        key={s.paymentSourceId}
                                        className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                                    >
                                        <CardHeader className="border-b border-border/30 pb-2">
                                            <CardTitle className="text-base">
                                                {s.sourceType === "CARD"
                                                    ? "卡券来源"
                                                    : "微信支付"}
                                                <span className="num ml-2 text-sm font-normal">
                                                    {s.sourceReference}
                                                </span>
                                                {s.sourceType === "CARD" ? (
                                                    <Badge
                                                        variant="outline"
                                                        className="ml-2"
                                                    >
                                                        非卡号
                                                    </Badge>
                                                ) : null}
                                            </CardTitle>
                                            <CardDescription>
                                                金额{" "}
                                                <MoneyValue value={s.amount} />{" "}
                                                · 归集{" "}
                                                {
                                                    ATTRIBUTION_STATUS_LABEL[
                                                        s.attributionStatus
                                                    ]
                                                }
                                            </CardDescription>
                                        </CardHeader>
                                        <CardContent className="space-y-3">
                                            {s.sourceType === "WECHAT" ? (
                                                <Alert variant="info">
                                                    <AlertTitle>
                                                        微信支付不挂企业卡券收入归属
                                                    </AlertTitle>
                                                    <AlertDescription>
                                                        微信支付仅显示支付摘要，不关联卡券明细。
                                                    </AlertDescription>
                                                </Alert>
                                            ) : null}
                                            {s.attributionIssue ? (
                                                <Alert
                                                    variant={
                                                        s.attributionIssue
                                                            .type ===
                                                        "BASELINE_CONFLICT"
                                                            ? "destructive"
                                                            : "warning"
                                                    }
                                                >
                                                    <AlertTitle>
                                                        {s.attributionIssue
                                                            .type ===
                                                        "BASELINE_CONFLICT"
                                                            ? "数据已更新，禁止覆盖"
                                                            : s.attributionIssue
                                                                    .type ===
                                                                "SOURCE_OBJECT_MISSING"
                                                              ? "来源对象缺失 · 待归集"
                                                              : "未归属 · 待归集"}
                                                    </AlertTitle>
                                                    <AlertDescription>
                                                        责任角色：
                                                        {s.attributionIssue
                                                            .ownerRole ===
                                                        "FINANCE"
                                                            ? "财务"
                                                            : "运营"}
                                                        {s.attributionIssue
                                                            .workItemId ? (
                                                            <>
                                                                {" · "}
                                                                <Link
                                                                    className="underline"
                                                                    href={`/governance/integration-errors?resolveWorkItemId=${s.attributionIssue.workItemId}&queueContextId=queue:W29:mine:all`}
                                                                >
                                                                    打开接口错误
                                                                    / 复核任务
                                                                </Link>
                                                            </>
                                                        ) : null}
                                                    </AlertDescription>
                                                </Alert>
                                            ) : null}
                                            {s.origin ? (
                                                <RelatedDocumentList
                                                    documents={[
                                                        {
                                                            id: s.origin
                                                                .customerId,
                                                            documentType:
                                                                "客户",
                                                            documentNumber:
                                                                s.origin
                                                                    .customerLabel,
                                                            status: {
                                                                label: "已归属",
                                                                tone: "success",
                                                            },
                                                            measure: {
                                                                kind: "quantity",
                                                                value: "—",
                                                            },
                                                            owner: "—",
                                                            openAction: (
                                                                <Button
                                                                    type="button"
                                                                    size="xs"
                                                                    variant="outline"
                                                                    render={
                                                                        <Link
                                                                            href={`/sales/customers/${s.origin.customerId}`}
                                                                        />
                                                                    }
                                                                >
                                                                    打开客户
                                                                </Button>
                                                            ),
                                                        },
                                                        {
                                                            id: s.origin
                                                                .salesOrderId,
                                                            documentType:
                                                                "原销售单",
                                                            documentNumber:
                                                                s.origin
                                                                    .salesOrderNo,
                                                            status: {
                                                                label: "可追溯",
                                                                tone: "info",
                                                            },
                                                            measure: {
                                                                kind: "quantity",
                                                                value: s.origin
                                                                    .salesOrderLineId,
                                                                label: "卡券明细",
                                                            },
                                                            owner: "—",
                                                            openAction: (
                                                                <Button
                                                                    type="button"
                                                                    size="xs"
                                                                    variant="outline"
                                                                    render={
                                                                        <Link
                                                                            href={`/sales/orders/${s.origin.salesOrderId}`}
                                                                        />
                                                                    }
                                                                >
                                                                    {openWorkspaceLabel(
                                                                        "W05",
                                                                    )}
                                                                </Button>
                                                            ),
                                                        },
                                                    ]}
                                                />
                                            ) : s.sourceType === "CARD" ? (
                                                <p className="text-sm text-muted-foreground">
                                                    卡券或客户尚未归集，保留原始引用，不补充猜测值。
                                                </p>
                                            ) : null}
                                        </CardContent>
                                    </Card>
                                ))}
                            </div>
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="supplier"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection title="供应商履约">
                            {view.fulfillment.chain === "LEGACY_MANUAL" ? (
                                <Alert variant="default">
                                    <AlertTitle>
                                        原人工履约链 · 无供应商子订单
                                    </AlertTitle>
                                    <AlertDescription>
                                        截止时点前支付只显示原人工履约，历史回填只记账。不创建供应商子订单，也不显示缺单错误。
                                    </AlertDescription>
                                </Alert>
                            ) : view.supplierOrders.length === 0 ? (
                                <Alert variant="warning">
                                    <AlertTitle>未形成供应商子订单</AlertTitle>
                                    <AlertDescription>
                                        {view.fulfillment
                                            .autoFulfillmentBlocker ??
                                            "自动履约条件不足或归集未完成；支付记录已保留，标记为差异。"}
                                        {view.workItemIds[0] ? (
                                            <div className="mt-2">
                                                <Button
                                                    type="button"
                                                    size="xs"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={`/governance/integration-errors?resolveWorkItemId=${view.workItemIds[0]}&queueContextId=queue:W29:mine:all`}
                                                        />
                                                    }
                                                >
                                                    {openWorkspaceLabel("W29")}
                                                </Button>
                                            </div>
                                        ) : null}
                                    </AlertDescription>
                                </Alert>
                            ) : (
                                <div className="space-y-3">
                                    {view.supplierOrders.map((so) => (
                                        <Card
                                            key={so.supplierFulfillmentOrderId}
                                            className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                                        >
                                            <CardHeader className="border-b border-border/30 pb-2">
                                                <CardTitle className="text-base">
                                                    <span className="num">
                                                        {so.fulfillmentOrderNo}
                                                    </span>
                                                    <span className="mx-2 font-normal text-muted-foreground">
                                                        {so.supplierLabel}
                                                    </span>
                                                </CardTitle>
                                                <CardDescription>
                                                    履约{" "}
                                                    {
                                                        SUPPLIER_STATUS_LABEL[
                                                            so.fulfillmentStatus
                                                        ]
                                                    }{" "}
                                                    · 取消{" "}
                                                    {SUPPLIER_CANCEL_LABEL[
                                                        so.cancelStatus
                                                    ] ?? so.cancelStatus}{" "}
                                                    · 退款{" "}
                                                    {SUPPLIER_REFUND_LABEL[
                                                        so.refundStatus
                                                    ] ?? so.refundStatus}
                                                </CardDescription>
                                            </CardHeader>
                                            <CardContent className="space-y-2">
                                                {(so.fulfillmentStatus ===
                                                    "RESULT_UNKNOWN" ||
                                                    so.fulfillmentStatus ===
                                                        "REJECTED" ||
                                                    so.fulfillmentStatus ===
                                                        "EXCEPTION") && (
                                                    <Alert variant="warning">
                                                        <AlertTitle>
                                                            商城支付已发生，正在处理履约异常
                                                        </AlertTitle>
                                                        <AlertDescription>
                                                            本页不支持编辑或重试商城订单，也不直接操作供应商订单；请前往供应商订单按原任务号处理。
                                                        </AlertDescription>
                                                    </Alert>
                                                )}
                                                {so.supplierRefundSummary ? (
                                                    <DocumentSummary
                                                        columns="three"
                                                        items={[
                                                            {
                                                                id: "f-40005",
                                                                label: "供应商退款记录数",
                                                                value: String(
                                                                    so
                                                                        .supplierRefundSummary
                                                                        .refundFactCount,
                                                                ),
                                                            },
                                                            {
                                                                id: "f-37855",
                                                                label: "成本冲减",
                                                                value: (
                                                                    <MoneyValue
                                                                        value={
                                                                            so
                                                                                .supplierRefundSummary
                                                                                .costReductionGross
                                                                        }
                                                                    />
                                                                ),
                                                            },
                                                            {
                                                                id: "f-5004",
                                                                label: "应付冲减",
                                                                value: (
                                                                    <MoneyValue
                                                                        value={
                                                                            so
                                                                                .supplierRefundSummary
                                                                                .payableReductionGross
                                                                        }
                                                                    />
                                                                ),
                                                            },
                                                            {
                                                                id: "f-69035",
                                                                label: "现金退回",
                                                                value: (
                                                                    <MoneyValue
                                                                        value={
                                                                            so
                                                                                .supplierRefundSummary
                                                                                .cashRefundGross
                                                                        }
                                                                    />
                                                                ),
                                                            },
                                                            {
                                                                id: "f-22899",
                                                                label: "付款分配冲正数",
                                                                value: String(
                                                                    so
                                                                        .supplierRefundSummary
                                                                        .reversedPaymentAllocationCount,
                                                                ),
                                                            },
                                                        ]}
                                                    />
                                                ) : null}
                                                <Button
                                                    type="button"
                                                    size="sm"
                                                    variant="outline"
                                                    render={
                                                        <Link
                                                            href={`/supplier-api/orders?supplierOrderId=${so.supplierFulfillmentOrderId}&from=W25&mallOrderId=${view.identity.mallOrderId}`}
                                                        />
                                                    }
                                                >
                                                    打开供应商订单
                                                    <ExternalLinkIcon data-icon="inline-end" />
                                                </Button>
                                            </CardContent>
                                        </Card>
                                    ))}
                                </div>
                            )}
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="cost"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection
                            title="成本口径"
                            description="无成本时仅显示原因，不按零成本计入利润。"
                        >
                            <CostCoverageNotice
                                basis={costBasisPrimary}
                                coveragePercent={costCoverage.percent}
                                coverageLabel={
                                    costCoverage.total === 0
                                        ? "无消费条目"
                                        : `${costCoverage.coveredCount}/${costCoverage.total} 条已覆盖`
                                }
                                coverageState={costCoverage.state}
                                breakdown={{
                                    ACTUAL: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "ACTUAL").length} 条`,
                                    STANDARD: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "STANDARD").length} 条`,
                                    NONE: `${costEntries.filter((e) => e.currentCostAssessment.costBasis === "NONE").length} 条`,
                                }}
                                profitBasis={
                                    costCoverage.coveredCount === 0
                                        ? "无成本数据时不计入利润；经营分析见「卡券经营分析」"
                                        : "利润解读须同时阅读成本覆盖"
                                }
                                notice={
                                    costCoverage.state === "none"
                                        ? "无可用成本条目金额为空并显示无成本，不按零成本计入利润。"
                                        : undefined
                                }
                            />

                            <div className="mt-4 space-y-3">
                                {view.consumptionEntries.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        尚无消费条目成本评估（待归集时常见）。支付记录与订单仍保留。
                                    </p>
                                ) : (
                                    view.consumptionEntries.map((entry) => {
                                        const ca = entry.currentCostAssessment
                                        return (
                                            <Card
                                                key={entry.consumptionEntryId}
                                                className="rounded-lg border-0 bg-muted/40 shadow-none ring-0"
                                            >
                                                <CardHeader className="border-b border-border/30 pb-2">
                                                    <CardTitle className="text-base">
                                                        {entry.direction ===
                                                        "REVERSAL"
                                                            ? "冲减"
                                                            : "消费"}{" "}
                                                        <span className="num text-sm font-normal">
                                                            {
                                                                entry.consumptionEntryId
                                                            }
                                                        </span>
                                                    </CardTitle>
                                                    <CardDescription>
                                                        <BusinessStatusBadge
                                                            context="list"
                                                            label={
                                                                COST_BASIS_LABEL[
                                                                    ca.costBasis
                                                                ]
                                                            }
                                                            tone={
                                                                COST_BASIS_TONE[
                                                                    ca.costBasis
                                                                ]
                                                            }
                                                        />
                                                        <span className="ml-2">
                                                            {
                                                                ca.basisSourceLabel
                                                            }
                                                        </span>
                                                    </CardDescription>
                                                </CardHeader>
                                                <CardContent>
                                                    <DocumentSummary
                                                        columns="three"
                                                        items={[
                                                            {
                                                                id: "f-89180",
                                                                label: "消费金额",
                                                                value: (
                                                                    <MoneyValue
                                                                        value={
                                                                            entry.amount
                                                                        }
                                                                    />
                                                                ),
                                                            },
                                                            {
                                                                id: "f-25665",
                                                                label: "成本金额（含税）",
                                                                value:
                                                                    ca.costBasis ===
                                                                        "NONE" ||
                                                                    view
                                                                        .fieldPermissions
                                                                        .cost ===
                                                                        "masked" ? (
                                                                        <MoneyValue
                                                                            value={
                                                                                null
                                                                            }
                                                                            unavailableReason={
                                                                                ca.noneReason ??
                                                                                (view
                                                                                    .fieldPermissions
                                                                                    .cost ===
                                                                                "masked"
                                                                                    ? "字段打码"
                                                                                    : "无可用成本")
                                                                            }
                                                                        />
                                                                    ) : (
                                                                        <MoneyValue
                                                                            value={
                                                                                ca.grossAmount
                                                                            }
                                                                        />
                                                                    ),
                                                            },
                                                            {
                                                                id: "f-38012",
                                                                label: "评估时间",
                                                                value: (
                                                                    <span className="num">
                                                                        {formatDateTime(
                                                                            ca.assessedAt,
                                                                            "default",
                                                                        )}
                                                                    </span>
                                                                ),
                                                            },
                                                        ]}
                                                    />
                                                    {ca.costBasis === "NONE" ? (
                                                        <p className="mt-2 text-sm text-warning-foreground">
                                                            {ca.noneReason ??
                                                                "无可用成本来源，金额为空，不计入利润"}
                                                        </p>
                                                    ) : null}
                                                </CardContent>
                                            </Card>
                                        )
                                    })
                                )}
                            </div>
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="aftersales"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection
                            title="售后结果分轨"
                            description="商城退款仅冲减消费，卡券余额恢复仅记回补，供应商退款另行分列，不替代商城退款。"
                        >
                            <div className="grid gap-3 md:grid-cols-3">
                                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle className="text-base">
                                            商城退款
                                        </CardTitle>
                                        <CardDescription>
                                            冲减消费
                                        </CardDescription>
                                    </CardHeader>
                                    <CardContent className="text-sm">
                                        {
                                            view.facts.filter(
                                                (f) =>
                                                    f.factType ===
                                                    "REFUND_SUCCEEDED",
                                            ).length
                                        }{" "}
                                        笔记录（逐笔展示）
                                    </CardContent>
                                </Card>
                                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle className="text-base">
                                            卡券余额恢复
                                        </CardTitle>
                                        <CardDescription>
                                            只记余额回补
                                        </CardDescription>
                                    </CardHeader>
                                    <CardContent className="text-sm">
                                        {
                                            view.facts.filter(
                                                (f) =>
                                                    f.factType ===
                                                    "CARD_BALANCE_RESTORED",
                                            ).length
                                        }{" "}
                                        笔记录（与退款分轨）
                                    </CardContent>
                                </Card>
                                <Card className="rounded-lg border-0 bg-muted/40 shadow-none ring-0">
                                    <CardHeader className="border-b border-border/30">
                                        <CardTitle className="text-base">
                                            供应商退款
                                        </CardTitle>
                                        <CardDescription>
                                            成本/应付/现金分列
                                        </CardDescription>
                                    </CardHeader>
                                    <CardContent className="text-sm">
                                        {view.supplierOrders.filter(
                                            (s) => s.supplierRefundSummary,
                                        ).length > 0 ? (
                                            <Button
                                                type="button"
                                                size="xs"
                                                variant="outline"
                                                onClick={() =>
                                                    setSection("supplier")
                                                }
                                            >
                                                查看履约区供应商退款摘要
                                            </Button>
                                        ) : (
                                            "无"
                                        )}
                                    </CardContent>
                                </Card>
                            </div>
                            <Separator className="my-4" />
                            <ul className="space-y-2 text-sm">
                                {view.facts
                                    .filter(
                                        (f) =>
                                            f.factType === "REFUND_SUCCEEDED" ||
                                            f.factType ===
                                                "CARD_BALANCE_RESTORED" ||
                                            f.factType === "ORDER_CANCELED",
                                    )
                                    .map((f) => (
                                        <li
                                            key={f.factId}
                                            className="rounded-lg bg-muted/40 p-3"
                                        >
                                            <BusinessStatusBadge
                                                context="list"
                                                label={
                                                    FACT_TYPE_LABEL[f.factType]
                                                }
                                                tone={
                                                    FACT_TYPE_TONE[f.factType]
                                                }
                                            />
                                            <span className="num ml-2 text-xs text-muted-foreground">
                                                {formatDateTime(
                                                    f.occurredAt,
                                                    "default",
                                                )}
                                            </span>
                                            <div className="mt-1 text-muted-foreground">
                                                {Object.entries(f.resultDetails)
                                                    .map(
                                                        ([k, v]) =>
                                                            `${k}=${String(v ?? "—")}`,
                                                    )
                                                    .join(" · ")}
                                            </div>
                                        </li>
                                    ))}
                            </ul>
                        </DocumentSection>
                    </TabsContent>

                    <TabsContent
                        value="audit"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <DocumentSection title="审计与禁止动作">
                            <DocumentSummary
                                columns="two"
                                items={[
                                    {
                                        id: "f-15241",
                                        label: "记录更新时间",
                                        value: (
                                            <span className="num">
                                                {formatDateTime(
                                                    view.freshness
                                                        .factWatermark,
                                                    "default",
                                                )}
                                            </span>
                                        ),
                                    },
                                    {
                                        id: "f-92756",
                                        label: "归集更新",
                                        value: (
                                            <span className="num">
                                                {formatDateTime(
                                                    view.freshness
                                                        .attributionUpdatedAt,
                                                    "default",
                                                )}
                                            </span>
                                        ),
                                    },
                                    {
                                        id: "f-24032",
                                        label: "供应商更新",
                                        value: (
                                            <span className="num">
                                                {formatDateTime(
                                                    view.freshness
                                                        .supplierUpdatedAt,
                                                    "default",
                                                )}
                                            </span>
                                        ),
                                    },
                                    {
                                        id: "f-18033",
                                        label: "成本评估",
                                        value: (
                                            <span className="num">
                                                {formatDateTime(
                                                    view.freshness
                                                        .costAssessedAt,
                                                    "default",
                                                )}
                                            </span>
                                        ),
                                    },
                                ]}
                            />
                            <div className="mt-4 space-y-2">
                                <p className="text-sm font-medium">动作阻断</p>
                                {view.actionBlockers.length === 0 ? (
                                    <p className="text-sm text-muted-foreground">
                                        无额外阻断
                                    </p>
                                ) : (
                                    <ul className="space-y-2">
                                        {view.actionBlockers.map((b) => (
                                            <li
                                                key={`${b.action}-${b.code}`}
                                                className="rounded-lg bg-muted/40 p-3 text-sm"
                                            >
                                                {BLOCKED_ACTION_LABEL[
                                                    b.action
                                                ] ? (
                                                    <span className="font-medium">
                                                        {
                                                            BLOCKED_ACTION_LABEL[
                                                                b.action
                                                            ]
                                                        }
                                                    </span>
                                                ) : null}
                                                <div className="text-muted-foreground">
                                                    {b.message}
                                                </div>
                                            </li>
                                        ))}
                                    </ul>
                                )}
                            </div>
                            <Alert variant="default" className="mt-4">
                                <AlertTitle>
                                    原始记录内容不在本页展示
                                </AlertTitle>
                                <AlertDescription>
                                    如需排障，请前往接口错误中心查看脱敏摘要。
                                    {view.workItemIds[0] ? (
                                        <div className="mt-2">
                                            <Button
                                                type="button"
                                                size="xs"
                                                variant="outline"
                                                render={
                                                    <Link
                                                        href={`/governance/integration-errors?resolveWorkItemId=${view.workItemIds[0]}&queueContextId=queue:W29:mine:all`}
                                                    />
                                                }
                                            >
                                                {openWorkspaceLabel("W29")}
                                            </Button>
                                        </div>
                                    ) : null}
                                </AlertDescription>
                            </Alert>
                        </DocumentSection>
                    </TabsContent>
                </Tabs>
            </div>
        </PageScaffold>
    )
}
