import type {
    SupplierEditorFormValues,
    SupplierFieldKey,
} from "@/features/master-data/lib/supplier-editor-model"

/** 分区组件的公共 props 类型；字段读写与敏感信息处理全部由编辑器上层注入。 */
export type SupplierSetFieldValue = (
    key: SupplierFieldKey,
    next: string,
) => void

export type SupplierSensitiveInfo = {
    maskedValue: string
    revealToken?: string
}

export type SupplierMediaLookup = (
    fieldKey: string,
) => Readonly<Record<string, string>>

export type SupplierRememberMediaFiles = (files: File[]) => void

export type SupplierRefreshRevealToken = (
    labels: readonly string[],
) => Promise<string | undefined>

export type SupplierEditorSectionProps = {
    values: SupplierEditorFormValues
    setFieldValue: SupplierSetFieldValue
    canEdit: boolean
}
