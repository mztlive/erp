"use client"

import {
    ChevronDownIcon,
    CircleAlertIcon,
    CircleCheckIcon,
    CircleXIcon,
    InfoIcon,
    TriangleAlertIcon,
} from "lucide-react"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
import {
    Collapsible,
    CollapsibleContent,
    CollapsibleTrigger,
} from "@/components/ui/collapsible"
import { StatusBadge, type StatusTone } from "@/components/ui/status-badge"
import { formatDateTime } from "@/lib/datetime"
import { actionLabel, resourceLabel } from "@/lib/permission-catalog"
import type { AccessCheckInput, AccessCheckView } from "../../api/access-check"

type CheckSummary = {
    title: string
    description: string
    next: string
    variant: "destructive" | "success" | "info" | "warning"
}

/** 仅解释服务端报告；配置检查不能推导成具体单据的访问许可。 */
function summarize(
    input: AccessCheckInput,
    result: AccessCheckView,
): CheckSummary {
    const blocked = result.steps.find((step) => step.status === "blocked")
    if (blocked) {
        return {
            title: "当前不能执行此操作",
            description: blocked.message,
            next: "按上述原因调整后重新检查；修改权限配置前，请先确认此人的工作职责。",
            variant: "destructive",
        }
    }

    const objectPassed = result.steps.some(
        (step) => step.layer === "具体单据范围" && step.status === "passed",
    )
    const reviews = result.steps.filter((step) => step.status === "review")
    if (input.object_id && objectPassed) {
        if (input.action === "detail" && reviews.length === 0) {
            return {
                title: "可以查看这张销售单",
                description: "已核对此人的查看权限和这张单据的数据范围。",
                next: "本结论仅适用于本次选择的销售单，不代表可以查看所有销售单。",
                variant: "success",
            }
        }
        const editable = result.steps.some(
            (step) => step.layer === "单据状态" && step.status === "passed",
        )
        if (
            input.action === "update" &&
            editable &&
            reviews.every((step) => step.layer === "保存内容")
        ) {
            return {
                title: "可以编辑这张销售单的草稿",
                description: "操作权限、单据范围和当前编辑状态已通过检查。",
                next: "保存时还会检查填写的内容、客户、合同和商品；本次检查不保证保存成功。",
                variant: "info",
            }
        }
    }

    const permissionPassed = result.steps.some(
        (step) => step.layer === "操作权限" && step.status === "passed",
    )
    const scopeReview = reviews.find((step) =>
        ["业务数据范围", "人员数据范围", "组织关系"].includes(step.layer),
    )
    if (scopeReview) {
        return {
            title: "数据范围还需核对",
            description: scopeReview.message,
            next: "请先核对人员资料中的数据范围及所属部门，再重新检查。",
            variant: "warning",
        }
    }

    if (permissionPassed && reviews.length > 0) {
        return {
            title: "有操作权限，具体业务能否处理尚未确认",
            description: reviews[0].message,
            next: "请按待检查项目核对具体单据和任务；只有实际业务中的完整检查通过后，才能执行对应操作。",
            variant: "info",
        }
    }

    if (permissionPassed && !input.object_id) {
        const canCheckOrder =
            input.resource === "sales_order" &&
            ["detail", "update"].includes(input.action)
        return {
            title: "已具备功能权限（视具体单据归属生效）",
            description:
                "已确认具备该项操作权限；实际办理时还需符合具体单据的归属部门、负责人及单据状态。",
            next: canCheckOrder
                ? "在上方选择一张销售单，再点「开始检查」，即可核对此人对这张单据的权限。"
                : "此操作目前只支持配置检查；请在对应业务中核对具体对象及操作条件。",
            variant: "info",
        }
    }

    return {
        title: "暂不能确认是否可以执行",
        description: "当前检查尚未提供完整结论，请查看检查明细中的待核对项目。",
        next: "请按检查明细补齐条件，再重新检查。",
        variant: "warning",
    }
}

function stepMeta(step: AccessCheckView["steps"][number]): {
    label: string
    tone: StatusTone
} {
    if (step.status === "blocked") {
        return { label: "未通过", tone: "destructive" }
    }
    if (step.status === "review") {
        const label = [
            "业务条件",
            "对象上下文",
            "对象资格",
            "保存内容",
        ].includes(step.layer)
            ? "本次未检查"
            : "待核对"
        return { label, tone: "warning" }
    }
    return {
        label: ["业务数据范围", "人员数据范围"].includes(step.layer)
            ? "已读取规则"
            : "已通过",
        tone: "success",
    }
}

function SummaryIcon({ variant }: { variant: CheckSummary["variant"] }) {
    if (variant === "success") return <CircleCheckIcon aria-hidden="true" />
    if (variant === "destructive") return <CircleXIcon aria-hidden="true" />
    if (variant === "warning") return <TriangleAlertIcon aria-hidden="true" />
    return <InfoIcon aria-hidden="true" />
}

