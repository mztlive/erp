"use client"

import Link from "next/link"
import { ArrowLeftIcon, ExternalLinkIcon, RefreshCwIcon } from "lucide-react"

import {
    DocumentHeader,
    PageHeader,
    PageScaffold,
    StatusTrackSummary,
    surfacePanelClassName,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Tabs, TabsContent, TabsList, TabsTrigger } from "@/components/ui/tabs"
import { useConsumptionOrderDetailQuery } from "@/features/mall-consumption-orders/hooks/queries"
import type { ObjectCenterSectionId } from "@/features/mall-consumption-orders/types"
import {
    ATTRIBUTION_STATUS_LABEL,
    ATTRIBUTION_STATUS_TONE,
    FULFILLMENT_CHAIN_LABEL,
    FULFILLMENT_CHAIN_TONE,
    OBJECT_CENTER_SECTIONS,
} from "@/features/mall-consumption-orders/types"
import { customerLabelFor } from "@/features/mall-consumption-orders/lib/customer-title"
import { cn } from "@/lib/utils"
import { formatDateTime } from "@/lib/datetime"
import {
    computeCostBasisPrimary,
    computeCostCoverage,
    resolveSelectedFactId,
    sortFactsByOccurredAt,
} from "./consumption-order-center-derivations"
import { useObjectCenterSection } from "./consumption-order-center-hooks"
import { AftersalesSection } from "./consumption-order-center-aftersales-section"
import { AuditSection } from "./consumption-order-center-audit-section"
import { CostSection } from "./consumption-order-center-cost-section"
import { FactsSection } from "./consumption-order-center-facts"
import { ItemsSection } from "./consumption-order-center-items-section"
import { OriginSection } from "./consumption-order-center-origin-section"
import { OverviewSection } from "./consumption-order-center-overview-section"
import { PaymentSection } from "./consumption-order-center-payment"
import {
    CenterPageEmptyState,
    CenterPageErrorState,
    CenterPagePendingState,
} from "./consumption-order-center-states"
import { SupplierSection } from "./consumption-order-center-supplier-section"

export function ConsumptionOrderCenterPage({
    mallOrderId,
}: {
    mallOrderId: string
}) {
    const { section, factId, backToListHref, setSection } =
        useObjectCenterSection()

    const detailQuery = useConsumptionOrderDetailQuery(mallOrderId)
    const view = detailQuery.data

    if (detailQuery.isPending) {
        return <CenterPagePendingState />
    }

    if (detailQuery.isError) {
        return (
            <CenterPageErrorState
                error={detailQuery.error}
                backToListHref={backToListHref}
                onRetry={() => void detailQuery.refetch()}
            />
        )
    }

    if (!view) {
        return <CenterPageEmptyState />
    }

    const costBasisPrimary = computeCostBasisPrimary(view.consumptionEntries)
    const costCoverage = computeCostCoverage(view.consumptionEntries)
    const sortedFacts = sortFactsByOccurredAt(view.facts)
    const selectedFactId = resolveSelectedFactId(factId, view.facts)

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
                        <OverviewSection view={view} />
                    </TabsContent>

                    <TabsContent
                        value="facts"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <FactsSection
                            facts={sortedFacts}
                            selectedFactId={selectedFactId}
                            onSelectFact={(factId) =>
                                setSection("facts", factId)
                            }
                        />
                    </TabsContent>

                    <TabsContent
                        value="items"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <ItemsSection view={view} />
                    </TabsContent>

                    <TabsContent
                        value="payment"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <PaymentSection view={view} />
                    </TabsContent>

                    <TabsContent
                        value="origin"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <OriginSection view={view} />
                    </TabsContent>

                    <TabsContent
                        value="supplier"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <SupplierSection view={view} />
                    </TabsContent>

                    <TabsContent
                        value="cost"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <CostSection
                            view={view}
                            costBasisPrimary={costBasisPrimary}
                            costCoverage={costCoverage}
                        />
                    </TabsContent>

                    <TabsContent
                        value="aftersales"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <AftersalesSection
                            view={view}
                            onOpenSupplier={() => setSection("supplier")}
                        />
                    </TabsContent>

                    <TabsContent
                        value="audit"
                        className="px-3 pt-4 pb-4 md:px-4"
                    >
                        <AuditSection view={view} />
                    </TabsContent>
                </Tabs>
            </div>
        </PageScaffold>
    )
}
