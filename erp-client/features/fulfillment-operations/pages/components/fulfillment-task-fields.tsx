"use client"

import { OptionCombobox, type ValidationIssue } from "@/components/business"
import {
    DateTimeLocalPicker,
    DateTimeRangeLocalPicker,
} from "@/components/ui/date-picker"
import { ReasonTextareaField } from "@/components/business/reason-textarea-field"
import { FileUpload } from "@/components/ui/file-upload"
import { Input } from "@/components/ui/input"
import { Label } from "@/components/ui/label"
import { RequiredMark } from "@/components/ui/required-mark"
import { utf8ByteLength } from "@/lib/validation/reason-length"
import { fileUploadText } from "@/lib/ui-text"
import {
    RESULT_OPTIONS,
    SERVICE_EVIDENCE_PENDING_REFERENCE,
    SERVICE_RESULT_OPTIONS,
    type FulfillmentDraft,
    type FulfillmentOperation,
    type FulfillmentResultCode,
    type ServiceFulfillmentResultCode,
} from "../../types"

const SERVICE_NOTE_ID = "fulfillment-operations-service-form-completion-note"
const SERVICE_NOTE_MAX_BYTES = 512

export function serviceCompletionNoteIssue(
    draft: FulfillmentDraft | null,
): ValidationIssue | undefined {
    if (
        draft?.type !== "SERVICE" ||
        utf8ByteLength(draft.completionNote.trim()) <= SERVICE_NOTE_MAX_BYTES
    )
        return undefined
    return {
        id: "service-completion-note-limit",
        label: "完成说明",
        message: `最多 ${SERVICE_NOTE_MAX_BYTES} 字节`,
        targetId: SERVICE_NOTE_ID,
    }
}

