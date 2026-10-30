"use client"

import { useFieldContext } from "@/components/form/form-context"
import { toFieldErrors } from "@/components/form/utils"
import {
    Field,
    FieldDescription,
    FieldError,
    FieldLabel,
} from "@/components/ui/field"
import { FileUpload } from "@/components/ui/file-upload"
import {
    JD_VOP_PRIVATE_KEY_MAX_BYTES,
    jdVopPrivateKeyFileError,
} from "../../lib/jd-vop-private-key"

/** 只选择本次授权的本地私钥，不预览密钥内容或登记文件资产。 */
export function JdVopPrivateKeyField({
    id,
    disabled,
}: {
    id: string
    disabled: boolean
}) {
    const field = useFieldContext<File | null>()
    const invalid = field.state.meta.isTouched && !field.state.meta.isValid
    const descriptionId = `${id}-description`
    const errorId = `${id}-error`
    return (
        <Field data-invalid={invalid || undefined}>
            <FieldLabel htmlFor={`${id}-input`}>
                RSA 私钥文件 <span className="text-destructive">*</span>
            </FieldLabel>
            <FileUpload
                idPrefix={id}
                multiple={false}
                maxFileSize={JD_VOP_PRIVATE_KEY_MAX_BYTES}
                disabled={disabled}
                selectedFiles={field.state.value ? [field.state.value] : []}
                label="选择私钥文件"
                description="选择或拖入 PEM、KEY、TXT 或无扩展名的私钥文本"
                selectedDescription="仅供本次账号授权使用"
                validateFiles={(files) =>
                    files[0] ? jdVopPrivateKeyFileError(files[0]) : null
                }
                onFileRemove={() => field.handleChange(null)}
                onFilesSelected={(files) => {
                    field.handleChange(files[0] ?? null)
                    field.handleBlur()
                }}
                aria-invalid={invalid || undefined}
                aria-describedby={`${descriptionId}${invalid ? ` ${errorId}` : ""}`}
            />
            <FieldDescription id={descriptionId}>
                支持 PEM 或纯 Base64 内容，单个文件不超过 32
                KB。私钥只用于本次授权，关闭窗口后清空。
            </FieldDescription>
            {invalid ? (
                <FieldError
                    id={errorId}
                    errors={toFieldErrors(field.state.meta.errors)}
                />
            ) : null}
        </Field>
    )
}
