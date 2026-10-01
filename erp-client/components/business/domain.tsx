"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"

import {
    Card,
    CardAction,
    CardContent,
    CardDescription,
    CardFooter,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import {
    DescriptionDetails,
    DescriptionItem,
    DescriptionList,
    DescriptionTerm,
} from "@/components/ui/description-list"
import {
    HoverCard,
    HoverCardContent,
    HoverCardTrigger,
} from "@/components/ui/hover-card"
import { Progress, ProgressLabel } from "@/components/ui/progress"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { cn } from "@/lib/utils"
import {
    DomainTime,
    NumericValue,
    type DomainDateTime,
    type DomainPanelProps,
} from "@/components/business/domain-shared"

export type PrepaymentGateCondition =
    | Readonly<{
          kind: "amount"
          required: React.ReactNode
          description?: React.ReactNode
      }>
    | Readonly<{
          kind: "ratio"
          required: React.ReactNode
          description?: React.ReactNode
      }>

export type PrepaymentGateProps = DomainPanelProps & {
    condition: PrepaymentGateCondition
    allocated: React.ReactNode
    gap: React.ReactNode
    updatedAt: DomainDateTime
    allowed: boolean
    paymentAction?: React.ReactNode
    /**
     * 面向一线作业角色时可整体替换为业务口语（如 W09 履约作业）。
     * 不传则沿用面向采购/财务的默认措辞。
     */
    copy?: Partial<PrepaymentGateCopy>
    /**
     * `panel`：完整卡片（采购中心默认）。
     * `badge`：仅结果徽章，悬停/聚焦展开详情（适合连续作业顶栏，避免打断读单）。
     */
    presentation?: "panel" | "badge"
}

export type PrepaymentGateCopy = {
    title: string
    description: string
    allowedBadge: string
    blockedBadge: string
    amountTerm: string
    ratioTerm: string
    allocatedTerm: string
    gapTerm: string
    updatedTerm: string
    allowedTitle: string
    blockedTitle: string
    allowedBody: string
    blockedBody: string
}

const PREPAYMENT_GATE_COPY: PrepaymentGateCopy = {
    title: "先款后货门禁",
    description: "仅按已确认付款的有效净分配判断，不以付款申请或附件代替。",
    allowedBadge: "允许继续履约",
    blockedBadge: "履约已阻断",
    amountTerm: "最低有效付款金额",
    ratioTerm: "最低有效付款比例",
    allocatedTerm: "已分配",
    gapTerm: "当前缺口",
    updatedTerm: "计算更新时间",
    allowedTitle: "付款门禁已满足",
    blockedTitle: "付款门禁尚未满足",
    allowedBody: "当前有效净付款分配已达到冻结条件，可以继续本次履约。",
    blockedBody: "新的入库、直发、电子交付或服务确认必须等待缺口补齐。",
}

function PrepaymentGate({
    condition,
    allocated,
    gap,
    updatedAt,
    allowed,
    paymentAction,
    copy,
    presentation = "panel",
    className,
    ...props
}: PrepaymentGateProps) {
    const text = copy
        ? { ...PREPAYMENT_GATE_COPY, ...copy }
        : PREPAYMENT_GATE_COPY
    const conditionLabel =
        condition.kind === "amount" ? text.amountTerm : text.ratioTerm
    const resultLabel = allowed ? text.allowedBadge : text.blockedBadge
    const resultTone: StatusTone = allowed ? "success" : "warning"

    const metrics = (
        <DescriptionList columns={presentation === "badge" ? "two" : "four"}>
            <DescriptionItem>
                <DescriptionTerm>{conditionLabel}</DescriptionTerm>
                <DescriptionDetails>
                    <NumericValue>{condition.required}</NumericValue>
                    {condition.description != null ? (
                        <span className="mt-1 block text-xs text-muted-foreground">
                            {condition.description}
                        </span>
                    ) : null}
                </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
                <DescriptionTerm>{text.allocatedTerm}</DescriptionTerm>
                <DescriptionDetails>
                    <NumericValue>{allocated}</NumericValue>
                </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
                <DescriptionTerm>{text.gapTerm}</DescriptionTerm>
                <DescriptionDetails>
                    <NumericValue>{gap}</NumericValue>
                </DescriptionDetails>
            </DescriptionItem>
            <DescriptionItem>
                <DescriptionTerm>{text.updatedTerm}</DescriptionTerm>
                <DescriptionDetails>
                    <DomainTime value={updatedAt} />
                </DescriptionDetails>
            </DescriptionItem>
        </DescriptionList>
    )

    const conclusion = (
        <Alert variant={allowed ? "success" : "warning"}>
            <AlertTitle>
                {allowed ? text.allowedTitle : text.blockedTitle}
            </AlertTitle>
            <AlertDescription>
                {allowed ? text.allowedBody : text.blockedBody}
            </AlertDescription>
        </Alert>
    )

    if (presentation === "badge") {
        const { id } = props
        return (
            <HoverCard>
                <HoverCardTrigger
                    id={id}
                    data-slot="prepayment-gate"
                    data-allowed={allowed}
                    data-presentation="badge"
                    render={
                        <button
                            type="button"
                            className={cn(
                                "inline-flex rounded-2xl outline-none focus-visible:ring-[3px] focus-visible:ring-ring/50",
                                className,
                            )}
                            aria-label={`${text.title}：${resultLabel}`}
                        />
                    }
                >
                    <StatusBadge tone={resultTone} label={resultLabel} />
                </HoverCardTrigger>
                <HoverCardContent
                    align="start"
                    side="bottom"
                    className="w-80 space-y-3 p-4 sm:w-96"
                >
                    <div className="space-y-1">
                        <p className="text-sm font-medium">{text.title}</p>
                        <p className="text-xs text-muted-foreground">
                            {text.description}
                        </p>
                    </div>
                    {metrics}
                    {conclusion}
                    {paymentAction != null ? (
                        <div className="flex justify-end border-t border-border pt-3">
                            {paymentAction}
                        </div>
                    ) : null}
                </HoverCardContent>
            </HoverCard>
        )
    }

    return (
        <Card
            data-slot="prepayment-gate"
            data-allowed={allowed}
            data-presentation="panel"
            className={className}
            {...props}
        >
            <CardHeader className="border-b border-border">
                <CardTitle>{text.title}</CardTitle>
                <CardDescription>{text.description}</CardDescription>
                <CardAction>
                    <StatusBadge tone={resultTone} label={resultLabel} />
                </CardAction>
            </CardHeader>

            <CardContent className="space-y-4">
                {metrics}
                {conclusion}
            </CardContent>

            {paymentAction != null ? (
                <CardFooter className="justify-end border-t border-border">
                    {paymentAction}
                </CardFooter>
            ) : null}
        </Card>
    )
}

export type CostBasis = "ACTUAL" | "STANDARD" | "NONE"

export type CostCoverageBreakdown = Readonly<Record<CostBasis, React.ReactNode>>

export type CostCoverageNoticeProps = DomainPanelProps & {
    basis: CostBasis
    /** 仅用于进度条渲染的 0..100 数值投影，由调用方提供。 */
    coveragePercent: number
    /** 服务端口径化后的覆盖率文本；组件不重新计算或格式化。 */
    coverageLabel: React.ReactNode
    /** 服务端判定的覆盖状态。 */
    coverageState: "complete" | "partial" | "none"
    breakdown: CostCoverageBreakdown
    profitBasis: React.ReactNode
    notice?: React.ReactNode
}

const costBasisStatus = {
    ACTUAL: {
        label: "实际成本",
        tone: "success",
        description: "当前成本来自实际发生或后续权威差额。",
    },
    STANDARD: {
        label: "标准成本",
        tone: "info",
        description: "当前成本使用消费发生时有效的标准供给成本。",
    },
    NONE: {
        label: "无可用成本",
        tone: "warning",
        description: "当前没有有效成本来源，不得按零成本计算利润。",
    },
} satisfies Record<
    CostBasis,
    {
        label: string
        tone: StatusTone
        description: string
    }
>

const costCoverageStatus = {
    complete: {
        label: "成本已覆盖",
        tone: "success",
        alert: "success",
        description: "当前范围的成本已完整覆盖。",
    },
    partial: {
        label: "成本部分覆盖",
        tone: "warning",
        alert: "warning",
        description: "当前范围仍有未覆盖成本，利润必须与覆盖率同时解读。",
    },
    none: {
        label: "成本未覆盖",
        tone: "destructive",
        alert: "destructive",
        description: "当前范围没有可用成本，不得按零成本计算利润。",
    },
} satisfies Record<
    "complete" | "partial" | "none",
    {
        label: string
        tone: StatusTone
        alert: "success" | "warning" | "destructive"
        description: string
    }
>

function CostCoverageNotice({
    basis,
    coveragePercent,
    coverageLabel,
    coverageState,
    breakdown,
    profitBasis,
    notice,
    className,
    ...props
}: CostCoverageNoticeProps) {
    const currentBasis = costBasisStatus[basis]
    const coverageStatus = costCoverageStatus[coverageState]

    return (
        <Card
            data-slot="cost-coverage-notice"
            data-cost-basis={basis}
            className={className}
            {...props}
        >
            <CardHeader className="border-b border-border">
                <CardTitle>成本覆盖</CardTitle>
                <CardDescription>
                    成本来源与覆盖率必须和利润口径同时展示。
                </CardDescription>
                <CardAction className="flex flex-wrap items-center justify-end gap-2">
                    <StatusBadge
                        tone={coverageStatus.tone}
                        label={coverageStatus.label}
                    />
                </CardAction>
            </CardHeader>

            <CardContent className="space-y-4">
                <Progress value={coveragePercent}>
                    <ProgressLabel>成本覆盖率</ProgressLabel>
                    <span className="num ml-auto text-sm text-muted-foreground">
                        {coverageLabel}
                    </span>
                </Progress>

                <DescriptionList columns="three">
                    {(Object.keys(costBasisStatus) as CostBasis[]).map(
                        (itemBasis) => (
                            <DescriptionItem key={itemBasis}>
                                <DescriptionTerm>
                                    {costBasisStatus[itemBasis].label}
                                </DescriptionTerm>
                                <DescriptionDetails>
                                    <NumericValue>
                                        {breakdown[itemBasis]}
                                    </NumericValue>
                                </DescriptionDetails>
                            </DescriptionItem>
                        ),
                    )}
                </DescriptionList>

                <DescriptionList columns="two">
                    <DescriptionItem>
                        <DescriptionTerm>当前成本口径</DescriptionTerm>
                        <DescriptionDetails>
                            <StatusBadge
                                tone={currentBasis.tone}
                                label={currentBasis.label}
                            />
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>利润口径</DescriptionTerm>
                        <DescriptionDetails>{profitBasis}</DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>

                <Alert variant={coverageStatus.alert}>
                    <AlertTitle>{coverageStatus.label}</AlertTitle>
                    <AlertDescription>
                        {notice ?? (
                            <>
                                {coverageStatus.description}{" "}
                                {currentBasis.description}
                            </>
                        )}
                    </AlertDescription>
                </Alert>
            </CardContent>
        </Card>
    )
}

export { CostCoverageNotice, PrepaymentGate }

export * from "@/components/business/domain-interface-errors"
