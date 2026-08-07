"use client"

/* eslint-disable @next/next/no-img-element -- 本地预览包含 blob URL，不能使用 Next Image 优化器。 */

import * as React from "react"
import {
  FileUpIcon,
  ImageIcon,
  Maximize2Icon,
  UploadCloudIcon,
  XIcon,
} from "lucide-react"

import { Button } from "@/components/ui/button"
import {
  Dialog,
  DialogContent,
  DialogDescription,
  DialogHeader,
  DialogTitle,
} from "@/components/ui/dialog"
import { cn } from "@/lib/utils"

type FileUploadImagePreview = Readonly<{
  /** 已上传图片的可访问 URL；不传时以文件名占位回显。 */
  src?: string
  name: string
  status?: "uploaded" | "pending"
}>

/** 仅把浏览器可直接访问的字符串识别为图片预览源。 */
function imagePreviewSource(value?: string | null): string | undefined {
  const source = value?.trim()
  if (!source) return undefined
  return /^(https?:\/\/|\/|blob:|data:image\/)/i.test(source)
    ? source
    : undefined
}

function FileUpload({
  onFilesSelected,
  accept,
  multiple = true,
  disabled = false,
  label = "上传文件",
  description = "点击选择文件，或将文件拖到此处",
  density = "default",
  preview,
  previewSelectedImage = false,
  onPreviewRemove,
  className,
}: {
  onFilesSelected: (files: File[]) => void
  accept?: string
  multiple?: boolean
  disabled?: boolean
  label?: React.ReactNode
  description?: React.ReactNode
  density?: "default" | "compact" | "tile"
  /** 已上传图片回显；src 必须是浏览器可访问 URL。 */
  preview?: FileUploadImagePreview | null
  /** 选择单张本地图片后立即生成 blob URL 预览。 */
  previewSelectedImage?: boolean
  /** 移除已上传或待上传预览；调用方负责同步清空表单值。 */
  onPreviewRemove?: () => void
  className?: string
}) {
  const inputRef = React.useRef<HTMLInputElement>(null)
  const [dragging, setDragging] = React.useState(false)
  const [previewOpen, setPreviewOpen] = React.useState(false)
  const [localPreview, setLocalPreview] =
    React.useState<FileUploadImagePreview | null>(null)
  const localPreviewUrlRef = React.useRef<string | null>(null)

  const clearLocalPreview = React.useCallback(() => {
    if (localPreviewUrlRef.current) {
      URL.revokeObjectURL(localPreviewUrlRef.current)
      localPreviewUrlRef.current = null
    }
    setLocalPreview(null)
  }, [])

  React.useEffect(
    () => () => {
      if (localPreviewUrlRef.current) {
        URL.revokeObjectURL(localPreviewUrlRef.current)
        localPreviewUrlRef.current = null
      }
    },
    [],
  )

  const selectFiles = (files: FileList | null) => {
    if (disabled || !files?.length) return
    const selectedFiles = Array.from(files)
    if (previewSelectedImage) {
      const image = selectedFiles.find((file) => file.type.startsWith("image/"))
      if (image) {
        clearLocalPreview()
        const src = URL.createObjectURL(image)
        localPreviewUrlRef.current = src
        setLocalPreview({ src, name: image.name, status: "pending" })
      }
    }
    onFilesSelected(selectedFiles)
  }

  const activePreview = localPreview ?? preview ?? null
  const previewStatus =
    activePreview?.status === "pending" ? "待上传" : "已上传"

  const removePreview = () => {
    setPreviewOpen(false)
    clearLocalPreview()
    onPreviewRemove?.()
  }

  return (
    <>
      <div
        data-slot="file-upload"
        data-dragging={dragging || undefined}
        className={cn(
          "flex min-w-0 items-center justify-center rounded-lg border border-dashed bg-surface-sunken transition-colors data-dragging:border-ring data-dragging:bg-accent",
          density === "tile"
            ? "flex-col gap-1 p-1 text-center"
            : density === "compact"
              ? "flex-row gap-2 p-2 text-left"
              : "flex-col gap-3 p-6 text-center",
          activePreview && "relative overflow-hidden p-0",
          disabled && "cursor-not-allowed opacity-50",
          className,
        )}
        onDragEnter={(event) => {
          event.preventDefault()
          if (!disabled) setDragging(true)
        }}
        onDragOver={(event) => event.preventDefault()}
        onDragLeave={(event) => {
          if (
            !event.currentTarget.contains(event.relatedTarget as Node | null)
          ) {
            setDragging(false)
          }
        }}
        onDrop={(event) => {
          event.preventDefault()
          setDragging(false)
          selectFiles(event.dataTransfer.files)
        }}
      >
        <input
          ref={inputRef}
          className="sr-only"
          type="file"
          accept={accept}
          multiple={multiple}
          disabled={disabled}
          onChange={(event) => {
            selectFiles(event.target.files)
            event.target.value = ""
          }}
          aria-label={typeof label === "string" ? label : "选择上传文件"}
        />

        {activePreview ? (
          <>
            <button
              type="button"
              className={cn(
                "group relative flex w-full items-center justify-center overflow-hidden bg-muted focus-visible:outline-none focus-visible:ring-2 focus-visible:ring-ring focus-visible:ring-inset",
                density === "tile"
                  ? "size-full"
                  : density === "compact"
                    ? "min-h-24"
                    : "min-h-40",
                activePreview.src && "cursor-zoom-in",
              )}
              aria-label={
                activePreview.src
                  ? `放大预览 ${activePreview.name}`
                  : activePreview.name
              }
              onClick={() => {
                if (activePreview.src) setPreviewOpen(true)
              }}
            >
              {activePreview.src ? (
                <img
                  src={activePreview.src}
                  alt={activePreview.name}
                  className="absolute inset-0 size-full object-cover"
                />
              ) : (
                <div className="flex min-w-0 flex-col items-center gap-1 p-2 text-muted-foreground">
                  <ImageIcon className="size-5" aria-hidden="true" />
                  <span className="line-clamp-2 break-all text-2xs leading-tight">
                    {activePreview.name}
                  </span>
                </div>
              )}
              {activePreview.src ? (
                <span className="absolute inset-0 flex items-center justify-center bg-overlay/0 text-primary-foreground opacity-0 transition group-hover:bg-overlay/50 group-hover:opacity-100 group-focus-visible:bg-overlay/50 group-focus-visible:opacity-100">
                  <Maximize2Icon className="size-4" aria-hidden="true" />
                </span>
              ) : null}
            </button>
            <span className="absolute bottom-1 left-1 rounded-md bg-background/90 px-1 text-2xs font-medium text-foreground shadow-xs">
              {previewStatus}
            </span>
            {localPreview || onPreviewRemove ? (
              <Button
                type="button"
                variant="secondary"
                size="icon-xs"
                className="absolute right-1 top-1 size-5"
                disabled={disabled}
                aria-label={`移除 ${activePreview.name}`}
                onClick={removePreview}
              >
                <XIcon className="size-3" />
              </Button>
            ) : null}
          </>
        ) : (
          <>
            {density !== "tile" ? (
              <div
                className={cn(
                  "flex shrink-0 items-center justify-center bg-muted text-foreground",
                  density === "compact"
                    ? "size-8 rounded-lg"
                    : "size-10 rounded-xl",
                )}
              >
                <UploadCloudIcon
                  className={density === "compact" ? "size-4" : "size-5"}
                  aria-hidden="true"
                />
              </div>
            ) : null}
            <div
              className={cn(
                density === "compact" && "min-w-0 flex-1",
                density === "tile" && "min-w-0 leading-none",
              )}
            >
              <div
                className={cn(
                  "font-medium text-foreground",
                  density === "tile"
                    ? "truncate text-2xs"
                    : density === "compact"
                      ? "text-xs"
                      : "text-sm",
                )}
              >
                {label}
              </div>
              <div
                className={cn(
                  "text-xs text-muted-foreground",
                  density === "tile"
                    ? "sr-only"
                    : density === "compact"
                      ? "truncate"
                      : "mt-1",
                )}
              >
                {description}
              </div>
            </div>
            <Button
              type="button"
              variant="outline"
              size={
                density === "tile"
                  ? "icon-xs"
                  : density === "compact"
                    ? "xs"
                    : "sm"
              }
              disabled={disabled}
              aria-label={
                density === "tile"
                  ? `选择${typeof label === "string" ? label : "上传文件"}`
                  : undefined
              }
              onClick={() => inputRef.current?.click()}
            >
              <FileUpIcon
                data-icon={density === "tile" ? undefined : "inline-start"}
                aria-hidden="true"
              />
              {density === "tile" ? (
                <span className="sr-only">选择文件</span>
              ) : density === "compact" ? (
                "选择"
              ) : (
                "选择文件"
              )}
            </Button>
          </>
        )}
      </div>

      {activePreview?.src ? (
        <Dialog open={previewOpen} onOpenChange={setPreviewOpen}>
          <DialogContent className="gap-4 p-4 sm:max-w-4xl">
            <DialogHeader className="pr-10">
              <DialogTitle>{activePreview.name}</DialogTitle>
              <DialogDescription>{previewStatus} · 图片预览</DialogDescription>
            </DialogHeader>
            <div className="flex min-h-0 items-center justify-center overflow-hidden rounded-lg bg-surface-sunken">
              <img
                src={activePreview.src}
                alt={activePreview.name}
                className="max-h-[75dvh] max-w-full object-contain"
              />
            </div>
          </DialogContent>
        </Dialog>
      ) : null}
    </>
  )
}

export {
  FileUpload,
  imagePreviewSource,
  type FileUploadImagePreview,
}
