import type { ApprovalMaterials } from "@/features/approval-workflow/api/materials"
import {
    inspectFinancialApprovalRisks,
    isFinancialApprovalDocumentType,
} from "@/features/approval-workflow/components/financial-approval-renderer"
import {
    inspectStructuredDiffRisks,
    isStructuredDiffDocumentType,
} from "@/features/approval-workflow/components/structured-diff-approval"
import { countUniqueDeliveryPoints } from "@/features/approval-workflow/purchase-fulfillment"
import { toAutomationIdSegment } from "@/lib/automation-id"
import {
    compareDecimal,
    formatCurrencyFixed,
    subtractFixed,
} from "@/lib/fixed-decimal"
import type { InvoiceApprovalReview } from "../api/invoice-review"
import {
    invoiceApprovalQuota,
    invoiceApprovalRisks,
    invoiceApprovalSubmission,
} from "../components/workspace-invoice-approval-model"
import {
    buildPurchaseApprovalMetrics,
    evaluatePurchaseLines,
} from "../components/workspace-purchase-comparison-model"
import { inspectReceiptRisks } from "../components/workspace-receipt-approval"
import type {
    WorkspaceReworkCommandCardProps,
    WorkspaceReworkTriageItem,
} from "../components/workspace-rework-command-card"
import type { WorkspaceWorkItem } from "../types"
import {
    assessSalesApprovalRisk,
    buildSalesApprovalMetrics,
    evaluateSalesApprovalLines,
    missingSalesApprovalEvidence,
} from "./sales-approval-model"

export type WorkspaceReworkTriageConfig = Omit<
    WorkspaceReworkCommandCardProps,
    "idPrefix" | "rejection"
>

export type BuildReworkTriageInput = Readonly<{
    item: WorkspaceWorkItem
    reworkTask?: WorkspaceWorkItem
    submission?: ApprovalMaterials
    invoiceReview?: InvoiceApprovalReview
    invoiceReviewLoading?: boolean
    invoiceReviewError?: boolean
    attachmentCount: number
    canProcess: boolean
    canAbandon: boolean
    supportsInlineEdit: boolean
    hasDocumentHref: boolean
    canOpenMaterials: boolean
    refreshing?: boolean
    onRequestAbandon?: () => void
    onOpenEdit?: () => void
    onOpenDocument?: () => void
    onOpenMaterials?: () => void
    onRefresh?: () => void
    onLocateInvoiceRisk?: (
        target: "source-warnings" | "quota-evidence" | "materials",
    ) => void
    onFilterPurchaseRisks?: () => void
    onFilterSalesRisks?: () => void
    onLocateReceiptRisk?: (riskId: string) => void
    onLocateFinancialLines?: () => void
    onLocateDiffChanges?: () => void
}>

const INVOICE_STATUS_LABELS: Record<string, string> = {
    draft: "草稿",
    pending: "审批中",
    in_approval: "审批中",
    approved: "已批准待开票",
    completed: "已开票",
    rejected: "已驳回",
    cancelled: "已取消",
}

function formatMoneyText(value: string): string {
    try {
        return `¥${formatCurrencyFixed(value, {
            maxScale: 2,
            minimumFractionDigits: 2,
        })}`
    } catch {
        return `¥${value}`
    }
}

function safeCompareMoney(left: string, right: string): number | null {
    try {
        return compareDecimal(left, right, 2)
    } catch {
        return null
    }
}

/**
 * 为除采购付款申请外的 12 类单据驳回待处理任务生成统一行动指挥卡配置：
 * - 态势 C（blocked）：存在 actionBlockers、无重提动作或开票可开票额度归零；
 * - 态势 B（needs_fix）：存在超额/warnings/负毛利/回款或变更风险等可修异常，附一键定位动作；
 * - 态势 A（ready）：无前置异常，按弹窗就地修改或跳转原单分流清晰提示。
 */
