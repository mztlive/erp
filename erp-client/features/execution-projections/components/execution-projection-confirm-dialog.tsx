"use client"

import { FormalActionConfirmDialog } from "@/components/business"
import type { ExecutionProjectionRow } from "@/features/execution-projections/types"

export type PendingAction =
    | {
          kind: "QUERY_RESULT"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "RETRY"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "ESCALATE"
          row: ExecutionProjectionRow
          objectVersion: string
      }
    | {
          kind: "BULK_QUERY"
          ids: string[]
      }
    | {
          kind: "BULK_RETRY"
          ids: string[]
      }
    | null

export function ExecutionProjectionConfirmDialog({
    pendingAction,
    onOpenChange,
    pending,
    onConfirm,
}: {
    pendingAction: PendingAction
    onOpenChange: (open: boolean) => void
    pending: boolean
    onConfirm: () => void | Promise<void>
}) {
    return (
        <FormalActionConfirmDialog
            id="execution-projections-confirm"
            open={pendingAction != null}
            onOpenChange={(open) => {
                if (!open) onOpenChange(false)
            }}
            title={
                pendingAction?.kind === "QUERY_RESULT"
                    ? "查询最终结果"
                    : pendingAction?.kind === "RETRY"
                      ? "重试发送"
                      : pendingAction?.kind === "ESCALATE"
                        ? "升级到接口错误中心"
                        : pendingAction?.kind === "BULK_QUERY"
                          ? "批量查询"
                          : pendingAction?.kind === "BULK_RETRY"
                            ? "批量重试"
                            : "确认操作"
            }
            actionLabel="执行"
            confirmLabel="确认执行"
            cancelLabel="取消"
            fromStatus={
                pendingAction && "row" in pendingAction && pendingAction.row
                    ? {
                          label: pendingAction.row.delivery.statusLabel,
                          tone: pendingAction.row.delivery.statusTone,
                      }
                    : { label: "当前选择", tone: "neutral" }
            }
            toStatus={
                pendingAction?.kind === "QUERY_RESULT"
                    ? { label: "明确结果或仍未知", tone: "warning" }
                    : pendingAction?.kind === "RETRY" ||
                        pendingAction?.kind === "BULK_RETRY"
                      ? { label: "按原记录重试", tone: "info" }
                      : pendingAction?.kind === "ESCALATE"
                        ? { label: "错误中心待办", tone: "warning" }
                        : { label: "后台逐项处理", tone: "info" }
            }
            lockedFields={
                pendingAction && "row" in pendingAction && pendingAction.row
                    ? [
                          `销售版本 v${pendingAction.row.salesOrderRevisionNo}`,
                          `数据版本 v${pendingAction.row.projectionRevisionNo}`,
                          pendingAction.row.targetMallName,
                          `销售单 ${pendingAction.row.salesOrderNo} · v${pendingAction.row.salesOrderRevisionNo} · ${pendingAction.row.targetMallName}`,
                      ]
                    : pendingAction && "ids" in pendingAction
                      ? [
                            `显式选择 ${pendingAction.ids.length} 项`,
                            "系统筛选结果（非当前筛选全部）",
                        ]
                      : []
            }
            effects={
                pendingAction?.kind === "QUERY_RESULT"
                    ? [
                          "未明确前不显示成功",
                          "不跳过、不计入已确认指标",
                          "超时可再次查询或升级到接口错误中心",
                      ]
                    : pendingAction?.kind === "RETRY"
                      ? [
                            "沿原数据修订继续发送",
                            "不生成新数据修订",
                            "不回退销售记录或应收",
                        ]
                      : pendingAction?.kind === "ESCALATE"
                        ? [
                              "创建或复用接口错误待办（不会重复建单）",
                              "本页只返回入口，不建立处理责任或完成任务",
                          ]
                        : pendingAction?.kind === "BULK_RETRY"
                          ? [
                                "系统按筛选结果逐项核对",
                                "已确认/结果未知/权限变化项跳过",
                                "展示成功/跳过/失败/仍未知",
                            ]
                          : ["系统按筛选结果逐项查询", "仍未知不按成功处理"]
            }
            nextDepartment={
                pendingAction?.kind === "ESCALATE"
                    ? "接口错误中心"
                    : "运营 / 系统"
            }
            pending={pending}
            onConfirm={onConfirm}
        />
    )
}
