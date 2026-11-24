import type { ReactNode } from "react"
import { CircleHelpIcon } from "lucide-react"
import {
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
import { timeLabel } from "@/features/supplier-portal/lib/presentation"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import { portalReviewOverview } from "../lib/review-overview"

export function PortalReviewOverview({
    application,
    fallbackSupplier,
    checkProgress,
}: {
    application: PortalApplication
    fallbackSupplier?: string
    checkProgress?: { completed: number; total: number }
}) {
    const overview = portalReviewOverview(application)
    const { completed, total } = checkProgress?.total
        ? checkProgress
        : {
              completed: overview.qualifiedCount,
              total: overview.qualificationChecks.length,
          }
    const supplier = overview.supplier || fallbackSupplier
    const hasPrimaryValue =
        overview.price !== null || application.kind === "new_product"
    const idPrefix = `supplier-portal-review-kpi-${toAutomationIdSegment(application.id)}`
    const allComplete = total > 0 && completed >= total
    const remaining = Math.max(0, total - completed)
    const deltaTone =
        overview.priceDelta?.direction === "up"
            ? "text-warning-soft-foreground"
            : overview.priceDelta?.direction === "down"
              ? "text-success-soft-foreground"
              : overview.priceDelta?.direction === "flat"
                ? "text-muted-foreground"
                : undefined

    return (
        <>
            <TooltipProvider>
                <dl
                    aria-label="供应商申请审核关键指标"
                    className="grid grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/supplier-portal-review:grid-cols-2 @[840px]/supplier-portal-review:grid-cols-4"
                >
                    <ReviewKpi
                        label={
                            overview.price !== null
                                ? overview.priceLabel
                                : application.kind === "new_product"
                                  ? "提报规格"
                                  : "申请类型"
                        }
                        basis={
                            overview.price !== null
                                ? "含税"
                                : application.kind === "new_product"
                                  ? "提报项数"
                                  : undefined
                        }
                        tooltipId={
                            overview.price !== null ||
                            application.kind === "new_product"
                                ? `${idPrefix}-primary-help`
                                : undefined
                        }
                        tooltipLabel="查看申请核心指标口径"
                        tooltipContent={
                            overview.price !== null
                                ? "展示供应商本次提交的拟生效含税供货单价（代发优先，无代发则展示集采单价）。"
                                : application.kind === "new_product"
                                  ? "统计本次新品申请中供应商提报的规格（SKU）总数。"
                                  : undefined
                        }
                    >
                        {hasPrimaryValue ? (
                            <>
                                <span className="num block max-w-full wrap-anywhere text-value-section font-semibold tracking-tight">
                                    {overview.price !== null ? (
                                        <MoneyValue
                                            value={overview.price}
                                            display="exact"
                                            className="max-w-full [&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
                                            inheritColor
                                        />
                                    ) : (
                                        <>{overview.skuCount} 项</>
                                    )}
                                </span>
                                <p className="text-xs text-muted-foreground">
                                    {overview.kind}
                                </p>
                            </>
                        ) : (
                            <span className="text-value-compact font-semibold">
                                {overview.kind}
                            </span>
                        )}
                    </ReviewKpi>
                    <ReviewKpi
                        label={
                            checkProgress?.total
                                ? "资料与审核核对"
                                : "提报资料完整度"
                        }
                        basis="核对进度"
                        tooltipId={`${idPrefix}-progress-help`}
                        tooltipLabel="查看核对进度统计口径"
                        tooltipContent={
                            checkProgress?.total
                                ? "统计提报基础资料与当前审核表单必填项的已完成数量，全部就绪后可确认通过。"
                                : "统计供应商本次提报的必备字段完整项数。"
                        }
                    >
                        <span className="num text-value-compact font-semibold">
                            {completed}/{total}
                        </span>
                        <div
                            className="mt-1.5 flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
                            aria-hidden="true"
                        >
                            {completed > 0 ? (
                                <span
                                    className={
                                        allComplete
                                            ? "bg-success"
                                            : "bg-primary"
                                    }
                                    style={{ flexGrow: completed }}
                                />
                            ) : null}
                            {remaining > 0 ? (
                                <span
                                    className="bg-transparent"
                                    style={{ flexGrow: remaining }}
                                />
                            ) : null}
                        </div>
                        <p className="mt-0.5 text-tiny text-muted-foreground">
                            {allComplete
                                ? "全部核对项已就绪"
                                : `尚余 ${remaining} 项待核对`}
                        </p>
                    </ReviewKpi>
                    <ReviewKpi
                        label={
                            overview.price !== null
                                ? "价格与生效影响"
                                : "生效影响"
                        }
                        basis={overview.priceDelta ? "对比现价" : "生效口径"}
                        tooltipId={
                            overview.priceDelta
                                ? `${idPrefix}-delta-help`
                                : undefined
                        }
                        tooltipLabel="查看单价变动口径"
                        tooltipContent={
                            overview.priceDelta
                                ? "单价变动 = 本次拟生效含税供货单价 − 当前生效含税供货单价；仅影响确认后的后续供货，不改写历史采购单。"
                                : undefined
                        }
                    >
                        <span
                            className={cn(
                                "num text-body-compact font-semibold",
                                deltaTone,
                            )}
                        >
                            {overview.impact}
                        </span>
                    </ReviewKpi>
                    <ReviewKpi label="提交时间" basis="提报来源">
                        <span className="num text-body-compact font-semibold">
                            {overview.submittedAt != null
                                ? timeLabel(overview.submittedAt)
                                : "—"}
                        </span>
                        {supplier ? (
                            <p className="text-xs text-muted-foreground">
                                {supplier}
                            </p>
                        ) : null}
                    </ReviewKpi>
                </dl>
            </TooltipProvider>
            {overview.title ||
            overview.brand ||
            overview.category ||
            overview.orderingCode ||
            overview.validFrom ||
            overview.validTo ||
            overview.reason ? (
                <WorkspaceFactGrid columns={3}>
                    {overview.title ? (
                        <WorkspaceFactItem
                            label="申请内容"
                            value={overview.title}
                        />
                    ) : null}
                    {overview.brand ? (
                        <WorkspaceFactItem
                            label="品牌"
                            value={overview.brand}
                        />
                    ) : null}
                    {overview.category ? (
                        <WorkspaceFactItem
                            label="建议商品分类"
                            value={overview.category}
                        />
                    ) : null}
                    {overview.orderingCode ? (
                        <WorkspaceFactItem
                            label="供应商订货编码"
                            value={overview.orderingCode}
                            numeric
                            selectable
                        />
                    ) : null}
                    {overview.validFrom ? (
                        <WorkspaceFactItem
                            label="拟生效日期"
                            value={overview.validFrom}
                            numeric
                        />
                    ) : null}
                    {overview.validTo ? (
                        <WorkspaceFactItem
                            label="拟失效日期"
                            value={overview.validTo}
                            numeric
                        />
                    ) : null}
                    {overview.reason ? (
                        <WorkspaceFactItem
                            label="提交说明"
                            value={overview.reason}
                            span="full"
                        />
                    ) : null}
                </WorkspaceFactGrid>
            ) : null}
        </>
    )
}

function ReviewKpi({
    label,
    basis,
    tooltipId,
    tooltipLabel,
    tooltipContent,
    children,
}: {
    label: string
    basis?: string
    tooltipId?: string
    tooltipLabel?: string
    tooltipContent?: ReactNode
    children: ReactNode
}) {
    return (
        <div className="min-w-0">
            <dt className="flex flex-wrap items-center gap-x-1.5 gap-y-0.5 text-xs text-muted-foreground">
                <span>{label}</span>
                {basis ? (
                    <span className="text-tiny text-muted-foreground/80">
                        {basis}
                    </span>
                ) : null}
                {tooltipId && tooltipContent ? (
                    <Tooltip>
                        <TooltipTrigger
                            id={tooltipId}
                            type="button"
                            aria-label={tooltipLabel ?? `${label}说明`}
                            className="inline-flex size-4 shrink-0 cursor-help items-center justify-center rounded-sm text-muted-foreground/80 transition-colors hover:text-foreground focus-visible:ring-2 focus-visible:ring-ring"
                        >
                            <CircleHelpIcon
                                className="size-3"
                                aria-hidden="true"
                            />
                        </TooltipTrigger>
                        <TooltipContent>{tooltipContent}</TooltipContent>
                    </Tooltip>
                ) : null}
            </dt>
            <dd className="mt-1 min-w-0 space-y-0.5 wrap-anywhere text-foreground">
                {children}
            </dd>
        </div>
    )
}