function StepStatusIcon({
    status,
}: {
    status: AccessCheckView["steps"][number]["status"]
}) {
    if (status === "blocked") {
        return (
            <CircleXIcon
                className="mt-0.5 size-4 shrink-0 text-destructive"
                aria-hidden="true"
            />
        )
    }
    if (status === "review") {
        return (
            <CircleAlertIcon
                className="mt-0.5 size-4 shrink-0 text-warning"
                aria-hidden="true"
            />
        )
    }
    return (
        <CircleCheckIcon
            className="mt-0.5 size-4 shrink-0 text-success"
            aria-hidden="true"
        />
    )
}

export function AccessCheckResult({
    input,
    result,
}: {
    input: AccessCheckInput
    result: AccessCheckView
}) {
    const summary = summarize(input, result)
    const reviews = result.steps.filter((step) => step.status === "review")
    const hasBlocked = result.steps.some((step) => step.status === "blocked")

    return (
        <section
            className="space-y-4 border-t border-border/60 pt-5"
            aria-label="访问检查结果"
        >
            <div className="flex flex-wrap items-center justify-between gap-2">
                <div className="flex flex-wrap items-center gap-2">
                    <h3 className="font-semibold text-foreground">检查结论</h3>
                    <Badge variant="outline">
                        {input.object_id ? "已选择具体单据" : "仅检查权限配置"}
                    </Badge>
                    {reviews.length > 0 ? (
                        <Badge variant="warning" className="num">
                            {reviews.length} 项待核对
                        </Badge>
                    ) : null}
                    <span className="text-xs text-muted-foreground">
                        {resourceLabel(input.resource)} ·{" "}
                        {actionLabel(input.action)}
                    </span>
                </div>
                {result.checked_at ? (
                    <span className="num text-xs text-muted-foreground">
                        检查于 {formatDateTime(result.checked_at, "full")}
                    </span>
                ) : null}
            </div>

            <Alert variant={summary.variant}>
                <SummaryIcon variant={summary.variant} />
                <AlertTitle>{summary.title}</AlertTitle>
                <AlertDescription>{summary.description}</AlertDescription>
            </Alert>

            <div className="rounded-lg border border-border/70 bg-surface-sunken/70 px-4 py-3 text-body-compact leading-relaxed">
                <span className="font-medium text-foreground">
                    {summary.variant === "success" ? "适用范围：" : "下一步："}
                </span>
                <span className="text-foreground/90">{summary.next}</span>
            </div>

            {reviews.length > 0 ? (
                <div className="space-y-2">
                    <h4 className="text-xs font-medium text-muted-foreground">
                        待核对项目
                    </h4>
                    <ul className="divide-y divide-border/60 border-y border-border/60">
                        {reviews.map((step, index) => (
                            <li
                                key={`${step.layer}-${index}`}
                                className="flex items-start gap-2.5 py-2.5"
                            >
                                <CircleAlertIcon
                                    className="mt-0.5 size-3.5 shrink-0 text-warning"
                                    aria-hidden="true"
                                />
                                <p className="min-w-0 text-body-compact text-foreground/90">
                                    <span className="font-medium text-foreground">
                                        {step.layer}：
                                    </span>
                                    {step.message}
                                </p>
                            </li>
                        ))}
                    </ul>
                </div>
            ) : null}

            <Collapsible
                defaultOpen={hasBlocked}
                className="border-t border-border/60 pt-3"
            >
                <CollapsibleTrigger
                    id="account-check-result-details"
                    className="group flex w-fit cursor-pointer items-center gap-1.5 rounded-sm py-1 text-body-compact font-medium text-muted-foreground transition-colors hover:text-foreground focus-visible:outline-2 focus-visible:outline-primary"
                >
                    <ChevronDownIcon
                        className="size-4 transition-transform group-aria-expanded:rotate-180"
                        aria-hidden="true"
                    />
                    <span>逐层检查明细</span>
                    <span className="num text-xs font-normal">
                        （{result.steps.length} 项）
                    </span>
                </CollapsibleTrigger>
                <CollapsibleContent>
                    <ol className="mt-3 divide-y divide-border/60 border-y border-border/60">
                        {result.steps.map((step, index) => {
                            const meta = stepMeta(step)
                            return (
                                <li
                                    key={`${step.layer}-${index}`}
                                    className="flex items-start gap-3 py-3"
                                >
                                    <StepStatusIcon status={step.status} />
                                    <div className="min-w-0 flex-1 space-y-1">
                                        <div className="flex flex-wrap items-center gap-2">
                                            <span className="font-medium text-foreground">
                                                {step.layer}
                                            </span>
                                            <StatusBadge
                                                tone={meta.tone}
                                                label={meta.label}
                                                appearance={
                                                    step.status === "passed"
                                                        ? "quiet"
                                                        : "badge"
                                                }
                                            />
                                        </div>
                                        <p className="text-xs text-muted-foreground">
                                            {step.message}
                                        </p>
                                    </div>
                                </li>
                            )
                        })}
                    </ol>
                </CollapsibleContent>
            </Collapsible>

            <p className="text-xs text-muted-foreground">
                结果以本次检查为准；权限、责任人或单据状态变化后，请重新检查。
            </p>
        </section>
    )
}
