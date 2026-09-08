import { execSync } from "node:child_process"

import type { BrowserContextOptions, Page } from "@playwright/test"

/**
 * 是否有界面运行。Playwright 在加载 config 之后才合并 --headed，
 * 所以这里读 argv / E2E_HEADED / PWDEBUG。
 */
export function isHeadedRun(): boolean {
    if (process.argv.includes("--headless")) {
        return false
    }
    return (
        process.argv.includes("--headed") ||
        process.argv.includes("--ui") ||
        process.argv.includes("--debug") ||
        process.env.E2E_HEADED === "1" ||
        Boolean(process.env.PWDEBUG)
    )
}

/** 有头时交给系统窗口尺寸；无头保持固定 viewport，截图可复现。 */
export function headedAwareViewport(size: { width: number; height: number }): {
    viewport: { width: number; height: number } | null
} {
    return { viewport: isHeadedRun() ? null : size }
}

export function headedContextOptions(): Pick<BrowserContextOptions, "viewport"> | Record<never, never> {
    return isHeadedRun() ? { viewport: null } : {}
}

/**
 * Chromium：Linux/Windows 认 --start-maximized；macOS 会忽略该参数，
 * 再补 window-size / window-position，避免有头窗口停在默认小尺寸。
 */
export function headedChromeArgs(): string[] {
    const args = ["--start-maximized"]
    if (process.platform === "darwin") {
        const size = macDesktopSize()
        if (size) {
            args.push(
                `--window-size=${size.width},${size.height}`,
                "--window-position=0,0",
            )
        }
    }
    return args
}

export async function maximizePageIfHeaded(page: Page): Promise<void> {
    if (!isHeadedRun()) {
        return
    }
    try {
        const session = await page.context().newCDPSession(page)
        const { windowId } = await session.send("Browser.getWindowForTarget")
        await session.send("Browser.setWindowBounds", {
            windowId,
            bounds: { windowState: "maximized" },
        })
        if (process.platform === "darwin") {
            const size = macDesktopSize()
            if (size) {
                await session.send("Browser.setWindowBounds", {
                    windowId,
                    bounds: {
                        windowState: "normal",
                        left: 0,
                        top: 0,
                        width: size.width,
                        height: size.height,
                    },
                })
            }
        }
    } catch {
        // 非 Chromium、无窗口或 CDP 不可用时忽略
    }
}

function macDesktopSize(): { width: number; height: number } | undefined {
    try {
        const raw = execSync(
            `osascript -e 'tell application "Finder" to get bounds of window of desktop'`,
            { encoding: "utf8" },
        )
        const parts = raw.split(",").map((part) => Number.parseInt(part.trim(), 10))
        if (parts.length === 4 && parts.every((value) => Number.isFinite(value))) {
            const width = parts[2] - parts[0]
            const height = parts[3] - parts[1]
            if (width > 0 && height > 0) {
                return { width, height }
            }
        }
    } catch {
        return undefined
    }
    return undefined
}
