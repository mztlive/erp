"use client"

import * as React from "react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
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
    Item,
    ItemActions,
    ItemContent,
    ItemDescription,
    ItemGroup,
    ItemTitle,
} from "@/components/ui/item"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import {
    DomainTime,
    type DomainDateTime,
    type DomainPanelProps,
} from "@/components/business/domain-shared"
import { interfaceText, resultText } from "@/lib/ui-text"

export type InterfaceErrorClass =
    | "capability-unsupported"
    | "parameter-or-mapping"
    | "business-rejected"
    | "network-timeout"
    | "result-unknown"
    | "authentication-or-signature"
    | "rate-limited"
    | "duplicate-callback"
    | "out-of-order-callback"

export type InterfaceErrorStatus =
    | "pending"
    | "auto-retrying"
    | "manual-required"
    | "resolved"
    | "closed"

export type InterfaceAttemptSummary = Readonly<{
    attemptNumber: number
    attemptedAt: DomainDateTime
    result: React.ReactNode
    requestSummary?: React.ReactNode
    responseSummary?: React.ReactNode
    nextRetryAt?: DomainDateTime
}>

type NoInterfaceErrorAction = Readonly<{
    stage: "none"
    queryOriginal?: never
    retrySameKey?: never
    manual?: never
    close?: never
    terminalEvidence?: never
    terminalBasis?: never
    queryResult?: never
}>

type QueryOriginalAction = Readonly<{
    stage: "query-original"
    queryOriginal: React.ReactElement
    retrySameKey?: never
    manual?: never
    close?: never
    terminalEvidence?: never
    terminalBasis?: never
    queryResult?: never
}>

type RetrySameKeyAction = Readonly<{
    stage: "safe-retry"
    queryResult: "confirmed-no-result"
    retrySameKey: React.ReactElement
    queryOriginal?: never
    manual?: never
    close?: never
    terminalEvidence?: never
    terminalBasis?: never
}>

type ManualResolutionAction = Readonly<{
    stage: "manual"
    manual: React.ReactElement
    queryOriginal?: never
    retrySameKey?: never
    close?: never
    terminalEvidence?: never
    terminalBasis?: never
    queryResult?: never
}>

type CloseResolutionAction = Readonly<{
    stage: "closable"
    terminalBasis: "verified-terminal" | "compensated-and-reconciled"
    terminalEvidence: string | React.ReactElement
    close: React.ReactElement
    queryOriginal?: never
    retrySameKey?: never
    manual?: never
    queryResult?: never
}>

export type InterfaceErrorResolutionActions =
    | NoInterfaceErrorAction
    | QueryOriginalAction
    | RetrySameKeyAction
    | ManualResolutionAction
    | CloseResolutionAction

type InterfaceErrorResolutionPanelBaseProps = DomainPanelProps & {
    status: InterfaceErrorStatus
    businessImpact: React.ReactNode
    latestAttempt: InterfaceAttemptSummary
    errorCode?: React.ReactNode
}

type QueryableInterfaceErrorClass = "network-timeout" | "result-unknown"

type ManualInterfaceErrorClass =
    | "capability-unsupported"
    | "parameter-or-mapping"
    | "business-rejected"
    | "authentication-or-signature"
    | "out-of-order-callback"

export type InterfaceErrorResolutionPanelProps =
    | (InterfaceErrorResolutionPanelBaseProps & {
          errorClass: QueryableInterfaceErrorClass
          actions?:
              | NoInterfaceErrorAction
              | QueryOriginalAction
              | RetrySameKeyAction
              | ManualResolutionAction
              | CloseResolutionAction
      })
    | (InterfaceErrorResolutionPanelBaseProps & {
          errorClass: ManualInterfaceErrorClass
          actions?:
              | NoInterfaceErrorAction
              | ManualResolutionAction
              | CloseResolutionAction
      })
    | (InterfaceErrorResolutionPanelBaseProps & {
          errorClass: "rate-limited"
          actions?: NoInterfaceErrorAction | ManualResolutionAction
      })
    | (InterfaceErrorResolutionPanelBaseProps & {
          errorClass: "duplicate-callback"
          actions?: NoInterfaceErrorAction | CloseResolutionAction
      })

