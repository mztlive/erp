"use client"

import { detailWorkspaceStyles } from "@/components/business/detail-workspace"
import { withForm } from "@/components/form"
import { Button } from "@/components/ui/button"
import { NativeCheckbox } from "@/components/ui/checkbox"
import { PersonDirectoryFilter } from "@/features/entity-selectors/components/person-directory-filter"
import { timeLabel } from "@/features/supplier-portal/lib/presentation"
import type {
    NewProductInput,
    PortalApplication,
} from "@/features/supplier-portal/types"
import { cn } from "@/lib/utils"
import { fieldHighlightClass, reviewFormBaseOptions } from "./types"

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

                <div className="max-w-xl border-t border-border/60 pt-5">
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
                </div>
            </div>
        )
    },
})