export function FulfillmentTaskFields({
    draft,
    disabled,
    onChange,
}: {
    operation: FulfillmentOperation
    draft: FulfillmentDraft
    disabled: boolean
    onChange: (draft: FulfillmentDraft) => void
}) {
    if (draft.type === "RECEIPT") {
        return (
            <section
                className="space-y-2 border-t border-border/60 pt-3"
                aria-label="入库登记信息"
            >
                <div className="grid gap-3 @[480px]/fulfillment-task:grid-cols-2">
                    <div className="min-w-0 space-y-1.5">
                        <Label htmlFor="fulfillment-operations-receipt-form-occurred-at">
                            入库时间 <RequiredMark />
                        </Label>
                        <DateTimeLocalPicker
                            id="fulfillment-operations-receipt-form-occurred-at"
                            value={draft.occurredAt || undefined}
                            disabled={disabled}
                            showTimeZone={false}
                            onValueChange={(next) =>
                                onChange({ ...draft, occurredAt: next ?? "" })
                            }
                        />
                    </div>
                </div>
                <p className="text-xs text-muted-foreground">
                    只有合格的货入库并留给对应的销售单；不合格的不进库存。
                </p>
            </section>
        )
    }

    if (draft.type === "ELECTRONIC") {
        const prefix = "fulfillment-operations-electronic-form"
        return (
            <section
                className="space-y-3 border-t border-border/60 pt-3"
                aria-label="电子交付表单"
            >
                <p className="text-xs text-muted-foreground">
                    填写接收人或交付渠道并上传交付凭证，请勿填写卡号卡密。确认后通过冲正更正。
                </p>
                <div className="grid gap-3 @[480px]/fulfillment-task:grid-cols-2">
                    <div className="min-w-0 space-y-1.5">
                        <Label htmlFor={`${prefix}-recipient`}>
                            交付对象 <RequiredMark />
                        </Label>
                        <Input
                            id={`${prefix}-recipient`}
                            value={draft.recipientMasked}
                            disabled={disabled}
                            onChange={(event) =>
                                onChange({
                                    ...draft,
                                    recipientMasked: event.target.value,
                                })
                            }
                        />
                    </div>
                    <div className="min-w-0 space-y-1.5">
                        <Label htmlFor={`${prefix}-occurred-at`}>
                            实际时间 <RequiredMark />
                        </Label>
                        <DateTimeLocalPicker
                            id={`${prefix}-occurred-at`}
                            value={draft.occurredAt || undefined}
                            disabled={disabled}
                            showTimeZone={false}
                            onValueChange={(next) =>
                                onChange({ ...draft, occurredAt: next ?? "" })
                            }
                        />
                    </div>
                    <div className="min-w-0 space-y-1.5">
                        <Label htmlFor={`${prefix}-result`}>
                            履约结果 <RequiredMark />
                        </Label>
                        <OptionCombobox
                            id={`${prefix}-result`}
                            value={draft.result}
                            onValueChange={(value) =>
                                onChange({
                                    ...draft,
                                    result: (value ??
                                        draft.result) as FulfillmentResultCode,
                                })
                            }
                            options={RESULT_OPTIONS}
                            allowClear={false}
                            disabled={disabled}
                            aria-label="履约结果"
                            placeholder="请选择履约结果"
                        />
                    </div>
                </div>
                <FileUpload
                    maxFileSize={5 * 1024 * 1024}
                    selectedDescription={fileUploadText.pendingSubmit}
                    idPrefix={`${prefix}-evidence`}
                    accept="image/jpeg,image/png,image/webp,.jpg,.jpeg,.png,.webp"
                    multiple={false}
                    disabled={disabled}
                    density="compact"
                    label="上传交付凭证"
                    description="上传已脱敏的交付记录图片，支持 JPG、PNG、WebP"
                    previewSelectedImage
                    selectedImageFile={draft.evidenceFile ?? null}
                    onFilesSelected={(files) =>
                        onChange({ ...draft, evidenceFile: files[0] })
                    }
                    onPreviewRemove={() =>
                        onChange({ ...draft, evidenceFile: undefined })
                    }
                />
            </section>
        )
    }

    if (draft.type !== "SERVICE") return null
    const prefix = "fulfillment-operations-service-form"
    const issue = serviceCompletionNoteIssue(draft)

    return (
        <section
            className="space-y-3 border-t border-border/60 pt-3"
            aria-label="服务登记"
        >
            <div className="grid gap-3 @[480px]/fulfillment-task:grid-cols-2">
                <div className="min-w-0 space-y-1.5">
                    <Label htmlFor={`${prefix}-result`}>
                        履约结果 <RequiredMark />
                    </Label>
                    <OptionCombobox
                        id={`${prefix}-result`}
                        value={draft.result || null}
                        options={SERVICE_RESULT_OPTIONS}
                        disabled={disabled}
                        allowClear={false}
                        required
                        placeholder="请选择成功或失败"
                        aria-label="履约结果"
                        onValueChange={(value) =>
                            onChange({
                                ...draft,
                                result: (value ?? "") as
                                    | ServiceFulfillmentResultCode
                                    | "",
                            })
                        }
                    />
                </div>
                <div className="min-w-0 space-y-1.5">
                    <Label htmlFor={`${prefix}-service-time`}>
                        服务时间 <RequiredMark />
                    </Label>
                    <DateTimeRangeLocalPicker
                        id={`${prefix}-service-time`}
                        disabled={disabled}
                        showTimeZone={false}
                        value={{
                            from: draft.startedAt || undefined,
                            to: draft.endedAt || undefined,
                        }}
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
                <div className="min-w-0 space-y-1.5 @[480px]/fulfillment-task:col-span-2">
                    <Label htmlFor={`${prefix}-service-location`}>
                        服务地点 <RequiredMark />
                    </Label>
                    <Input
                        id={`${prefix}-service-location`}
                        value={draft.serviceLocation}
                        disabled={disabled}
                        placeholder="客户现场或安装地址"
                        onChange={(event) =>
                            onChange({
                                ...draft,
                                serviceLocation: event.target.value,
                            })
                        }
                    />
                </div>
            </div>
            <div id={`${prefix}-evidence`} className="min-w-0 space-y-1.5">
                <Label htmlFor={`${prefix}-evidence-input`}>
                    图片凭证 <RequiredMark />
                </Label>
                <FileUpload
                    idPrefix={`${prefix}-evidence`}
                    maxFileSize={5 * 1024 * 1024}
                    selectedDescription={fileUploadText.pendingSubmit}
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
            <ReasonTextareaField
                id={SERVICE_NOTE_ID}
                mode="utf8-bytes"
                max={SERVICE_NOTE_MAX_BYTES}
                label="完成说明"
                required
                disabled={disabled}
                value={draft.completionNote}
                error={issue?.message}
                placeholder={
                    draft.result === "FAILURE"
                        ? "例如：客户不在现场，未能完成安装"
                        : "例如：已上门安装并完成现场验收"
                }
                onChange={(completionNote) =>
                    onChange({ ...draft, completionNote })
                }
            />
        </section>
    )
}