export function buildWorkspaceReworkTriage(
    input: BuildReworkTriageInput,
): WorkspaceReworkTriageConfig {
    const {
        item,
        reworkTask,
        submission,
        invoiceReview,
        invoiceReviewLoading = false,
        invoiceReviewError = false,
        attachmentCount,
        canProcess,
        canAbandon,
        supportsInlineEdit,
        hasDocumentHref,
        canOpenMaterials,
        refreshing = false,
        onRequestAbandon,
        onOpenEdit,
        onOpenDocument,
        onOpenMaterials,
        onRefresh,
        onLocateInvoiceRisk,
        onFilterPurchaseRisks,
        onFilterSalesRisks,
        onLocateReceiptRisk,
        onLocateFinancialLines,
        onLocateDiffChanges,
    } = input

    const idBase = `workspace-rework-command-${toAutomationIdSegment(item.workItemId)}`
    const docType = item.routeContext?.documentType ?? item.businessObjectType
    const blockers = [
        ...new Set(
            (reworkTask ?? item).actionBlockers
                .map(({ message }) => message.trim())
                .filter(Boolean),
        ),
    ]

    const abandonPrimaryAction =
        canAbandon && onRequestAbandon
            ? {
                  id: `${idBase}-abandon`,
                  label: "放弃本次办理",
                  variant: "default" as const,
                  onClick: onRequestAbandon,
              }
            : undefined

    // 1. 通用硬阻塞仲裁（优先级最高）
    if (blockers.length > 0 || (item.status === "OPEN" && !canProcess)) {
        const evidence =
            blockers.length > 0
                ? blockers
                : [
                      "当前任务暂无可执行修改或重提权限，可能业务前置状态已变更或仅保留放弃入口。",
                  ]
        return {
            stance: "blocked",
            headline: "当前无法继续重提：业务前提受阻或暂不支持修改",
            description: canAbandon
                ? "请核对下方阻塞原因。若无需继续办理，建议直接点击「放弃本次办理」关闭此待办。"
                : "请先核对下方阻塞原因或联系管理员确认业务状态后再处理。",
            evidence,
            primaryAction: abandonPrimaryAction,
            secondaryActions:
                hasDocumentHref && onOpenDocument
                    ? [
                          {
                              id: `${idBase}-open-doc`,
                              label: "查看原单详情",
                              icon: "external",
                              variant: "outline",
                              onClick: onOpenDocument,
                          },
                      ]
                    : [],
            onRefresh,
            refreshing,
        }
    }

    // 2. 开票申请（sales_invoice_request）：结合实时票款额度与警告体检
    if (docType === "sales_invoice_request") {
        if (invoiceReview) {
            const quota = invoiceApprovalQuota(invoiceReview)
            const { shortage, quotaWarnings, sourceWarnings } =
                invoiceApprovalRisks(invoiceReview)
            const submissionSummary = invoiceApprovalSubmission(
                submission,
                item,
                invoiceReview,
            )
            const beforeSign = safeCompareMoney(quota.beforeRequest, "0")
            const zeroQuota = beforeSign !== null && beforeSign <= 0

            if (zeroQuota) {
                const activeRequests = invoiceReview.requests
                    .filter(
                        (req) =>
                            !req.is_current &&
                            !["cancelled", "rejected"].includes(
                                req.status.toLowerCase(),
                            ),
                    )
                    .slice(0, 3)

                const evidence: string[] = [
                    `本笔应收可开票总额 ${formatMoneyText(invoiceReview.invoiceable_total)} · 已开票净额 ${formatMoneyText(invoiceReview.invoiced_total)}`,
                ]
                if (
                    (safeCompareMoney(
                        invoiceReview.amounts.other_pending,
                        "0",
                    ) ?? 0) > 0
                ) {
                    evidence.push(
                        `其他审批中申请占用 ${formatMoneyText(invoiceReview.amounts.other_pending)}`,
                    )
                }
                if (
                    (safeCompareMoney(
                        invoiceReview.amounts.other_approved,
                        "0",
                    ) ?? 0) > 0
                ) {
                    evidence.push(
                        `其他已批准待开票占用 ${formatMoneyText(invoiceReview.amounts.other_approved)}`,
                    )
                }
                for (const req of activeRequests) {
                    const statusLabel =
                        INVOICE_STATUS_LABELS[req.status.toLowerCase()] ??
                        req.status
                    evidence.push(
                        `占用申请 ${req.request_no}（${formatMoneyText(req.amount)} · ${statusLabel}）`,
                    )
                }

                return {
                    stance: "blocked",
                    headline: "当前无法继续重提：该笔应收可开票额度已为 ¥0.00",
                    description: canAbandon
                        ? "可开票额度已被已开票记录或其他在途申请占满。若无需继续开票，建议核对占用依据后「放弃本次办理」。"
                        : "当前应收已无可用开票额度，请等待在途申请或开票状态更新后刷新重试。",
                    evidence,
                    primaryAction: abandonPrimaryAction,
                    secondaryActions: [
                        ...(onLocateInvoiceRisk
                            ? [
                                  {
                                      id: `${idBase}-locate-quota`,
                                      label: "查看额度与占用明细",
                                      icon: "locate" as const,
                                      variant: "outline" as const,
                                      onClick: () =>
                                          onLocateInvoiceRisk("quota-evidence"),
                                  },
                              ]
                            : []),
                        ...(hasDocumentHref && onOpenDocument
                            ? [
                                  {
                                      id: `${idBase}-open-doc`,
                                      label: "打开原单页面",
                                      icon: "external" as const,
                                      variant: "outline" as const,
                                      onClick: onOpenDocument,
                                  },
                              ]
                            : []),
                    ],
                    onRefresh,
                    refreshing,
                    refreshLabel: "刷新额度",
                }
            }

            let overGap = shortage
            if (!overGap && submissionSummary.amount) {
                try {
                    if (
                        compareDecimal(
                            submissionSummary.amount,
                            quota.beforeRequest,
                            2,
                        ) > 0
                    ) {
                        overGap = subtractFixed(
                            submissionSummary.amount,
                            quota.beforeRequest,
                            { maxScale: 2, outputScale: 2 },
                        )
                    }
                } catch {
                    // 金额解析异常时保留既有 shortage
                }
            }

            const items: WorkspaceReworkTriageItem[] = []
            if (overGap) {
                items.push({
                    id: "invoice-shortage",
                    tone: "destructive",
                    text: `当前申请超出实时可开票额度 ${formatMoneyText(overGap)}（可用 ${formatMoneyText(quota.beforeRequest)}），请调减开票明细金额`,
                    action: onLocateInvoiceRisk
                        ? {
                              id: `${idBase}-locate-quota`,
                              label: "定位超额说明",
                              icon: "locate",
                              onClick: () =>
                                  onLocateInvoiceRisk("quota-evidence"),
                          }
                        : undefined,
                })
            }
            for (const [index, warning] of [
                ...quotaWarnings,
                ...sourceWarnings,
            ].entries()) {
                if (
                    overGap &&
                    warning.startsWith("当前开票占用超出未开票额度")
                ) {
                    continue
                }
                items.push({
                    id: `invoice-warning-${index}`,
                    tone: "warning",
                    text: warning,
                    action: onLocateInvoiceRisk
                        ? {
                              id: `${idBase}-locate-warning-${index}`,
                              label: "查看异常依据",
                              icon: "locate",
                              onClick: () =>
                                  onLocateInvoiceRisk("source-warnings"),
                          }
                        : undefined,
                })
            }

            if (items.length > 0) {
                return {
                    stance: "needs_fix",
                    headline: "提交前需先核对以下票款与开票依据异常",
                    items,
                    description: supportsInlineEdit
                        ? "点击底栏「就地修改并重提」可直接修改明细与金额；或点「打开原单页面」查看完整档案。"
                        : "请先核对异常依据，点击底栏「修改或重新提交 ↗」前往原单修改并重提。",
                    onRefresh,
                    refreshing,
                }
            }

            return {
                stance: "ready",
                headline:
                    "实时额度与票税依据校验通过，修改或确认后即可重新提交",
                description: supportsInlineEdit
                    ? `当前可再申请额度 ${formatMoneyText(quota.beforeRequest)}。点击底栏「就地修改并重提」调整明细或补充说明。`
                    : `当前可再申请额度 ${formatMoneyText(quota.beforeRequest)}。点击底栏「修改或重新提交 ↗」前往原单修改后重提。`,
                onRefresh,
                refreshing,
            }
        }

        if (invoiceReviewError) {
            return {
                stance: "needs_fix",
                headline: "实时开票额度核对暂未完成",
                items: [
                    {
                        id: "invoice-review-error",
                        tone: "warning",
                        text: "实时票款与可开票额度读取失败，建议先刷新核对后再重新提交。",
                    },
                ],
                description: supportsInlineEdit
                    ? "仍可点击底栏「就地修改并重提」打开编辑弹窗查看实时可申请额度。"
                    : "可点击底栏「修改或重新提交 ↗」前往原单页签核对实时额度。",
                onRefresh,
                refreshing,
            }
        }

        return {
            stance: "ready",
            headline: invoiceReviewLoading
                ? "正在同步实时开票额度与回款记录…"
                : "核对驳回意见后可进行修改重提",
            description: supportsInlineEdit
                ? "点击底栏「就地修改并重提」调整开票明细并填写重提说明；或点「打开原单页面」查看完整单据。"
                : "点击底栏「修改或重新提交 ↗」前往原单修改并重新提交。",
            onRefresh,
            refreshing,
        }
    }

    // 3. 采购单（purchase_order）：重型多行只读视图 + 风险项一键定位
    if (docType === "purchase_order") {
        const lines = submission?.purchase_lines ?? []
        const extraSections = submission?.display.source.extra_sections ?? []
        const evaluated = lines.length ? evaluatePurchaseLines(lines) : []
        const metrics = buildPurchaseApprovalMetrics(evaluated, extraSections)
        const uniqueDeliveryCount = countUniqueDeliveryPoints(
            lines.map((line) => line.delivery_allocations),
        )
        const fulfillment = submission?.purchase_fulfillment
        const missingDeliveryAddress =
            uniqueDeliveryCount === 0 &&
            (fulfillment?.responsibility === "SUPPLIER_DIRECT" ||
                fulfillment?.responsibility === "SERVICE")

        const items: WorkspaceReworkTriageItem[] = []
        if (metrics.externalLossCount > 0) {
            items.push({
                id: "po-external-loss",
                tone: "destructive",
                text: `存在 ${metrics.externalLossCount} 项对外负毛利（实际采购单价高于对外含税售价）`,
                action: onFilterPurchaseRisks
                    ? {
                          id: `${idBase}-filter-risks`,
                          label: `仅看风险项（${metrics.actionableRiskLineCount}）`,
                          icon: "locate",
                          onClick: onFilterPurchaseRisks,
                      }
                    : undefined,
            })
        }
        if (metrics.quantityExcessCount > 0) {
            items.push({
                id: "po-qty-excess",
                tone: "destructive",
                text: `存在 ${metrics.quantityExcessCount} 项采购数量超过来源销售需求`,
                action:
                    metrics.externalLossCount === 0 && onFilterPurchaseRisks
                        ? {
                              id: `${idBase}-filter-risks`,
                              label: `仅看风险项（${metrics.actionableRiskLineCount}）`,
                              icon: "locate",
                              onClick: onFilterPurchaseRisks,
                          }
                        : undefined,
            })
        }
        if (metrics.mismatchCount > 0) {
            items.push({
                id: "po-mismatch",
                tone: "destructive",
                text: `存在 ${metrics.mismatchCount} 项采购商品或规格与来源销售不一致`,
                action:
                    metrics.externalLossCount === 0 &&
                    metrics.quantityExcessCount === 0 &&
                    onFilterPurchaseRisks
                        ? {
                              id: `${idBase}-filter-risks`,
                              label: `仅看风险项（${metrics.actionableRiskLineCount}）`,
                              icon: "locate",
                              onClick: onFilterPurchaseRisks,
                          }
                        : undefined,
            })
        }
        if (metrics.costAboveSupplyCount > 0 || metrics.delayCount > 0) {
            const parts = [
                metrics.costAboveSupplyCount > 0
                    ? `采购倒挂 ${metrics.costAboveSupplyCount} 项`
                    : null,
                metrics.delayCount > 0
                    ? `交期晚于承诺 ${metrics.delayCount} 项`
                    : null,
            ]
                .filter(Boolean)
                .join(" · ")
            items.push({
                id: "po-medium-risks",
                tone: "warning",
                text: `另有明细风险待核对：${parts}`,
                action:
                    metrics.externalLossCount === 0 &&
                    metrics.quantityExcessCount === 0 &&
                    metrics.mismatchCount === 0 &&
                    metrics.actionableRiskLineCount > 0 &&
                    onFilterPurchaseRisks
                        ? {
                              id: `${idBase}-filter-risks`,
                              label: `仅看风险项（${metrics.actionableRiskLineCount}）`,
                              icon: "locate",
                              onClick: onFilterPurchaseRisks,
                          }
                        : undefined,
            })
        }
        if (missingDeliveryAddress) {
            items.push({
                id: "po-missing-address",
                tone: "warning",
                text: "当前履行方式缺少收货地址分配，请补充配送地址后再重提。",
            })
        }
        if (submission && attachmentCount === 0) {
            items.push({
                id: "po-missing-attachments",
                tone: "warning",
                text: "当前提交未附带报价单、合同或相关凭证附件。",
                action:
                    canOpenMaterials && onOpenMaterials
                        ? {
                              id: `${idBase}-open-materials`,
                              label: "查看附件栏",
                              icon: "locate",
                              onClick: onOpenMaterials,
                          }
                        : undefined,
            })
        }

        if (items.length > 0) {
            return {
                stance: "needs_fix",
                headline: "采购单存在以下风险与核对要点",
                items,
                description:
                    "可先点击「仅看风险项」定位异常行，再点击底栏「修改或重新提交 ↗」前往原采购单编辑后重提；若无需采购可点击「放弃本次办理」。",
                onRefresh,
                refreshing,
            }
        }

        return {
            stance: "ready",
            headline: "前置明细核查未发现价格或交期异常",
            description:
                "请结合上轮驳回意见核对下方明细，点击底栏「修改或重新提交 ↗」在工作台新页签编辑采购单并重新提交；或点击「放弃本次办理」。",
            onRefresh,
            refreshing,
        }
    }

    // 4. 销售单 / 卡券销售单（sales_order / voucher_sales_order）
    if (docType === "sales_order" || docType === "voucher_sales_order") {
        const salesSubmission = submission?.sales_order
        if (salesSubmission) {
            const rows = evaluateSalesApprovalLines(salesSubmission, () => "")
            const metrics = buildSalesApprovalMetrics(salesSubmission, rows)
            const risk = assessSalesApprovalRisk(salesSubmission)
            const evidenceMissing = missingSalesApprovalEvidence(
                salesSubmission,
                attachmentCount,
            )

            const items: WorkspaceReworkTriageItem[] = []
            if (metrics && metrics.riskLineCount > 0) {
                const counts = [
                    metrics.negativeMarginCount
                        ? `负毛利 ${metrics.negativeMarginCount}`
                        : null,
                    metrics.belowFloorCount
                        ? `低于原底价 ${metrics.belowFloorCount}`
                        : null,
                    metrics.priceAdjustedCount
                        ? `供给价调整 ${metrics.priceAdjustedCount}`
                        : null,
                    metrics.otherPriceRiskCount
                        ? `价格格式待核 ${metrics.otherPriceRiskCount}`
                        : null,
                ]
                    .filter(Boolean)
                    .join(" · ")
                items.push({
                    id: "so-line-risks",
                    tone: risk.highRisk ? "destructive" : "warning",
                    text: `${metrics.riskLineCount} 项商品存在价格或毛利风险（${counts}）`,
                    action: onFilterSalesRisks
                        ? {
                              id: `${idBase}-filter-risks`,
                              label: `仅看风险项（${metrics.riskLineCount}）`,
                              icon: "locate",
                              onClick: onFilterSalesRisks,
                          }
                        : undefined,
                })
            }
            if (evidenceMissing) {
                items.push({
                    id: "so-missing-evidence",
                    tone: "warning",
                    text: "整单未关联销售合同且暂无凭证附件，请确认是否需补充依据。",
                    action:
                        canOpenMaterials && onOpenMaterials
                            ? {
                                  id: `${idBase}-open-materials`,
                                  label: "核对凭证附件",
                                  icon: "locate",
                                  onClick: onOpenMaterials,
                              }
                            : undefined,
                })
            }

            if (items.length > 0) {
                return {
                    stance: "needs_fix",
                    headline: "销售单存在以下复核风险项",
                    items,
                    description:
                        "可先点击「仅看风险项」定位异常明细，再点击底栏「修改或重新提交 ↗」前往原销售单调整并重提；或点击「放弃本次办理」。",
                    onRefresh,
                    refreshing,
                }
            }
        }

        return {
            stance: "ready",
            headline: "前置明细未检出负毛利或价格倒挂风险",
            description:
                "请结合上轮驳回意见核对下方销售明细，点击底栏「修改或重新提交 ↗」前往原销售单编辑并重提；或点击「放弃本次办理」。",
            onRefresh,
            refreshing,
        }
    }

    // 5. 客户回款（customer_receipt）
    if (docType === "customer_receipt") {
        const risks = inspectReceiptRisks(
            submission?.display.source,
            submission ? attachmentCount : undefined,
        )
        if (risks.length > 0) {
            return {
                stance: "needs_fix",
                headline: `回款核验发现 ${risks.length} 项待核对事项`,
                items: risks.map((risk) => ({
                    id: `receipt-${risk.id}`,
                    tone: risk.severe ? "destructive" : "warning",
                    text: `${risk.label}：${risk.phrase}`,
                    action: onLocateReceiptRisk
                        ? {
                              id: `${idBase}-locate-${toAutomationIdSegment(risk.id)}`,
                              label: "定位核对",
                              icon: "locate",
                              onClick: () => onLocateReceiptRisk(risk.id),
                          }
                        : undefined,
                })),
                description:
                    "点击底栏「就地修改并重提」可直接修改回款金额、到账时间、银行流水与核销金额并重提；如需补充附件请点「打开原单页面」。",
                onRefresh,
                refreshing,
            }
        }

        return {
            stance: "ready",
            headline: "回款要素与核销配平预检通过",
            description:
                "点击底栏「就地修改并重提」可直接在弹窗内修改回款信息并填写重提说明；或点「打开原单页面」查看完整往来记录。",
            onRefresh,
            refreshing,
        }
    }

    // 6. 客户退款 / 供应商退款 / 收付款红字冲正（4 类逆向财务单据）
    if (isFinancialApprovalDocumentType(docType) && submission) {
        const summary = inspectFinancialApprovalRisks(
            docType,
            submission.display.source,
            submission.display.impact_summary,
            attachmentCount,
        )
        const items: WorkspaceReworkTriageItem[] = []
        if (summary.missingOriginalDocument) {
            items.push({
                id: "fin-missing-original",
                tone: "destructive",
                text: "原关联资金单据号待补全，请核对原单依据。",
            })
        }
        if (summary.evidenceMissing) {
            items.push({
                id: "fin-missing-evidence",
                tone: "warning",
                text: "本次提交尚未附带复核凭证附件。",
                action:
                    canOpenMaterials && onOpenMaterials
                        ? {
                              id: `${idBase}-open-materials`,
                              label: "查看附件栏",
                              icon: "locate",
                              onClick: onOpenMaterials,
                          }
                        : undefined,
            })
        }
        if (summary.severe || summary.impactDescriptions.length > 0) {
            const detailText =
                summary.impactDescriptions[0] ??
                (docType === "receipt_reversal" ||
                docType === "payment_reversal"
                    ? "红字冲正单据：重提审批通过后将冲回原收付款记录并同步调整往来余额。"
                    : "单笔退款金额已达到高危复核阈值，请核对金额与退款原因。")
            items.push({
                id: "fin-impact",
                tone: summary.severe ? "destructive" : "warning",
                text: detailText,
                action:
                    summary.hasLines && onLocateFinancialLines
                        ? {
                              id: `${idBase}-locate-lines`,
                              label: "查看原单核销明细",
                              icon: "locate",
                              onClick: onLocateFinancialLines,
                          }
                        : undefined,
            })
        }

        if (items.length > 0) {
            return {
                stance: "needs_fix",
                headline: "提交前请结合驳回意见核对以下资金影响与依据",
                items,
                description:
                    "点击底栏「就地修改并重提」可在弹窗内直接调整金额及原因说明并重提；若无需继续办理可点击「放弃本次办理」。",
                onRefresh,
                refreshing,
            }
        }
    }

    // 7. 库存调整 / 销售变更 / 采购变更（3 类结构化差异单据）
    if (isStructuredDiffDocumentType(docType) && submission) {
        const diffSummary = inspectStructuredDiffRisks(
            docType,
            submission.display.source,
            submission.display.impact_summary,
        )
        const items: WorkspaceReworkTriageItem[] = []
        if (diffSummary.missingWarehouse) {
            items.push({
                id: "diff-missing-warehouse",
                tone: "destructive",
                text: "库存调整单缺少仓库信息，请补齐后重新提交。",
            })
        }
        if (diffSummary.missingBaseline) {
            items.push({
                id: "diff-missing-baseline",
                tone: "warning",
                text: "变更基准快照缺失，请打开编辑弹窗确认最新变更明细。",
            })
        }
        if (
            diffSummary.changedHeaderCount > 0 ||
            diffSummary.changedLineCount > 0 ||
            diffSummary.deltaSigned ||
            diffSummary.impactSummary
        ) {
            const summaryText = [
                diffSummary.changedHeaderCount > 0
                    ? `表头变更 ${diffSummary.changedHeaderCount} 项`
                    : null,
                diffSummary.changedLineCount > 0
                    ? `明细变更 ${diffSummary.changedLineCount} 行`
                    : null,
                diffSummary.deltaSigned
                    ? `金额净变 ${diffSummary.deltaSigned}`
                    : null,
                diffSummary.impactSummary ?? null,
            ]
                .filter(Boolean)
                .join(" · ")
            items.push({
                id: "diff-summary",
                tone: diffSummary.severe ? "destructive" : "warning",
                text: summaryText,
                action:
                    diffSummary.visibleChanged > 0 && onLocateDiffChanges
                        ? {
                              id: `${idBase}-locate-changes`,
                              label: `仅看变更行（${diffSummary.visibleChanged}）`,
                              icon: "locate",
                              onClick: onLocateDiffChanges,
                          }
                        : undefined,
            })
        }

        if (items.length > 0) {
            return {
                stance: "needs_fix",
                headline: "提交前请核对以下变更影响与差异明细",
                items,
                description:
                    "点击底栏「就地修改并重提」可在弹窗内直接调整内容并填写重提说明；或点「打开原单页面」查看上下文。",
                onRefresh,
                refreshing,
            }
        }
    }

    // 兜底态势 A（如未加载出额外风险的就地修改单据或通用单据）
    return {
        stance: "ready",
        headline: "前置校验未发现硬阻塞，核对后即可修改重提",
        description: supportsInlineEdit
            ? "点击底栏「就地修改并重提」直接在弹窗内修改并重新提交；或点击「放弃本次办理」关闭此待办。"
            : onOpenEdit
              ? "点击底栏「修改或重新提交 ↗」前往原单编辑并重新提交；或点击「放弃本次办理」关闭此待办。"
              : "请核对驳回意见后在下方选择对应办理操作。",
        onRefresh,
        refreshing,
    }
}
