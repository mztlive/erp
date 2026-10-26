import { fileUploadText } from "@/lib/ui-text"

const extensionsByMime: Readonly<Record<string, readonly string[]>> = {
    "application/pdf": [".pdf"],
    "image/jpeg": [".jpg", ".jpeg"],
    "image/png": [".png"],
    "image/webp": [".webp"],
    "application/vnd.openxmlformats-officedocument.wordprocessingml.document": [
        ".docx",
    ],
}

export function formatFileSize(size: number): string {
    if (size < 1024 * 1024) return `${Math.ceil(size / 1024)} KB`
    return `${(size / (1024 * 1024)).toFixed(1)} MB`
}

/** 仅用于选择阶段提示；业务校验及服务端内容校验仍由原流程执行。 */
export function fileSelectionError(
    files: readonly File[],
    options: { accept?: string; multiple?: boolean; maxFileSize?: number },
): string | null {
    if (options.multiple === false && files.length > 1) {
        return fileUploadText.singleOnly
    }
    const accepted = options.accept
        ?.toLowerCase()
        .split(",")
        .map((item) => item.trim())
        .filter(Boolean)
    for (const file of files) {
        if (file.size === 0) return `${file.name}：文件为空，请重新选择`
        const name = file.name.toLowerCase()
        const type = file.type.toLowerCase()
        const supported =
            !accepted?.length ||
            accepted.some((item) => {
                if (item.startsWith(".")) return name.endsWith(item)
                if (item.endsWith("/*"))
                    return type.startsWith(item.slice(0, -1))
                return (
                    type === item ||
                    (!type &&
                        extensionsByMime[item]?.some((extension) =>
                            name.endsWith(extension),
                        ))
                )
            })
        if (!supported) return `${file.name}：${fileUploadText.unsupported}`
        if (
            options.maxFileSize !== undefined &&
            file.size > options.maxFileSize
        ) {
            return `${file.name}：单个文件不能超过 ${formatFileSize(options.maxFileSize)}`
        }
    }
    return null
}

export function selectedFileKey(file: File): string {
    return `${encodeURIComponent(file.name)}:${file.size}:${file.lastModified}`
}

/** 文件名作为业务引用时，拒绝整批中的重名，避免覆盖已有文件。 */
export function fileNameConflictError(
    files: readonly File[],
    existingNames: readonly string[],
): string | null {
    const names = new Set(existingNames)
    for (const file of files) {
        if (names.has(file.name)) {
            return `「${file.name}」已存在同名文件，请使用不同文件名后重新添加。`
        }
        names.add(file.name)
    }
    return null
}
