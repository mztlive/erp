import { useEffect, useRef, useState, type ReactNode } from "react"
import Link from "next/link"
import { ChevronRightIcon } from "lucide-react"

import {
    DecisionKpiBar,
    type DecisionKpiItem,
    MoneyValue,
    TaskRiskSummaryBar,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { useUnifiedTaskWorkbenchRail } from "@/components/business/unified-task-workbench"
import { buttonVariants } from "@/components/ui/button"
import { formatDateTime } from "@/lib/datetime"
import { compareDecimal } from "@/lib/fixed-decimal"
import { cn } from "@/lib/utils"

import {
    EVIDENCE_KIND_LABEL,
    REVIEWER_SEPARATION_LABEL,
    type IntegrationResolutionItemView,
} from "../../../types"
import {
    InvestigationFieldTable,
    type InvestigationFieldView,
} from "./field-table"

export type InvestigationAmount = { label: string; value: string }

function buildImpactKpiItem({
    amount,
    idPrefix,
    difference,
    impactLabel,
}: {
    amount?: InvestigationAmount
    idPrefix: string
    difference: boolean
    impactLabel: string
}): DecisionKpiItem {
    const amountMatch = amount?.value
        .trim()
        .match(/^(-?)[¥￥]?((?:\d+|\d{1,3}(?:,\d{3})+)(?:\.\d{1,4})?)$/u)
    const value = amountMatch
        ? amountMatch[1] +
          amountMatch[2].replaceAll(",", "").replace(/^0+(?=\d)/u, "")
        : undefined
    const isDifferenceAmount =
        difference && Boolean(amount && /差额|差异金额/u.test(amount.label))
    const sign = isDifferenceAmount && value ? compareDecimal(value, "0", 4) : 0
    const direction = sign > 0 ? "正差" : sign < 0 ? "负差" : "无差额"
    return {
        id: "impact",
        label:
            value && amount
                ? amount.label
                : difference
                  ? "差异金额"
                  : "资金影响",
        emphasis: value ? "primary" : "terms",
        formulaTooltipId: `${idPrefix}-amount-basis`,
        formulaTooltipLabel: difference
            ? "查看对账金额口径"
            : "查看资金影响口径",
        formulaTooltip: difference
            ? isDifferenceAmount && value
                ? "取待办记录的差异金额，正负沿用来源记录口径。"
                : "差异详情提供两侧凭证名称。金额须按原始凭证核对。"
            : "反映本条外部同步异常是否涉及收付款或结算资金变动，具体金额以关联凭证为准。",
        footnote: value || difference ? impactLabel : undefined,
        value: value ? (
            <span
                className={cn(
                    "num inline-flex flex-wrap items-baseline gap-x-1.5",
                    sign > 0 && "text-warning-soft-foreground",
                    sign < 0 && "text-destructive",
                )}
            >
                {isDifferenceAmount ? (
                    <span className="text-body-compact">{direction}</span>
                ) : null}
                <MoneyValue
                    display="exact"
                    size="section"
                    value={value}
                    inheritColor={isDifferenceAmount}
                />
            </span>
        ) : difference ? (
            "待核实"
        ) : (
            impactLabel
        ),
    }
}

function DateValue({ date }: { date: string }) {
    return (
        <time dateTime={date} className="num">
            {formatDateTime(date, "default")}
        </time>
    )
}

function safeNextStep(item: IntegrationResolutionItemView, canQuery: boolean) {
    const errorClass = item.classification.errorClass
    if (item.difference) return "核对两侧凭证与业务记录后，再登记处理结论。"
    if (errorClass === "result-unknown" || errorClass === "network-timeout") {
        return canQuery
            ? "先查询原结果，确认原操作未生效后再决定是否重放。"
            : "请具备处理权限的责任人核实原操作结果。"
    }
    if (errorClass === "authentication-or-signature")
        return "核验接口授权和签名配置，取得验证结果后再处理。"
    if (errorClass === "parameter-or-mapping")
        return "核对来源资料和编码映射后再处理。"
    if (errorClass === "rate-limited") return "查看最近执行结果和重试安排。"
    return "先核对最近执行结果与关联凭证。"
}

export function InvestigationCanvas({
    item,
    idPrefix,
    amount,
    attemptCount,
    terminal,
    canQuery,
    canOpenSettings,
    classificationInHeader,
    deadline,
    titleInHeader,
    onFocusPayload,
    fieldView,
    onFieldViewChange,
    feedback,
    actionForm,
}: {
    item: IntegrationResolutionItemView
    idPrefix: string
    amount?: InvestigationAmount
    attemptCount?: number
    terminal: boolean
    canQuery: boolean
    canOpenSettings: boolean
    classificationInHeader: boolean
    deadline?: { date: string; label: string }
    titleInHeader: boolean
    onFocusPayload: () => void
    fieldView: InvestigationFieldView
    onFieldViewChange: (view: InvestigationFieldView) => void
    feedback: ReactNode
    actionForm: ReactNode
}) {
    const rail = useUnifiedTaskWorkbenchRail()
    const fieldHeading = useRef<HTMLHeadingElement>(null)
    const [highlightFields, setHighlightFields] = useState(false)
    const highlightTimer = useRef<number | null>(null)
    useEffect(
        () => () => {
            if (highlightTimer.current !== null) {
                window.clearTimeout(highlightTimer.current)
            }
        },
        [],
    )
    const difference = item.difference
    const policy = item.resolutionEvidencePolicy
    const failure = item.failureSource
    const severe = ["critical", "high"].includes(item.classification.severity)
    const occurredAt = failure?.failedAt || item.createdAt
    const rootSummary =
        failure?.summary ||
        difference?.differenceSummary ||
        item.classification.label
    const condition = policy?.requiredEvidenceKinds
        .map((kind) => EVIDENCE_KIND_LABEL[kind])
        .join("、")
    const reviewerRequirement =
        policy && policy.reviewerSeparation !== "NONE"
            ? REVIEWER_SEPARATION_LABEL[policy.reviewerSeparation]
            : undefined

    return (
        <div className="@container/integration-task min-w-0 space-y-3.5 bg-card py-3.5">
            {!terminal ? (
                <TaskRiskSummaryBar
                    severity={severe ? "destructive" : "warning"}
                    aria-label="异常诊断"
                    leadLabel={rootSummary}
                    summary={
                        <>
                            异常代码{" "}
                            <code className="num select-all">
                                {item.classification.code}
                            </code>
                            {" · "}
                            {safeNextStep(item, canQuery)}
                        </>
                    }
                    actions={[
                        {
                            id: "focus-diagnostics",
                            buttonId: `${idPrefix}-focus-diagnostics`,
                            label: difference ? "查看差异明细" : "查看诊断载荷",
                            onClick: () => {
                                if (difference && fieldHeading.current) {
                                    onFieldViewChange({ filter: "", page: 0 })
                                    setHighlightFields(true)
                                    if (highlightTimer.current !== null) {
                                        window.clearTimeout(
                                            highlightTimer.current,
                                        )
                                    }
                                    highlightTimer.current = window.setTimeout(
                                        () => setHighlightFields(false),
                                        2200,
                                    )
                                    const reduceMotion = window.matchMedia(
                                        "(prefers-reduced-motion: reduce)",
                                    ).matches
                                    fieldHeading.current.focus({
                                        preventScroll: true,
                                    })
                                    fieldHeading.current.scrollIntoView({
                                        block: "center",
                                        behavior: reduceMotion
                                            ? "auto"
                                            : "smooth",
                                    })
                                } else {
                                    rail?.expand()
                                    onFocusPayload()
                                }
                            },
                        },
                    ]}
                />
            ) : null}
            {feedback}
            <DecisionKpiBar
                columns={4}
                ariaLabel="调查指标"
                items={[
                    buildImpactKpiItem({
                        idPrefix,
                        amount,
                        difference: Boolean(difference),
                        impactLabel: item.fundsImpactLabel,
                    }),
                    {
                        id: "attempt",
                        label: "执行尝试",
                        emphasis: attemptCount != null ? "secondary" : "terms",
                        formulaTooltipId: `${idPrefix}-attempt-basis`,
                        formulaTooltipLabel: "查看执行尝试统计口径",
                        formulaTooltip:
                            "统计该异常记录的历史执行、查询与重放累计尝试次数。",
                        value: attemptCount != null ? attemptCount : "暂无记录",
                    },
                    {
                        id: "classification",
                        label: classificationInHeader ? "异常等级" : "异常分类",
                        emphasis: "terms",
                        value: classificationInHeader
                            ? item.classification.severityLabel
                            : `${item.classification.label} · ${item.classification.severityLabel}`,
                        footnote:
                            deadline || occurredAt ? (
                                <>
                                    {deadline ? (
                                        <>
                                            截止{" "}
                                            <time
                                                className="num"
                                                dateTime={deadline.date}
                                            >
                                                {deadline.label ||
                                                    formatDateTime(
                                                        deadline.date,
                                                        "default",
                                                    )}
                                            </time>
                                            {" · "}
                                        </>
                                    ) : null}
                                    {occurredAt ? (
                                        <>
                                            发生于{" "}
                                            <DateValue date={occurredAt} />
                                        </>
                                    ) : null}
                                </>
                            ) : undefined,
                    },
                    {
                        id: "target",
                        label: titleInHeader ? "完成条件" : "受影响对象",
                        emphasis: "terms",
                        value: titleInHeader
                            ? condition ||
                              (terminal ? item.status.label : "核对关联凭证")
                            : item.businessObject.title,
                        footnote: !titleInHeader
                            ? condition || "核对关联凭证"
                            : reviewerRequirement,
                    },
                ]}
            />
            {failure?.operation ||
            failure?.jobNumber ||
            item.environment ||
            item.ownerUser ||
            item.ownerRole ||
            item.compensationOpen ||
            (!titleInHeader && reviewerRequirement) ||
            (difference?.watermark && difference.watermark !== occurredAt) ? (
                <WorkspaceFactGrid columns={3}>
                    {failure?.operation ? (
                        <WorkspaceFactItem
                            label="来源操作"
                            value={failure.operation}
                        />
                    ) : null}
                    {failure?.jobNumber ? (
                        <WorkspaceFactItem
                            label="执行批次"
                            value={failure.jobNumber}
                            numeric
                            selectable
                        />
                    ) : null}
                    {item.environment ? (
                        <WorkspaceFactItem
                            label="环境"
                            value={item.environmentLabel}
                        />
                    ) : null}
                    {item.ownerUser || item.ownerRole ? (
                        <WorkspaceFactItem
                            label="责任人"
                            value={item.ownerUser || item.ownerRole}
                        />
                    ) : null}
                    {item.compensationOpen ? (
                        <WorkspaceFactItem
                            label="补救进度"
                            value="待完成"
                            tone="warning"
                        />
                    ) : null}
                    {difference?.watermark &&
                    difference.watermark !== occurredAt ? (
                        <WorkspaceFactItem label="对账时点" numeric selectable>
                            <DateValue date={difference.watermark} />
                        </WorkspaceFactItem>
                    ) : null}
                    {!titleInHeader && reviewerRequirement ? (
                        <WorkspaceFactItem
                            label="复核要求"
                            value={reviewerRequirement}
                            span="full"
                        />
                    ) : null}
                </WorkspaceFactGrid>
            ) : null}
            {canOpenSettings
                ? item.repairLinks
                      .filter((link) => link.workspaceId === "W20")
                      .map((link) => (
                          <Link
                              key={link.workspaceId}
                              id={`${idPrefix}-connection-settings`}
                              href={link.href}
                              className={buttonVariants({
                                  variant: "outline",
                                  size: "xs",
                              })}
                          >
                              {link.label}
                              <ChevronRightIcon aria-hidden="true" />
                          </Link>
                      ))
                : null}
            <InvestigationFieldTable
                item={item}
                idPrefix={idPrefix}
                headingRef={fieldHeading}
                view={fieldView}
                onViewChange={onFieldViewChange}
                highlighted={highlightFields}
            />
            {actionForm}
        </div>
    )
}
