"use client"

import * as React from "react"
import { ImageIcon, UploadIcon } from "lucide-react"

import { Button } from "@/components/ui/button"
import { imagePreviewSource } from "@/components/ui/file-upload"
import { Label } from "@/components/ui/label"

export function BrandLogoField({
    id,
    value,
    previewUrl,
    disabled,
    onChange,
    onFilesSelected,
}: {
    id: string
    value: string
    previewUrl: string
    disabled: boolean
    onChange: (value: string) => void
    onFilesSelected: (files: File[]) => void
}) {
    const inputRef = React.useRef<HTMLInputElement>(null)
    const [file, setFile] = React.useState<File | null>(null)
    const [localPreview, setLocalPreview] = React.useState("")

    React.useEffect(() => {
        if (!file) {
            setLocalPreview("")
            return
        }
        const url = URL.createObjectURL(file)
        setLocalPreview(url)
        return () => URL.revokeObjectURL(url)
    }, [file])

    const src = value
        ? localPreview || previewUrl || imagePreviewSource(value)
        : undefined

    const selectFile = (selected?: File) => {
        if (!selected || disabled) return
        setFile(selected)
        onFilesSelected([selected])
        onChange(selected.name)
    }

    return (
        <div
            id={`${id}-dropzone`}
            className="flex min-w-0 items-center gap-4 rounded-xl border border-border bg-surface-sunken/50 p-4 focus-within:border-ring"
            onDragOver={(event) => event.preventDefault()}
            onDrop={(event) => {
                event.preventDefault()
                selectFile(event.dataTransfer.files[0])
            }}
        >
            <div className="flex size-20 shrink-0 flex-col items-center justify-center gap-1.5 overflow-hidden rounded-lg border border-border bg-background sm:size-24">
                {src ? (
                    // eslint-disable-next-line @next/next/no-img-element -- 品牌图片支持本地 blob 预览。
                    <img
                        src={src}
                        alt="品牌 Logo 预览"
                        className="size-full object-contain p-2"
                    />
                ) : (
                    <>
                        <ImageIcon
                            className="size-6 text-muted-foreground/60"
                            aria-hidden
                        />
                        <span className="text-2xs text-muted-foreground">
                            {value ? "暂无预览" : "暂无 Logo"}
                        </span>
                    </>
                )}
            </div>
            <div className="min-w-0 flex-1 space-y-2">
                <div className="flex flex-wrap items-baseline gap-2">
                    <Label htmlFor={`${id}-input`}>品牌 Logo</Label>
                    <span className="text-xs text-muted-foreground">选填</span>
                </div>
                <p
                    id={`${id}-hint`}
                    className="text-xs leading-relaxed text-muted-foreground"
                >
                    建议使用 1:1 方形图片，支持 JPG、PNG、WebP。
                </p>
                {value ? (
                    <p
                        className="truncate text-xs text-muted-foreground"
                        title={value}
                    >
                        {value}
                    </p>
                ) : null}
                <input
                    ref={inputRef}
                    id={`${id}-input`}
                    type="file"
                    accept="image/jpeg,image/png,image/webp"
                    className="sr-only"
                    disabled={disabled}
                    aria-describedby={`${id}-hint`}
                    onChange={(event) => {
                        selectFile(event.target.files?.[0])
                        event.target.value = ""
                    }}
                />
                <div className="flex flex-wrap gap-2">
                    <Button
                        id={`${id}-select`}
                        type="button"
                        variant="outline"
                        size="sm"
                        disabled={disabled}
                        onClick={() => inputRef.current?.click()}
                    >
                        <UploadIcon data-icon="inline-start" aria-hidden />
                        {value ? "更换图片" : "上传图片"}
                    </Button>
                    {value ? (
                        <Button
                            id={`${id}-remove`}
                            type="button"
                            variant="ghost"
                            size="sm"
                            disabled={disabled}
                            onClick={() => {
                                setFile(null)
                                onChange("")
                            }}
                        >
                            移除
                        </Button>
                    ) : null}
                </div>
            </div>
        </div>
    )
}
