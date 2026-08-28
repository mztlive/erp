"use client"

import { OptionCombobox } from "@/components/business"
import { DateTimeRangeLocalPicker } from "@/components/ui/date-picker"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { Textarea } from "@/components/ui/textarea"
import {
    displayText,
    lineItemTitle,
} from "@/features/fulfillment-operations/lib/readable-label"
import type {
    FulfillmentDraft,
    FulfillmentOperation,
    ServiceFulfillmentResultCode,
} from "@/features/fulfillment-operations/types"
import {
    SERVICE_EVIDENCE_PENDING_REFERENCE,
    SERVICE_RESULT_OPTIONS,
} from "@/features/fulfillment-operations/types"

/**
 * 线下服务表单。先选择成功或失败，再登记现场、凭证和完成数量。
 *
 * @param operation 服务履约工作单。
 * @param draft 线下服务草稿。
 * @param onChange 草稿变更回调。
 * @param disabled 只读或提交中禁用。
 */
export function FulfillmentServiceForm({
    operation,
    draft,
    onChange,
    disabled,
}: {
    operation: FulfillmentOperation
    draft: Extract<FulfillmentDraft, { type: "SERVICE" }>
    onChange: (d: FulfillmentDraft) => void
    disabled?: boolean
}) {
    const failed = draft.result === "FAILURE"
    return (
        <div className="space-y-6" aria-label="线下服务表单">
            <section className="space-y-3">
                <h3 className="text-sm font-semibold">服务项目</h3>
                {draft.lines.map((line, i) => {
                    const src = operation.lines.find(
                        (item) =>
                            item.salesOrderLineId === line.salesOrderLineId,
                    )
                    const remaining = displayText(src?.remainingQuantity)
                    const unit = displayText(src?.unitCode)
                    return (
                        <div
                            key={line.salesOrderLineId}
                            className="space-y-3 rounded-lg border border-border bg-muted/20 p-3"
                        >
                            <div className="space-y-0.5">
                                <p className="text-sm font-medium">
                                    {lineItemTitle(src?.itemName, i)}
                                </p>
                                {remaining ? (
                                    <p className="text-xs text-muted-foreground">
                                        还剩{" "}
                                        <span className="num">{remaining}</span>
                                        {unit} 待完成
                                    </p>
                                ) : null}
                            </div>
                            <div className="space-y-1.5">
                                <Label htmlFor={`svc-qty-${i}`}>
                                    本次完成数量
                                </Label>
                                <Input
                                    id={`svc-qty-${i}`}
                                    className="num"
                                    inputMode="decimal"
                                    value={line.quantity}
                                    disabled={disabled}
                                    onChange={(e) => {
                                        const lines = draft.lines.map(
                                            (item, idx) =>
                                                idx === i
                                                    ? {
                                                          ...item,
                                                          quantity:
                                                              e.target.value,
                                                      }
                                                    : item,
                                        )
                                        onChange({ ...draft, lines })
                                    }}
                                />
                            </div>
                        </div>
                    )
                })}
            </section>
            <section className="space-y-3">
                <div className="grid gap-4 sm:grid-cols-2">
                    <div className="space-y-1.5">
                        <Label htmlFor="svc-result">
                            履约结果
                            <span className="text-destructive">*</span>
                        </Label>
                        <OptionCombobox
                            id="svc-result"
                            value={draft.result || null}
                            onValueChange={(next) =>
                                onChange({
                                    ...draft,
                                    result: (next ?? "") as
                                        | ServiceFulfillmentResultCode
                                        | "",
                                })
                            }
                            options={SERVICE_RESULT_OPTIONS}
                            allowClear={false}
                            required
                            disabled={disabled}
                            aria-label="履约结果"
                            placeholder="请选择成功或失败"
                        />
                    </div>
                    <div className="space-y-1.5">
                        <Label htmlFor="service-start">
                            服务时间
                            <span className="text-destructive">*</span>
                        </Label>
                        <DateTimeRangeLocalPicker
                            id="service-start"
                            value={{
                                from: draft.startedAt || undefined,
                                to: draft.endedAt || undefined,
                            }}
                            disabled={disabled}
                            showTimeZone={false}
                            placeholder="选择开始到结束时间"
                            onValueChange={(next) =>
                                onChange({
                                    ...draft,
                                    startedAt: next?.from ?? "",
                                    endedAt: next?.to ?? "",
                                })
                            }
                        />
                    </div>
                    <div className="space-y-1.5 sm:col-span-2">
                        <Label htmlFor="service-loc">
                            服务地点
                            <span className="text-destructive">*</span>
                        </Label>
                        <Input
                            id="service-loc"
                            value={draft.serviceLocation}
                            disabled={disabled}
                            placeholder="客户现场或安装地址"
                            onChange={(e) =>
                                onChange({
                                    ...draft,
                                    serviceLocation: e.target.value,
                                })
                            }
                        />
                    </div>
                </div>
            </section>

            <section className="space-y-3">
                <div className="space-y-1.5" id="service-evidence">
                    <Label>
                        图片凭证
                        <span className="text-destructive">*</span>
                    </Label>
                    <FileUpload
                        accept="image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp"
                        multiple={false}
                        disabled={disabled}
                        density="compact"
                        label="上传现场图片"
                        description="支持 JPG、PNG、WebP，单张不超过 5 MB"
                        previewSelectedImage
                        selectedImageFile={draft.evidenceFile ?? null}
                        onFilesSelected={(files) => {
                            const file = files[0]
                            onChange({
                                ...draft,
                                evidenceFile: file,
                                evidenceAttachmentId: file
                                    ? SERVICE_EVIDENCE_PENDING_REFERENCE
                                    : "",
                            })
                        }}
                        onPreviewRemove={() =>
                            onChange({
                                ...draft,
                                evidenceFile: undefined,
                                evidenceAttachmentId: "",
                            })
                        }
                    />
                </div>
                <div className="space-y-1.5">
                    <Label htmlFor="service-note">
                        完成说明
                        <span className="text-destructive">*</span>
                    </Label>
                    <Textarea
                        id="service-note"
                        value={draft.completionNote}
                        disabled={disabled}
                        rows={3}
                        placeholder={
                            failed
                                ? "例如：客户不在现场，未能完成安装"
                                : "例如：已上门安装并完成现场验收"
                        }
                        onChange={(e) =>
                            onChange({
                                ...draft,
                                completionNote: e.target.value,
                            })
                        }
                    />
                </div>
            </section>
        </div>
    )
}
