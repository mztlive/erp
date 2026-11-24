import { useEffect, useRef, useState, type ReactNode } from "react"
import Link from "next/link"
import {
    ChevronRightIcon,
    CircleHelpIcon,
    TriangleAlertIcon,
} from "lucide-react"

import {
    MoneyValue,
    WorkspaceFactGrid,
    WorkspaceFactItem,
} from "@/components/business"
import { useUnifiedTaskWorkbenchRail } from "@/components/business/unified-task-workbench"
import { Button, buttonVariants } from "@/components/ui/button"
import {
    Tooltip,
    TooltipContent,
    TooltipProvider,
    TooltipTrigger,
} from "@/components/ui/tooltip"
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

function Kpi({
    label,
    children,
    detail,
}: {
    label: ReactNode
    children: ReactNode
    detail?: ReactNode
}) {
    return (
        <div className="min-w-0 space-y-1 wrap-anywhere">
            <dt className="flex items-center gap-1 text-xs text-muted-foreground">
                {label}
            </dt>
            <dd className="text-body-compact font-medium">{children}</dd>
            {detail ? (
                <dd className="text-tiny text-muted-foreground">{detail}</dd>
            ) : null}
        </div>
    )
}

function ImpactKpi({
    amount,
    idPrefix,
    difference,
    impactLabel,
}: {
    amount?: InvestigationAmount
    idPrefix: string
    difference: boolean
    impactLabel: string
}) {
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
    return (
        <Kpi
            label={
                <>
                    {value && amount
                        ? amount.label
                        : difference
                          ? "差异金额"
                          : "资金影响"}
                    <Tooltip>
                        <TooltipTrigger
                            id={`${idPrefix}-amount-basis`}
                            render={
                                <Button
                                    type="button"
                                    variant="ghost"
                                    size="icon-xs"
                                    aria-label={
                                        difference
                                            ? "查看对账金额口径"
                                            : "查看资金影响口径"
                                    }
                                    className="rounded-full text-muted-foreground"
                                />
                            }
                        >
                            <CircleHelpIcon aria-hidden="true" />
                        </TooltipTrigger>
                        <TooltipContent>
                            {difference
                                ? isDifferenceAmount && value
                                    ? "取待办记录的差异金额，正负沿用来源记录口径。"
                                    : "差异详情提供两侧凭证名称。金额须按原始凭证核对。"
                                : "反映本条外部同步异常是否涉及收付款或结算资金变动，具体金额以关联凭证为准。"}
                        </TooltipContent>
                    </Tooltip>
                </>
            }
            detail={value || difference ? impactLabel : undefined}
        >
            {value ? (
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
                        className="[&_[data-slot=money-value-number]]:wrap-anywhere [&_[data-slot=money-value-number]]:whitespace-normal"
                    />
                </span>
            ) : difference ? (
                "待核实"
            ) : (
                impactLabel
            )}
        </Kpi>
    )
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
                <section
                    role="alert"
                    aria-label="异常诊断"
                    className={cn(
                        "flex min-w-0 flex-wrap items-center justify-between gap-x-3 gap-y-1.5 rounded-r-md border-l-2 px-3 py-2.5",
                        severe
                            ? "border-l-destructive bg-destructive-soft/65 text-destructive-soft-foreground"
                            : "border-l-warning bg-warning-soft/75 text-warning-soft-foreground",
                    )}
                >
                    <div className="flex min-w-0 flex-[1_1_12rem] items-start gap-2">
                        <TriangleAlertIcon
                            aria-hidden="true"
                            className="mt-0.5 size-3.5 shrink-0"
                        />
                        <div className="min-w-0 flex-1 space-y-1 wrap-anywhere">
                            <p className="text-body-compact font-semibold">
                                {rootSummary}
                            </p>
                            <p className="text-xs opacity-90">
                                异常代码{" "}
                                <code className="num select-all">
                                    {item.classification.code}
                                </code>
                                {" · "}
                                {safeNextStep(item, canQuery)}
                            </p>
                        </div>
                    </div>
                    <Button
                        id={`${idPrefix}-focus-diagnostics`}
                        type="button"
                        size="xs"
                        variant="ghost"
                        className={cn(
                            "shrink-0 font-medium shadow-none",
                            severe
                                ? "text-destructive-soft-foreground hover:bg-destructive-soft hover:text-destructive-soft-foreground"
                                : "text-warning-soft-foreground hover:bg-warning-soft hover:text-warning-soft-foreground",
                        )}
                        onClick={() => {
                            if (difference && fieldHeading.current) {
                                onFieldViewChange({ filter: "", page: 0 })
                                setHighlightFields(true)
                                if (highlightTimer.current !== null) {
                                    window.clearTimeout(highlightTimer.current)
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
                                    behavior: reduceMotion ? "auto" : "smooth",
                                })
                            } else {
                                rail?.expand()
                                onFocusPayload()
                            }
                        }}
                    >
                        {difference ? "查看差异明细" : "查看诊断载荷"}
                        <ChevronRightIcon aria-hidden="true" />
                    </Button>
                </section>
            ) : null}
            {feedback}
            <TooltipProvider>
                <dl
                    aria-label="调查指标"
                    className="grid grid-cols-1 gap-x-5 gap-y-3 rounded-lg bg-surface-sunken px-4 py-3.5 @[480px]/integration-task:grid-cols-2 @[840px]/integration-task:grid-cols-4"
                >
                    <ImpactKpi
                        idPrefix={idPrefix}
                        amount={amount}
                        difference={Boolean(difference)}
                        impactLabel={item.fundsImpactLabel}
                    />
                    <Kpi
                        label={
                            <>
                                执行尝试
                                <Tooltip>
                                    <TooltipTrigger
                                        id={`${idPrefix}-attempt-basis`}
                                        render={
                                            <Button
                                                type="button"
                                                variant="ghost"
                                                size="icon-xs"
                                                aria-label="查看执行尝试统计口径"
                                                className="rounded-full text-muted-foreground"
                                            />
                                        }
                                    >
                                        <CircleHelpIcon aria-hidden="true" />
                                    </TooltipTrigger>
                                    <TooltipContent>
                                        统计该异常记录的历史执行、查询与重放累计尝试次数。
                                    </TooltipContent>
                                </Tooltip>
                            </>
                        }
                    >
                        {attemptCount != null ? (
                            <span className="num text-value-compact">
                                {attemptCount}
                            </span>
                        ) : (
                            "暂无记录"
                        )}
                    </Kpi>
                    <Kpi
                        label={classificationInHeader ? "异常等级" : "异常分类"}
                        detail={
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
                                        发生于 <DateValue date={occurredAt} />
                                    </>
                                ) : null}
                            </>
                        }
                    >
                        {classificationInHeader
                            ? item.classification.severityLabel
                            : `${item.classification.label} · ${item.classification.severityLabel}`}
                    </Kpi>
                    <Kpi
                        label={titleInHeader ? "完成条件" : "受影响对象"}
                        detail={
                            !titleInHeader
                                ? condition || "核对关联凭证"
                                : reviewerRequirement
                        }
                    >
                        {titleInHeader
                            ? condition ||
                              (terminal ? item.status.label : "核对关联凭证")
                            : item.businessObject.title}
                    </Kpi>
                </dl>
            </TooltipProvider>
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
