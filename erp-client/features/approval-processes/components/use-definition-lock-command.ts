"use client"

import * as React from "react"

import {
    definitionErrorMessage,
    isDefinitionVersionConflict,
    newCommandKey,
} from "../errors"

/**
 * 发布与退役共用的锁版本命令。
 * 打开对话框或锁版本变化时更换命令键；版本冲突后也更换，避免用旧键重放。
 */
export function useDefinitionLockCommand({
    prefix,
    open,
    lockVersion,
    run,
    onOpenChange,
    onDone,
    onConflict,
}: {
    prefix: string
    open: boolean
    lockVersion: string
    run: (commandKey: string) => Promise<unknown>
    onOpenChange: (open: boolean) => void
    onDone: () => void
    onConflict: () => void
}) {
    const [error, setError] = React.useState<string | null>(null)
    const [commandKey, setCommandKey] = React.useState(() =>
        newCommandKey(prefix),
    )

    React.useEffect(() => {
        if (!open) return
        setError(null)
        setCommandKey(newCommandKey(prefix))
    }, [open, lockVersion, prefix])

    const submit = async () => {
        setError(null)
        try {
            await run(commandKey)
            onOpenChange(false)
            onDone()
        } catch (cause) {
            setError(definitionErrorMessage(cause))
            if (isDefinitionVersionConflict(cause)) {
                setCommandKey(newCommandKey(prefix))
                onConflict()
            }
        }
    }

    return { error, submit }
}