const interfaceErrorClassPresentation = {
    "capability-unsupported": {
        label: "能力不支持",
        tone: "warning",
        alert: "warning",
        guidance: "查看商品或连接能力并转人工，不进行自动重试。",
    },
    "parameter-or-mapping": {
        label: "参数或映射错误",
        tone: "destructive",
        alert: "destructive",
        guidance: "先修复参数或基础资料映射，当前请求不可直接重试。",
    },
    "business-rejected": {
        label: "供应商业务拒绝",
        tone: "destructive",
        alert: "destructive",
        guidance: "保留拒绝记录，并进入退款、恢复或替代履约流程。",
    },
    "network-timeout": {
        label: "网络超时",
        tone: "warning",
        alert: "warning",
        guidance: "先查询原结果；确认无结果后才允许沿用原任务号重试。",
    },
    "result-unknown": {
        label: "结果未知",
        tone: "destructive",
        alert: "destructive",
        guidance: "查询原订单或退款结果；仍未知时转人工并保留风险标记。",
    },
    "authentication-or-signature": {
        label: "鉴权或签名失败",
        tone: "destructive",
        alert: "destructive",
        guidance: "停止自动重试并排查连接配置，不展示或复制密钥正文。",
    },
    "rate-limited": {
        label: "调用次数受限",
        tone: "warning",
        alert: "warning",
        guidance: "请稍后重试，不要高频重复操作。",
    },
    "duplicate-callback": {
        label: "重复通知",
        tone: "neutral",
        alert: "info",
        guidance: interfaceText.duplicateCallbackIgnored,
    },
    "out-of-order-callback": {
        label: "通知顺序异常",
        tone: "warning",
        alert: "warning",
        guidance: "保留当前有效状态，并展示被拒绝的状态变化。",
    },
} satisfies Record<
    InterfaceErrorClass,
    {
        label: string
        tone: StatusTone
        alert: "destructive" | "warning" | "info"
        guidance: string
    }
>

const interfaceErrorStatusPresentation = {
    pending: { label: "待处理", tone: "warning" },
    "auto-retrying": { label: "自动重试中", tone: "info" },
    "manual-required": { label: "待人工", tone: "destructive" },
    resolved: { label: "已解决", tone: "success" },
    closed: { label: "已关闭", tone: "void" },
} satisfies Record<InterfaceErrorStatus, { label: string; tone: StatusTone }>

function ResolutionActionSlot({
    title,
    description,
    action,
}: {
    title: string
    description: string
    action?: React.ReactNode
}) {
    if (action == null) return null

    return (
        <Item variant="muted" size="sm">
            <ItemContent>
                <ItemTitle>{title}</ItemTitle>
                <ItemDescription>{description}</ItemDescription>
            </ItemContent>
            <ItemActions>{action}</ItemActions>
        </Item>
    )
}

