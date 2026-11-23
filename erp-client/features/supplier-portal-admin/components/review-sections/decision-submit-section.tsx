"use client"

import { TriangleAlertIcon } from "lucide-react"
import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { StatusBadge } from "@/components/ui/status-badge"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { timeLabel } from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
} from "@/features/supplier-portal/types"
import { toAutomationIdSegment } from "@/lib/automation-id"
import { cn } from "@/lib/utils"
import {
    fieldHighlightClass,
    reviewFormBaseOptions,
    type ReviewSectionIssue,
} from "./types"

export const DecisionSubmitSection = withForm({
    ...reviewFormBaseOptions,
    props: {
        application: {} as PortalApplication,
        rawProduct: null as NewProductInput | null,
        initialAvailability: false,
        nowMs: null as number | null,
        reportedConfirmed: false,
        onReportedConfirmedChange: (_checked: boolean) => {},
        canProcess: false,
        canListProcurementPeople: false,
        actorUserId: "",
        disabled: false,
        highlightedFieldId: null as string | null,
        onClearHighlight: () => {},
        allIssues: [] as ReviewSectionIssue[],
        onFocusIssue: (_fieldId: string) => {},
    },
    render: function DecisionSubmitSectionContent({
        form,
        application,
        rawProduct,
        initialAvailability,
        nowMs,
        reportedConfirmed,
        onReportedConfirmedChange,
        canProcess,
        canListProcurementPeople,
        actorUserId,
        disabled,
        highlightedFieldId,
        onClearHighlight,
        allIssues,
        onFocusIssue,
    }) {
        return (
            <div className="max-w-4xl space-y-5">
                {initialAvailability ? (
                    <section className="space-y-3">
                        <h3 className="text-body-compact font-medium text-foreground">
                            初始可供报送时间核对
                        </h3>
                        <p className="text-xs text-muted-foreground">
                            当前未配置自动过期时限；请核对供应商实际报送时间，需要新信息时退回确认。通过审核不会重新报送供应商可供情况。
                        </p>
                        {(rawProduct
                            ? rawProduct.skus.map((sku) => ({
                                  key: sku.row_id,
                                  name: sku.name,
                                  at: sku.reported_at,
                              }))
                            : [
                                  {
                                      key: "quote-reported-at",
                                      name: "本次首次报价",
                                      at: (
                                          application.submitted_snapshot ??
                                          application.input
                                      ).availability_reported_at as number,
                                  },
                              ]
                        ).map((row) => {
                            const reportedMs = new Date(
                                typeof row.at === "number"
                                    ? row.at * 1000
                                    : row.at,
                            ).getTime()
                            const elapsed =
                                nowMs != null && Number.isFinite(reportedMs)
                                    ? Math.floor((nowMs - reportedMs) / 60000)
                                    : null
                            return (
                                <p key={row.key} className="text-body-compact">
                                    {row.name}：
                                    <span className="num">
                                        {timeLabel(row.at)}
                                    </span>
                                    {elapsed != null ? (
                                        <span className="num text-muted-foreground">
                                            {" · "}
                                            {elapsed >= 0
                                                ? elapsed < 60
                                                    ? `距今 ${elapsed} 分钟`
                                                    : `距今 ${Math.floor(elapsed / 60)} 小时`
                                                : "请核对报送时间"}
                                        </span>
                                    ) : null}
                                </p>
                            )
                        })}
                        <div
                            className={fieldHighlightClass(
                                highlightedFieldId,
                                "supplier-portal-review-reported-confirmed",
                            )}
                        >
                            <label
                                htmlFor="supplier-portal-review-reported-confirmed"
                                className="flex items-start gap-2 text-xs"
                            >
                                <NativeCheckbox
                                    id="supplier-portal-review-reported-confirmed"
                                    checked={reportedConfirmed}
                                    disabled={disabled}
                                    onCheckedChange={(checked) => {
                                        onClearHighlight()
                                        onReportedConfirmedChange(checked)
                                    }}
                                />
                                我已逐项核对供应商实际报送时间，并确认本次可供资料可用于采购确认。
                            </label>
                        </div>
                    </section>
                ) : null}

                {canProcess ? (
                    <div
                        className={cn(
                            initialAvailability &&
                                "border-t border-border/60 pt-5",
                            fieldHighlightClass(
                                highlightedFieldId,
                                canListProcurementPeople
                                    ? "supplier-portal-review-maintainer"
                                    : "supplier-portal-review-maintainer-self",
                            ),
                        )}
                    >
                        <form.AppField name="maintainerUserId">
                            {(field) => (
                                <div className="space-y-2">
                                    <p className="text-xs text-muted-foreground">
                                        通过前请指定内部跟进负责人。
                                    </p>
                                    {canListProcurementPeople ? (
                                        <PersonDirectoryFilter
                                            id="supplier-portal-review-maintainer"
                                            category="procurement"
                                            value={field.state.value}
                                            onChange={(value) => {
                                                if (!disabled) {
                                                    onClearHighlight()
                                                    field.handleChange(value)
                                                }
                                            }}
                                            label="建档及供给内部维护人"
                                            selectionMode="single"
                                        />
                                    ) : (
                                        <>
                                            <p className="text-body-compact">
                                                当前暂无可选采购人员列表，可选择由本人跟进或转交他人处理。
                                            </p>
                                            <Button
                                                id="supplier-portal-review-maintainer-self"
                                                type="button"
                                                size="sm"
                                                variant="ghost"
                                                className={
                                                    detailWorkspaceStyles.subtleButton
                                                }
                                                disabled={
                                                    disabled || !actorUserId
                                                }
                                                onClick={() => {
                                                    onClearHighlight()
                                                    field.handleChange(
                                                        actorUserId,
                                                    )
                                                }}
                                            >
                                                {field.state.value ===
                                                actorUserId
                                                    ? "已选择由我维护"
                                                    : "由我维护"}
                                            </Button>
                                        </>
                                    )}
                                </div>
                            )}
                        </form.AppField>
                    </div>
                ) : null}

                <div className="grid gap-5 border-t border-border/60 pt-5 md:grid-cols-2">
                    <div
                        className={fieldHighlightClass(
                            highlightedFieldId,
                            "supplier-portal-review-comment",
                        )}
                    >
                        <form.AppField name="internalNote">
                            {(field) => (
                                <field.TextareaField
                                    id="supplier-portal-review-comment"
                                    required={Boolean(rawProduct)}
                                    label={
                                        rawProduct
                                            ? "内部核对备注（通过必填，仅内部留档）"
                                            : "内部核对备注（仅内部留档）"
                                    }
                                    description="记录核对依据与复用/新建理由，仅采购内部留档，退回时不会发送给供应商。"
                                    placeholder="填写匹配依据或内部处理备忘"
                                    disabled={disabled}
                                    rows={3}
                                />
                            )}
                        </form.AppField>
                    </div>
                    <div
                        className={fieldHighlightClass(
                            highlightedFieldId,
                            "supplier-portal-review-return-reason",
                        )}
                    >
                        <form.AppField name="returnReason">
                            {(field) => (
                                <field.TextareaField
                                    id="supplier-portal-review-return-reason"
                                    label="退回原因（供应商可见，退回必填）"
                                    description="仅在退回供应商修改时发送给供应商，请明确指出需调整的字段或材料。"
                                    placeholder="如需退回，请填写清晰可执行的修改说明"
                                    disabled={disabled}
                                    rows={3}
                                />
                            )}
                        </form.AppField>
                    </div>
                </div>

                {/* 提交前缺失项汇总与一键定位 */}
                {canProcess ? (
                    allIssues.length > 0 ? (
                        <div className="space-y-2 border-l-2 border-destructive-border bg-destructive-soft/45 px-3.5 py-3">
                            <div className="flex flex-wrap items-center gap-2 text-xs font-medium text-destructive-soft-foreground">
                                <TriangleAlertIcon
                                    aria-hidden="true"
                                    className="size-3.5 shrink-0 text-destructive"
                                />
                                <span>
                                    通过前尚有{" "}
                                    <span className="num font-semibold">
                                        {allIssues.length}
                                    </span>{" "}
                                    项必填或核对项待完成（退回供应商不受此限）
                                </span>
                            </div>
                            <ul className="space-y-1">
                                {allIssues.slice(0, 5).map((issue) => (
                                    <li
                                        key={`${issue.fieldId}:${issue.message}`}
                                        className="flex flex-wrap items-center justify-between gap-2 text-xs text-destructive-soft-foreground"
                                    >
                                        <span className="min-w-0 truncate">
                                            · {issue.message}
                                        </span>
                                        <button
                                            id={`supplier-portal-review-issue-jump-${toAutomationIdSegment(issue.fieldId)}`}
                                            type="button"
                                            className="shrink-0 cursor-pointer font-medium underline underline-offset-2 hover:opacity-80"
                                            onClick={() =>
                                                onFocusIssue(issue.fieldId)
                                            }
                                        >
                                            定位
                                        </button>
                                    </li>
                                ))}
                                {allIssues.length > 5 ? (
                                    <li className="text-tiny text-muted-foreground">
                                        另有{" "}
                                        <span className="num">
                                            {allIssues.length - 5}
                                        </span>{" "}
                                        项待补，完成上述条目后自动更新。
                                    </li>
                                ) : null}
                            </ul>
                        </div>
                    ) : (
                        <div className="flex items-center gap-2 border-l-2 border-success-border bg-success-soft/45 px-3.5 py-2.5">
                            <StatusBadge
                                tone="success"
                                appearance="text"
                                label="全部核对项已就绪，可确认通过并生效"
                            />
                        </div>
                    )
                ) : null}
            </div>
        )
    },
})
