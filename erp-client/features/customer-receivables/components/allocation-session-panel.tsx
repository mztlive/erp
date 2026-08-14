"use client"

import Link from "next/link"
import { SaveIcon } from "lucide-react"

import {
    AllocationWorkspace,
    DiscardConfirmDialog,
    FormalActionConfirmDialog,
    FormalActionResult,
    MoneyValue,
    ValidationSummary,
} from "@/components/business"
import { Alert, AlertDescription, AlertTitle } from "@/components/ui/alert"
import { Button } from "@/components/ui/button"
import { Input } from "@/components/ui/input"
import { SessionFactFields } from "@/features/customer-receivables/components/session-fact-fields"
import { SessionHeader } from "@/features/customer-receivables/components/session-header"
import { SessionPool } from "@/features/customer-receivables/components/session-pool"
import { SessionRemoveLineDialog } from "@/features/customer-receivables/components/session-remove-line-dialog"
import { useAllocationSession } from "@/features/customer-receivables/hooks/use-allocation-session"
import { money } from "@/features/customer-receivables/lib/allocation-math"
import type { AllocationSessionView } from "@/features/customer-receivables/types"

export function AllocationSessionPanel({
    session,
    onClose,
    onPosted,
}: {
    session: AllocationSessionView
    onClose: () => void
    onPosted: () => void
}) {
    const {
        form,
        isReceipt,
        existing,
        locked,
        allocations,
        draftSavedAt,
        postedLocally,
        result,
        actionError,
        confirmOpen,
        setConfirmOpen,
        leaveConfirmOpen,
        setLeaveConfirmOpen,
        pendingRemove,
        setPendingRemove,
        issues,
        canSubmit,
        factAmountStr,
        proposedAllocated,
        proposedUnallocated,
        addFromPool,
        updateAmount,
        removeLine,
        fillLineAmount,
        requestClose,
        doSaveDraft,
        doPost,
        resolveUnknown,
        saveMutation,
        postMutation,
        resolveMutation,
    } = useAllocationSession({ session, onClose, onPosted })

    return (
        <div className="flex flex-col gap-4">
            <SessionHeader
                session={session}
                isReceipt={isReceipt}
                existing={existing}
                draftSavedAt={draftSavedAt}
                onRequestClose={requestClose}
            />

            {result ? (
                <FormalActionResult
                    status={
                        result.status === "failed" ? "rejected" : result.status
                    }
                    title={result.title}
                    description={result.description}
                    reference={result.reference}
                    facts={result.facts}
                    actions={
                        <>
                            {result.pendingKey ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    onClick={() => void resolveUnknown()}
                                    disabled={resolveMutation.isPending}
                                >
                                    查询最终结果
                                </Button>
                            ) : null}
                            {result.returnTo ? (
                                <Button
                                    type="button"
                                    size="sm"
                                    render={<Link href={result.returnTo} />}
                                >
                                    返回销售单
                                </Button>
                            ) : null}
                            <Button
                                type="button"
                                size="sm"
                                variant="outline"
                                onClick={onClose}
                            >
                                返回列表
                            </Button>
                        </>
                    }
                />
            ) : null}

            {actionError ? (
                <Alert variant="destructive">
                    <AlertTitle>操作未成功</AlertTitle>
                    <AlertDescription>{actionError}</AlertDescription>
                </Alert>
            ) : null}

            <div className="grid gap-4 lg:grid-cols-2">
                <SessionFactFields
                    form={form}
                    isReceipt={isReceipt}
                    existing={existing}
                    locked={locked}
                />
                <SessionPool
                    session={session}
                    allocations={allocations}
                    disabled={
                        session.status === "posted" || postedLocally
                    }
                    onAdd={addFromPool}
                />
            </div>

            <AllocationWorkspace
                title="本次分配"
                description="拟分配金额仅供参考，以提交后结果为准。"
                summary={{
                    totalToAllocate: (
                        <MoneyValue
                            value={factAmountStr || "0"}
                            taxBasis="gross"
                        />
                    ),
                    allocated: (
                        <span>
                            <MoneyValue
                                value={money(proposedAllocated)}
                                taxBasis="gross"
                            />
                            <span className="ml-1 text-xs text-muted-foreground">
                                拟
                            </span>
                        </span>
                    ),
                    difference: (
                        <span>
                            <MoneyValue
                                value={money(proposedUnallocated)}
                                taxBasis="gross"
                            />
                            <span className="ml-1 text-xs text-muted-foreground">
                                拟未分配
                            </span>
                        </span>
                    ),
                }}
                allocations={allocations}
                getRowId={(a) => a.lineKey}
                disabled={session.status === "posted" || postedLocally}
                addLabel="从池中选择"
                addDisabledReason="请从左侧同主体池加入目标"
                onRemoveAllocation={(a) => setPendingRemove(a.lineKey)}
                columns={[
                    {
                        id: "target",
                        header: "目标",
                        renderValue: ({ item }) => (
                            <div>
                                <div className="text-sm">{item.label}</div>
                                <div className="num text-xs text-muted-foreground">
                                    {item.salesOrderNo}
                                </div>
                            </div>
                        ),
                    },
                    {
                        id: "open",
                        header: "开放余额",
                        align: "end",
                        numeric: true,
                        renderValue: ({ item }) => (
                            <MoneyValue
                                value={item.openAmount}
                                taxBasis="gross"
                            />
                        ),
                    },
                    {
                        id: "amount",
                        header: "分配金额",
                        align: "end",
                        numeric: true,
                        renderValue: ({ item }) => (
                            <MoneyValue value={item.amount || "0"} />
                        ),
                        renderEditor: ({ item }) => (
                            <div className="flex items-center justify-end gap-1">
                                <Input
                                    className="num text-right"
                                    value={item.amount}
                                    inputMode="decimal"
                                    aria-label={`${item.label} 分配金额`}
                                    onChange={(e) =>
                                        updateAmount(
                                            item.lineKey,
                                            e.target.value,
                                        )
                                    }
                                />
                                <Button
                                    type="button"
                                    size="xs"
                                    variant="ghost"
                                    onClick={() => fillLineAmount(item)}
                                >
                                    填满
                                </Button>
                            </div>
                        ),
                    },
                ]}
                statusNotice={
                    issues.length > 0 ? (
                        <ValidationSummary issues={issues} title="分配校验" />
                    ) : (
                        <p className="text-xs text-muted-foreground">
                            {session.submitPolicy.label}
                        </p>
                    )
                }
                actions={
                    <>
                        <Button
                            type="button"
                            variant="outline"
                            disabled={
                                saveMutation.isPending ||
                                session.status === "posted" ||
                                postedLocally
                            }
                            onClick={() => void doSaveDraft()}
                        >
                            <SaveIcon
                                data-icon="inline-start"
                                aria-hidden="true"
                            />
                            保存草稿
                        </Button>
                        <Button
                            type="button"
                            disabled={
                                !canSubmit ||
                                postMutation.isPending ||
                                postedLocally
                            }
                            onClick={() => {
                                void form.handleSubmit()
                            }}
                        >
                            确认登记并核销
                        </Button>
                    </>
                }
            />

            {/* 离开前未保存草稿确认 */}
            <DiscardConfirmDialog
                open={leaveConfirmOpen}
                onOpenChange={setLeaveConfirmOpen}
                title="本次核销尚未保存草稿，确定离开？"
                description="记录表单与分配金额尚未保存，离开后将丢失；可先「保存草稿」再离开。"
                confirmLabel="放弃并离开"
                cancelLabel="继续编辑"
                onConfirm={() => {
                    setLeaveConfirmOpen(false)
                    onClose()
                }}
            />

            {/* 移除分配行确认 */}
            <SessionRemoveLineDialog
                pendingRemove={pendingRemove}
                onOpenChange={(open) => {
                    if (!open) setPendingRemove(null)
                }}
                onConfirmRemove={removeLine}
            />

            <FormalActionConfirmDialog
                open={confirmOpen}
                onOpenChange={setConfirmOpen}
                title={
                    isReceipt ? "确认登记回款并核销" : "确认登记销项发票并分配"
                }
                actionLabel="提交"
                confirmLabel="确认提交"
                fromStatus={{ label: "本次草稿", tone: "warning" }}
                toStatus={{
                    label: isReceipt ? "已确认回款" : "已登记发票",
                    tone: "success",
                }}
                lockedFields={["往来主体", "记录编号（提交后）", "既有分配行"]}
                effects={[
                    "形成回款/发票记录与追加式分配明细",
                    "同步更新应收开放余额与净分配（系统）",
                    "未分配余额按系统策略保留并可见",
                    "重复提交不会重复生成记录",
                ]}
                nextDepartment="财务"
                onConfirm={() => void doPost()}
            />
        </div>
    )
}