function InterfaceErrorResolutionPanel({
    errorClass,
    status,
    businessImpact,
    latestAttempt,
    actions,
    errorCode,
    className,
    ...props
}: InterfaceErrorResolutionPanelProps) {
    const classification = interfaceErrorClassPresentation[errorClass]
    const statusPresentation = interfaceErrorStatusPresentation[status]
    const actionStage = actions?.stage ?? "none"
    const queryOriginalAction =
        actions?.stage === "query-original" ? actions.queryOriginal : undefined
    const retrySameKeyAction =
        actions?.stage === "safe-retry" ? actions.retrySameKey : undefined
    const manualAction =
        actions?.stage === "manual" ? actions.manual : undefined
    const terminalEvidenceCandidate =
        actions?.stage === "closable" ? actions.terminalEvidence : undefined
    const terminalEvidenceIsValid =
        typeof terminalEvidenceCandidate === "string"
            ? terminalEvidenceCandidate.trim().length > 0
            : React.isValidElement(terminalEvidenceCandidate)
    const terminalEvidence = terminalEvidenceIsValid
        ? terminalEvidenceCandidate
        : undefined
    const closeAction =
        actions?.stage === "closable" && terminalEvidenceIsValid
            ? actions.close
            : undefined
    const hasActions = Boolean(
        queryOriginalAction ||
        retrySameKeyAction ||
        manualAction ||
        closeAction,
    )

    return (
        <Card
            data-slot="interface-error-resolution-panel"
            data-error-class={errorClass}
            data-action-stage={actionStage}
            className={className}
            {...props}
        >
            <CardHeader className="border-b border-border">
                <CardTitle>接口错误处理</CardTitle>
                <CardDescription>
                    先确认原请求的处理结果，再决定重试、转人工或关闭任务。
                </CardDescription>
                <CardAction className="flex flex-wrap items-center justify-end gap-2">
                    {errorCode != null ? (
                        <Badge variant="outline">{errorCode}</Badge>
                    ) : null}
                    <StatusBadge
                        tone={classification.tone}
                        label={classification.label}
                    />
                    <StatusBadge
                        tone={statusPresentation.tone}
                        label={statusPresentation.label}
                    />
                </CardAction>
            </CardHeader>

            <CardContent className="space-y-4">
                <DescriptionList columns="three">
                    <DescriptionItem>
                        <DescriptionTerm>业务影响</DescriptionTerm>
                        <DescriptionDetails>
                            {businessImpact}
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>最近尝试</DescriptionTerm>
                        <DescriptionDetails>
                            第{" "}
                            <span className="num">
                                {latestAttempt.attemptNumber}
                            </span>{" "}
                            次 ·{" "}
                            <DomainTime value={latestAttempt.attemptedAt} />
                        </DescriptionDetails>
                    </DescriptionItem>
                    <DescriptionItem>
                        <DescriptionTerm>尝试结果</DescriptionTerm>
                        <DescriptionDetails>
                            {latestAttempt.result}
                        </DescriptionDetails>
                    </DescriptionItem>
                </DescriptionList>

                {latestAttempt.requestSummary != null ||
                latestAttempt.responseSummary != null ||
                latestAttempt.nextRetryAt != null ? (
                    <DescriptionList columns="three">
                        <DescriptionItem>
                            <DescriptionTerm>请求摘要</DescriptionTerm>
                            <DescriptionDetails>
                                {latestAttempt.requestSummary ?? (
                                    <span className="text-muted-foreground">
                                        未提供
                                    </span>
                                )}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>响应摘要</DescriptionTerm>
                            <DescriptionDetails>
                                {latestAttempt.responseSummary ?? (
                                    <span className="text-muted-foreground">
                                        未提供
                                    </span>
                                )}
                            </DescriptionDetails>
                        </DescriptionItem>
                        <DescriptionItem>
                            <DescriptionTerm>下次重试时间</DescriptionTerm>
                            <DescriptionDetails>
                                {latestAttempt.nextRetryAt ? (
                                    <DomainTime
                                        value={latestAttempt.nextRetryAt}
                                    />
                                ) : (
                                    <span className="text-muted-foreground">
                                        未安排
                                    </span>
                                )}
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>
                ) : null}

                <Alert variant={classification.alert}>
                    <AlertTitle>{classification.label}</AlertTitle>
                    <AlertDescription>
                        {classification.guidance}
                    </AlertDescription>
                </Alert>

                {terminalEvidence != null ? (
                    <DescriptionList columns="one">
                        <DescriptionItem>
                            <DescriptionTerm>
                                可关闭任务的完成凭证
                            </DescriptionTerm>
                            <DescriptionDetails>
                                {terminalEvidence}
                            </DescriptionDetails>
                        </DescriptionItem>
                    </DescriptionList>
                ) : null}

                {hasActions ? (
                    <ItemGroup>
                        <ResolutionActionSlot
                            title="查询原结果"
                            description="先确认原订单、取消或退款是否已经被受理。"
                            action={queryOriginalAction}
                        />
                        <ResolutionActionSlot
                            title={resultText.useOriginalTaskNoRetry}
                            description="仅在系统确认原请求无结果且可安全重试时使用。"
                            action={retrySameKeyAction}
                        />
                        <ResolutionActionSlot
                            title="转人工或补偿"
                            description="结果仍未知或外部系统不支持查询时保留风险并转交。"
                            action={manualAction}
                        />
                        <ResolutionActionSlot
                            title="关闭任务"
                            description="仅关闭重复、误派或已有完成凭证的任务，不改变业务记录。"
                            action={closeAction}
                        />
                    </ItemGroup>
                ) : null}
            </CardContent>

            <CardFooter className="border-t border-border text-xs text-muted-foreground">
                本组件不提供“直接标记成功”；成功必须来自可验证的处理结果或已复核的补偿记录。
            </CardFooter>
        </Card>
    )
}

export { InterfaceErrorResolutionPanel }
