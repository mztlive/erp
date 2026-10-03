"use client"

import { z } from "zod"
import { useAppForm } from "@/components/form"
import { toFieldErrors } from "@/components/form/utils"
import { Button } from "@/components/ui/button"
import {
    Dialog,
    DialogContent,
    DialogDescription,
    DialogHeader,
    DialogTitle,
} from "@/components/ui/dialog"
import { Field, FieldError, FieldLabel } from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import { CompanySearchCombobox } from "@/features/companies/company-search-combobox"
import { getErrorMessage } from "@/lib/api/errors"
import {
    DOCX_MIME,
    NUMBER_GROUP_OPTIONS,
    type NumberGroup,
} from "../api/templates"
import { useTemplateMutations } from "../hooks/template-queries"

export function TemplateUploadDialog({ onClose }: { onClose: () => void }) {
    const { upload } = useTemplateMutations()
    const form = useAppForm({
        defaultValues: {
            name: "",
            companyId: "",
            group: "FSY",
            file: null as File | null,
        },
        validators: {
            onSubmit: z.object({
                name: z.string().trim().min(1, "请输入模板名称").max(256),
                companyId: z.string().min(1, "请选择公司主体"),
                group: z.enum(["FSY", "ZHYF", "GYL", "BDKJ"]),
                file: z
                    .custom<File | null>()
                    .refine(
                        (file) =>
                            Boolean(
                                file &&
                                file.name.toLowerCase().endsWith(".docx") &&
                                file.size > 0 &&
                                file.size <= 20 * 1024 * 1024,
                            ),
                        "请选择不超过 20 MB 的 Word（DOCX）模板",
                    ),
            }),
        },
        onSubmit: async ({ value }) => {
            if (!value.file) return
            try {
                await upload.mutateAsync({
                    name: value.name.trim(),
                    company_id: value.companyId,
                    group: value.group as NumberGroup,
                    file: value.file,
                })
                onClose()
            } catch {
                /* 错误由 mutation 渲染，表单内容保留。 */
            }
        },
    })
    return (
        <Dialog
            open
            onOpenChange={(open) => {
                if (!open && !upload.isPending) onClose()
            }}
        >
            <DialogContent
                id="template-upload-dialog"
                closeButtonId="template-upload-dismiss"
                className="sm:max-w-xl"
            >
                <DialogHeader>
                    <DialogTitle>上传合同模板</DialogTitle>
                    <DialogDescription>
                        选择我方签约公司及编号组。同一公司的后续模板沿用首次绑定的编号组；替换模板时上传新文件，并停用旧模板。
                    </DialogDescription>
                </DialogHeader>
                <form
                    className="space-y-4"
                    onSubmit={(event) => {
                        event.preventDefault()
                        void form.handleSubmit()
                    }}
                >
                    <form.AppField name="name">
                        {(field) => (
                            <field.TextField
                                id="template-upload-name"
                                label="模板名称"
                                required
                                disabled={upload.isPending}
                                placeholder="例如：实物采购合同"
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="companyId">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="template-upload-company">
                                    签约公司
                                </FieldLabel>
                                <CompanySearchCombobox
                                    id="template-upload-company"
                                    value={field.state.value}
                                    onValueChange={(value) => {
                                        field.handleChange(value ?? "")
                                        field.handleBlur()
                                    }}
                                    disabled={upload.isPending}
                                />
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    <form.AppField name="group">
                        {(field) => (
                            <field.SelectField
                                id="template-upload-group"
                                label="合同编号组"
                                options={NUMBER_GROUP_OPTIONS}
                                disabled={upload.isPending}
                                allowClear={false}
                            />
                        )}
                    </form.AppField>
                    <form.AppField name="file">
                        {(field) => (
                            <Field>
                                <FieldLabel htmlFor="template-upload-file-input">
                                    Word 模板
                                </FieldLabel>
                                <FileUpload
                                    idPrefix="template-upload-file"
                                    accept={`${DOCX_MIME},.docx`}
                                    multiple={false}
                                    disabled={upload.isPending}
                                    label={
                                        field.state.value?.name ??
                                        "选择 Word 模板"
                                    }
                                    description="仅支持 DOCX，最大 20 MB。首页页眉右上角须留出编号位置。"
                                    onFilesSelected={(files) => {
                                        field.handleChange(files[0] ?? null)
                                        field.handleBlur()
                                    }}
                                />
                                <FieldError
                                    errors={toFieldErrors(
                                        field.state.meta.errors,
                                    )}
                                />
                            </Field>
                        )}
                    </form.AppField>
                    <p className="text-xs text-muted-foreground">
                        上传后下载编号样张，用 Word
                        检查第一页排版。样张不占流水。
                    </p>
                    {upload.isError && (
                        <p role="alert" className="text-sm text-destructive">
                            {getErrorMessage(
                                upload.error,
                                "模板上传失败，请重试",
                            )}
                        </p>
                    )}
                    <div className="flex justify-end gap-2">
                        <Button
                            id="template-upload-cancel"
                            type="button"
                            variant="outline"
                            disabled={upload.isPending}
                            onClick={onClose}
                        >
                            取消
                        </Button>
                        <form.AppForm>
                            <form.SubmitButton
                                id="template-upload-submit"
                                label="上传模板"
                                pendingLabel="正在上传…"
                            />
                        </form.AppForm>
                    </div>
                </form>
            </DialogContent>
        </Dialog>
    )
}
