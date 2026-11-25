import {
    DecisionKpiBar,
    DecisionKpiCell,
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
    type DecisionKpiTone,
} from "@/components/business"
import { timeLabel } from "@/features/supplier-portal/lib/presentation"
import type { PortalApplication } from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
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
    const deltaTone: DecisionKpiTone =
        overview.priceDelta?.direction === "up"
            ? "warning"
            : overview.priceDelta?.direction === "down"
              ? "success"
              : overview.priceDelta?.direction === "flat"
                ? "muted"
                : "default"

    return (
        <>
            <DecisionKpiBar
                columns={4}
                ariaLabel="供应商申请审核关键指标"
                gridClassName="gap-x-5 gap-y-3"
            >
                <DecisionKpiCell
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
                    emphasis={hasPrimaryValue ? "primary" : "secondary"}
                    formulaTooltipId={
                        overview.price !== null ||
                        application.kind === "new_product"
                            ? `${idPrefix}-primary-help`
                            : undefined
                    }
                    formulaTooltipLabel="查看申请核心指标口径"
                    formulaTooltip={
                        overview.price !== null
                            ? "展示供应商本次提交的拟生效含税供货单价（代发优先，无代发则展示集采单价）。"
                            : application.kind === "new_product"
                              ? "统计本次新品申请中供应商提报的规格（SKU）总数。"
                              : undefined
                    }
                    value={
                        overview.price !== null ? (
                            <MoneyValue
                                value={overview.price}
                                display="exact"
                                inheritColor
                            />
                        ) : application.kind === "new_product" ? (
                            <>{overview.skuCount} 项</>
                        ) : (
                            overview.kind
                        )
                    }
                    footnote={hasPrimaryValue ? overview.kind : undefined}
                />
                <DecisionKpiCell
                    label={
                        checkProgress?.total
                            ? "资料与审核核对"
                            : "提报资料完整度"
                    }
                    basis="核对进度"
                    emphasis="secondary"
                    formulaTooltipId={`${idPrefix}-progress-help`}
                    formulaTooltipLabel="查看核对进度统计口径"
                    formulaTooltip={
                        checkProgress?.total
                            ? "统计提报基础资料与当前审核表单必填项的已完成数量，全部就绪后可确认通过。"
                            : "统计供应商本次提报的必备字段完整项数。"
                    }
                    value={`${completed}/${total}`}
                    extra={
                        <div
                            className="flex h-1.5 w-full overflow-hidden rounded-full bg-muted"
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
                    }
                    footnote={
                        allComplete
                            ? "全部核对项已就绪"
                            : `尚余 ${remaining} 项待核对`
                    }
                />
                <DecisionKpiCell
                    label={
                        overview.price !== null ? "价格与生效影响" : "生效影响"
                    }
                    basis={overview.priceDelta ? "对比现价" : "生效口径"}
                    emphasis="terms"
                    tone={deltaTone}
                    formulaTooltipId={
                        overview.priceDelta
                            ? `${idPrefix}-delta-help`
                            : undefined
                    }
                    formulaTooltipLabel="查看单价变动口径"
                    formulaTooltip={
                        overview.priceDelta
                            ? "单价变动 = 本次拟生效含税供货单价 − 当前生效含税供货单价；仅影响确认后的后续供货，不改写历史采购单。"
                            : undefined
                    }
                    value={overview.impact}
                />
                <DecisionKpiCell
                    label="提交时间"
                    basis="提报来源"
                    emphasis="terms"
                    value={
                        overview.submittedAt != null
                            ? timeLabel(overview.submittedAt)
                            : "—"
                    }
                    footnote={supplier}
                />
            </DecisionKpiBar>
            {overview.title ||
            overview.brand ||
            overview.category ||
            overview.orderingCode ||
            overview.validFrom ||
            overview.validTo ||
            overview.reason
                ? (() => {
                      const hasValidity = Boolean(
                          overview.validFrom || overview.validTo,
                      )
                      const shortCount =
                          (overview.title ? 1 : 0) +
                          (overview.brand ? 1 : 0) +
                          (overview.category ? 1 : 0) +
                          (overview.orderingCode ? 1 : 0) +
                          (hasValidity ? 1 : 0)
                      const overviewColumns: 2 | 3 | 4 =
                          shortCount <= 2 ? 2 : shortCount === 4 ? 4 : 3
                      return (
                          <div className="border-b border-border/60 pb-3.5">
                              <WorkspaceFactGrid columns={overviewColumns}>
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
                                  {hasValidity ? (
                                      <WorkspaceFactItem
                                          label="拟定有效期"
                                          value={`${overview.validFrom ?? "—"} 至 ${overview.validTo ?? "长期"}`}
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
                          </div>
                      )
                  })()
                : null}
        </>
    )
}
