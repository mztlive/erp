"use client"

import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Badge } from "@/components/ui/badge"
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

    const contextual = result.steps.find((step) => step.layer === "业务授权")
    if (permissionPassed && contextual) {
        return {
            title: "有操作权限，具体业务能否处理尚未确认",
            description: contextual.message,
            next: "请进入对应业务或待办，核对来源单据和当前处理人。本页未检查具体任务的处理资格。",
            variant: "info",
        }
    }

    if (permissionPassed && !input.object_id) {
        const canCheckOrder =
            input.resource === "sales_order" &&
            ["detail", "update"].includes(input.action)
        return {
            title: "有操作权限，尚未检查具体单据",
            description:
                "本次只检查了权限配置，不能据此判断某张单据是否在此人的可操作范围内。",
            next: canCheckOrder
                ? "在上方选择一张销售单，再点“开始检查”，即可核对此人对这张单据的权限。"
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

function stepStatus(step: AccessCheckView["steps"][number]) {
    if (step.status === "blocked") return "未通过"
    if (step.status === "review") {
        return ["业务条件", "对象上下文", "保存内容"].includes(step.layer)
            ? "本次未检查"
            : "待核对"
    }
    return ["业务数据范围", "人员数据范围"].includes(step.layer)
        ? "已读取规则"
        : "已通过"
}

export function AccessCheckResult({
    input,
    result,
}: {
    input: AccessCheckInput
    result: AccessCheckView
}) {
    const summary = summarize(input, result)
    return (
        <section
            className="space-y-4 rounded-xl border p-4"
            aria-label="访问检查结果"
        >
            <div className="flex flex-wrap items-center gap-2">
                <h3 className="font-semibold">检查结果</h3>
                <Badge variant="outline">
                    {input.object_id ? "已选择具体单据" : "仅检查权限配置"}
                </Badge>
                <span className="text-sm text-muted-foreground">
                    {resourceLabel(input.resource)} ·{" "}
                    {actionLabel(input.action)}
                </span>
            </div>
            <Alert variant={summary.variant}>
                <AlertTitle>{summary.title}</AlertTitle>
                <AlertDescription>{summary.description}</AlertDescription>
            </Alert>
            <p className="text-sm leading-relaxed">
                <span className="font-medium">
                    {summary.variant === "success" ? "适用范围：" : "下一步："}
                </span>
                {summary.next}
            </p>
            <details className="border-t pt-3">
                <summary
                    id="account-check-result-details"
                    className="w-fit cursor-pointer rounded-sm text-sm text-muted-foreground focus-visible:outline-2 focus-visible:outline-ring"
                >
                    查看检查明细
                </summary>
                <div className="mt-3 space-y-3">
                    {result.steps.map((step, index) => (
                        <div
                            key={`${step.layer}-${index}`}
                            className="border-l-2 pl-3"
                        >
                            <p className="font-medium">
                                {step.layer} · {stepStatus(step)}
                            </p>
                            <p className="mt-1 text-sm text-muted-foreground">
                                {step.message}
                            </p>
                        </div>
                    ))}
                </div>
            </details>
            <p className="text-xs text-muted-foreground">
                结果以本次检查为准；权限、责任人或单据状态变化后，请重新检查。
            </p>
        </section>
    )
}
