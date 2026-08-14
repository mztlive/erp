import * as React from "react"

import { z } from "zod"

import {
    BusinessEmptyState,
    BusinessFailureState,
} from "@/components/business"
import { useAppForm } from "@/components/form"
import {
    useBlockedApprovalsQuery,
    useRecoverApprovalMutation,
    type BlockedApprovalView,
} from "@/features/work-items"
import { Button } from "@/components/ui/button"
import {
    Card,
    CardContent,
    CardDescription,
    CardHeader,
    CardTitle,
} from "@/components/ui/card"
import { cn } from "@/lib/utils"

import { createIdempotencyKey } from "../lib/idempotency"

export function BlockedApprovalList({ canRecover }: { canRecover: boolean }) {
    const query = useBlockedApprovalsQuery(canRecover)
    const recover = useRecoverApprovalMutation()
    const form = useAppForm({
        defaultValues: { reason: "" },
        validators: {
            onChange: z.object({
                reason: z
                    .string()
                    .trim()
                    .min(1, "请填写恢复原因")
                    .max(500, "原因不超过 500 字"),
            }),
        },
        onSubmit: async () => undefined,
    })
    const [selected, setSelected] = React.useState<
        BlockedApprovalView | undefined
    >()
    const idempotencyKeys = React.useRef(new Map<string, string>())

    if (!canRecover) {
        return (
            <BusinessFailureState
                kind="permission"
                title="无权恢复受阻审批"
                description="请联系具备审批恢复权限的系统管理员。"
            />
        )
    }

    if (query.isPending) {
        return <div className="h-56 animate-pulse rounded-lg bg-muted" />
    }
    if (query.isError) {
        return (
            <BusinessFailureState
                error={query.error}
                onRetry={() => void query.refetch()}
            />
        )
    }
    if (!query.data?.items.length) {
        return (
            <BusinessEmptyState
                kind="no-exceptions"
                title="当前没有受阻审批"
                description="需要管理员重试的审批会显示在这里。"
            />
        )
    }

    return (
        <div className="grid gap-4 lg:grid-cols-[minmax(0,2fr)_minmax(18rem,1fr)]">
            <div className="space-y-2">
                {query.data.items.map((approval) => (
                    <button
                        key={approval.approvalInstanceId}
                        type="button"
                        className={cn(
                            "w-full rounded-lg border p-4 text-left",
                            selected?.approvalInstanceId ===
                                approval.approvalInstanceId
                                ? "border-primary bg-primary/5"
                                : "border-border bg-card",
                        )}
                        onClick={() => {
                            setSelected(approval)
                            form.reset()
                        }}
                    >
                        <p className="font-medium">
                            {approval.businessObjectLabel}
                        </p>
                        <p className="mt-1 text-sm text-muted-foreground">
                            {approval.blockerMessage}
                        </p>
                    </button>
                ))}
            </div>
            <Card>
                <CardHeader>
                    <CardTitle>重试当前步骤</CardTitle>
                    <CardDescription>
                        只重新执行当前步骤，不指定处理人，也不跳过审批。
                    </CardDescription>
                </CardHeader>
                <CardContent>
                    {selected ? (
                        <form
                            className="space-y-4"
                            onSubmit={(event) => {
                                event.preventDefault()
                                void form.handleSubmit()
                            }}
                        >
                            <form.AppField
                                name="reason"
                                children={(field) => (
                                    <field.TextareaField
                                        label="重试原因"
                                        rows={4}
                                    />
                                )}
                            />
                            {recover.isError ? (
                                <BusinessFailureState
                                    error={recover.error}
                                    title="当前步骤尚未恢复"
                                />
                            ) : null}
                            <form.Subscribe
                                selector={(state) =>
                                    [
                                        state.canSubmit,
                                        state.values.reason,
                                    ] as const
                                }
                            >
                                {([canSubmit, reason]) => (
                                    <Button
                                        type="button"
                                        disabled={
                                            !canSubmit ||
                                            recover.isPending ||
                                            !selected.allowedActions.includes(
                                                "RETRY_CURRENT_STEP",
                                            )
                                        }
                                        onClick={() => {
                                            const workItem = selected.workItem
                                            const key =
                                                idempotencyKeys.current.get(
                                                    selected.approvalInstanceId,
                                                ) ??
                                                createIdempotencyKey(
                                                    selected.approvalInstanceId,
                                                    "recover",
                                                )
                                            idempotencyKeys.current.set(
                                                selected.approvalInstanceId,
                                                key,
                                            )
                                            void recover
                                                .mutateAsync({
                                                    approvalInstanceId:
                                                        selected.approvalInstanceId,
                                                    currentStepInstanceId:
                                                        selected.currentStepInstanceId,
                                                    expectedInstanceVersion:
                                                        selected.instanceVersion,
                                                    expectedStepVersion:
                                                        selected.stepVersion,
                                                    expectedTaskVersion:
                                                        workItem?.taskVersion,
                                                    recoveryAction:
                                                        "RETRY_CURRENT_STEP",
                                                    reason,
                                                    idempotencyKey: key,
                                                })
                                                .then(() => {
                                                    idempotencyKeys.current.delete(
                                                        selected.approvalInstanceId,
                                                    )
                                                })
                                                .catch(() => undefined)
                                        }}
                                    >
                                        {recover.isPending
                                            ? "正在重试"
                                            : "重试当前步骤"}
                                    </Button>
                                )}
                            </form.Subscribe>
                        </form>
                    ) : (
                        <p className="text-sm text-muted-foreground">
                            请选择一项受阻审批。
                        </p>
                    )}
                </CardContent>
            </Card>
        </div>
    )
}
